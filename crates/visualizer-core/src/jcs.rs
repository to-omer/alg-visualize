//! RFC 8785 JSON Canonicalization Scheme boundary.

use std::collections::HashSet;
use std::fmt;
use std::io::Write;

use serde::de::Error as _;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq)]
enum JcsValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

/// Canonical JSON parse or serialization failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum JcsError {
    /// Input is not one complete valid JSON value.
    #[error("invalid JSON at line {line}, column {column}: {message}")]
    InvalidJson {
        /// Stable parser message.
        message: String,
        /// One-based line.
        line: usize,
        /// One-based column.
        column: usize,
    },
    /// Objects must not repeat property names.
    #[error("duplicate JSON property")]
    DuplicateProperty,
    /// I-JSON numbers must be representable by finite IEEE-754 doubles.
    #[error("JSON number is outside the I-JSON safe range")]
    UnsafeNumber,
    /// Writing into the in-memory output unexpectedly failed.
    #[error("canonical JSON serialization failed")]
    Serialization,
}

/// Parses, validates, and canonicalizes one RFC 8785 JSON value.
///
/// # Errors
///
/// Rejects malformed JSON, duplicate properties, lone surrogates, non-finite
/// numbers, and integer tokens outside the exact IEEE-754 range.
pub fn canonicalize(input: &[u8]) -> Result<Vec<u8>, JcsError> {
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = JcsValue::deserialize(&mut deserializer).map_err(|error| map_json_error(&error))?;
    deserializer.end().map_err(|error| map_json_error(&error))?;

    let mut output = Vec::with_capacity(input.len());
    write_value(&value, &mut output)?;
    Ok(output)
}

/// Returns lowercase SHA-256 of canonical JSON bytes.
pub fn sha256_hex(canonical_json: &[u8]) -> String {
    let digest = Sha256::digest(canonical_json);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn map_json_error(error: &serde_json::Error) -> JcsError {
    JcsError::InvalidJson {
        message: error.to_string(),
        line: error.line(),
        column: error.column(),
    }
}

fn write_value(value: &JcsValue, output: &mut Vec<u8>) -> Result<(), JcsError> {
    match value {
        JcsValue::Null => output.extend_from_slice(b"null"),
        JcsValue::Bool(true) => output.extend_from_slice(b"true"),
        JcsValue::Bool(false) => output.extend_from_slice(b"false"),
        JcsValue::I64(value) => write!(output, "{value}").map_err(|_| JcsError::Serialization)?,
        JcsValue::U64(value) => write!(output, "{value}").map_err(|_| JcsError::Serialization)?,
        JcsValue::F64(value) => {
            let mut buffer = ryu_js::Buffer::new();
            output.extend_from_slice(buffer.format(*value).as_bytes());
        }
        JcsValue::String(value) => write_string(value, output)?,
        JcsValue::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output)?;
            }
            output.push(b']');
        }
        JcsValue::Object(properties) => {
            let mut ordered: Vec<_> = properties.iter().collect();
            ordered.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
            output.push(b'{');
            for (index, (key, value)) in ordered.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_string(key, output)?;
                output.push(b':');
                write_value(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn write_string(value: &str, output: &mut Vec<u8>) -> Result<(), JcsError> {
    serde_json::to_writer(output, value).map_err(|_| JcsError::Serialization)
}

impl<'de> Deserialize<'de> for JcsValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(JcsVisitor)
    }
}

struct JcsVisitor;

impl<'de> Visitor<'de> for JcsVisitor {
    type Value = JcsValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an I-JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JcsValue::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JcsValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value.unsigned_abs() > MAX_SAFE_INTEGER {
            return Err(E::custom(JcsError::UnsafeNumber));
        }
        Ok(JcsValue::I64(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value > MAX_SAFE_INTEGER {
            return Err(E::custom(JcsError::UnsafeNumber));
        }
        Ok(JcsValue::U64(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if !value.is_finite() {
            return Err(E::custom(JcsError::UnsafeNumber));
        }
        Ok(JcsValue::F64(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JcsValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JcsValue::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(JcsValue::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut properties = Vec::with_capacity(map.size_hint().unwrap_or(0));
        let mut names = HashSet::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((name, value)) = map.next_entry::<String, JcsValue>()? {
            if !names.insert(name.clone()) {
                return Err(A::Error::custom(JcsError::DuplicateProperty));
            }
            properties.push((name, value));
        }
        Ok(JcsValue::Object(properties))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_rfc_8785_sample() {
        let input = br#"{
          "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
          "string": "\u20ac$\u000f\nA'B\"\\\"/",
          "literals": [null, true, false]
        }"#;
        let expected = concat!(
            r#"{"literals":[null,true,false],"numbers":[333333333.3333333,"#,
            r#"1e+30,4.5,0.002,1e-27],"string":"€$\u000f\nA'B\"\\\"/"}"#,
        );

        assert_eq!(canonicalize(input), Ok(expected.as_bytes().to_vec()));
    }

    #[test]
    fn sorts_property_names_by_utf16_not_utf8() {
        let input = "{\"\u{e000}\":2,\"😀\":1}";

        assert_eq!(
            canonicalize(input.as_bytes()),
            Ok("{\"😀\":1,\"\u{e000}\":2}".as_bytes().to_vec())
        );
    }

    #[test]
    fn rejects_duplicate_properties_before_materialization() {
        let error = canonicalize(br#"{"same":1,"same":2}"#)
            .expect_err("duplicate properties must be rejected");

        assert!(error.to_string().contains("duplicate JSON property"));
    }

    #[test]
    fn rejects_lone_surrogates() {
        assert!(canonicalize(br#"{"value":"\ud800"}"#).is_err());
    }

    #[test]
    fn rejects_integer_outside_exact_ieee754_range() {
        assert!(canonicalize(b"9007199254740992").is_err());
        assert_eq!(
            canonicalize(b"9007199254740991"),
            Ok(b"9007199254740991".to_vec())
        );
    }

    #[test]
    fn digest_is_lowercase_and_stable() {
        assert_eq!(
            sha256_hex(br#"{"a":1}"#),
            "015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
        );
    }

    #[test]
    fn cross_language_fixture_matches_typescript_oracle() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            #[serde(rename = "fixtureRevision")]
            revision: u32,
            input: String,
            canonical: String,
            sha256: String,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/jcs-cross-language.json"
        ))
        .expect("fixture JSON is valid");
        let actual = canonicalize(fixture.input.as_bytes()).expect("fixture is I-JSON");

        assert_eq!(fixture.revision, 1);
        assert_eq!(actual, fixture.canonical.as_bytes());
        assert_eq!(sha256_hex(&actual), fixture.sha256);
    }
}
