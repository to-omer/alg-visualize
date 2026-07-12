//! Strict, line-oriented ordered-map input language.

use serde::Serialize;
use thiserror::Error;

use crate::scenario::{Entry, Operation};

/// Maximum combined byte length accepted for manual initial and operation input.
pub const MAX_DSL_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of initial insert lines.
pub const MAX_INITIAL_LINES: usize = 10_000;
/// Maximum number of operation lines.
pub const MAX_OPERATION_LINES: usize = 100_000;

/// Stable source diagnostic returned to editor integrations.
#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize)]
#[error("{code} at {line}:{column}: {message}")]
pub struct DslError {
    /// Stable machine-readable error code.
    pub code: &'static str,
    /// One-origin source line.
    pub line: usize,
    /// One-origin UTF-16 code-unit column.
    pub column: usize,
    /// Short repair-oriented explanation.
    pub message: &'static str,
}

impl DslError {
    fn at(
        code: &'static str,
        line: usize,
        source: &str,
        byte: usize,
        message: &'static str,
    ) -> Self {
        Self {
            code,
            line,
            column: source[..byte.min(source.len())].encode_utf16().count() + 1,
            message,
        }
    }
}

fn byte_limit_error() -> DslError {
    DslError {
        code: "DSL_BYTE_LIMIT",
        line: 1,
        column: 1,
        message: "manual input exceeds the combined 64 MiB limit",
    }
}

/// Validates the combined byte budget for the two manual DSL streams.
///
/// # Errors
///
/// Returns a resource-limit diagnostic before either stream is parsed.
pub fn validate_document_size(initial: &[u8], operations: &[u8]) -> Result<(), DslError> {
    validate_document_lengths(initial.len(), operations.len())
}

fn validate_document_lengths(initial: usize, operations: usize) -> Result<(), DslError> {
    if initial.saturating_add(operations) > MAX_DSL_BYTES {
        return Err(byte_limit_error());
    }
    Ok(())
}

/// Parses initial-map DSL. Only `insert` statements are accepted.
///
/// # Errors
///
/// Returns the first stable source diagnostic or a resource-limit diagnostic.
pub fn parse_initial(bytes: &[u8]) -> Result<Vec<Entry>, DslError> {
    let lines = source_lines(bytes)?;
    let mut entries = Vec::new();
    for (line_index, source) in lines.iter().enumerate() {
        let Some(statement) = parse_statement(source, line_index + 1)? else {
            continue;
        };
        match statement {
            ParsedStatement::Insert { key, value } => entries.push(Entry { key, value }),
            ParsedStatement::Remove { .. }
            | ParsedStatement::Get { .. }
            | ParsedStatement::LowerBound { .. } => {
                return Err(DslError::at(
                    "INITIAL_INSERT_ONLY",
                    line_index + 1,
                    source,
                    first_non_whitespace(source),
                    "initial input accepts only insert statements",
                ));
            }
        }
        if entries.len() > MAX_INITIAL_LINES {
            return Err(limit_error("INITIAL_LINE_LIMIT", MAX_INITIAL_LINES));
        }
    }
    Ok(entries)
}

/// Parses operation DSL.
///
/// # Errors
///
/// Returns the first stable source diagnostic or a resource-limit diagnostic.
pub fn parse_operations(bytes: &[u8]) -> Result<Vec<Operation>, DslError> {
    let lines = source_lines(bytes)?;
    let mut operations = Vec::new();
    for (line_index, source) in lines.iter().enumerate() {
        let Some(statement) = parse_statement(source, line_index + 1)? else {
            continue;
        };
        operations.push(match statement {
            ParsedStatement::Insert { key, value } => Operation::Insert { key, value },
            ParsedStatement::Remove { key } => Operation::Remove { key },
            ParsedStatement::Get { key } => Operation::Get { key },
            ParsedStatement::LowerBound { key } => Operation::LowerBound { key },
        });
        if operations.len() > MAX_OPERATION_LINES {
            return Err(limit_error("OPERATION_LINE_LIMIT", MAX_OPERATION_LINES));
        }
    }
    Ok(operations)
}

fn limit_error(code: &'static str, limit: usize) -> DslError {
    let (message, line) = if limit == MAX_INITIAL_LINES {
        (
            "initial input exceeds 10,000 statements",
            MAX_INITIAL_LINES + 1,
        )
    } else {
        (
            "operation input exceeds 100,000 statements",
            MAX_OPERATION_LINES + 1,
        )
    };
    DslError {
        code,
        line,
        column: 1,
        message,
    }
}

fn source_lines(bytes: &[u8]) -> Result<Vec<String>, DslError> {
    if bytes.len() > MAX_DSL_BYTES {
        return Err(byte_limit_error());
    }
    let source = std::str::from_utf8(bytes).map_err(|_| DslError {
        code: "INVALID_UTF8",
        line: 1,
        column: 1,
        message: "input must be valid UTF-8",
    })?;
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    if source.contains('\u{feff}') {
        return Err(DslError {
            code: "UNEXPECTED_BOM",
            line: source[..source.find('\u{feff}').unwrap_or(0)]
                .matches('\n')
                .count()
                + 1,
            column: 1,
            message: "a BOM is allowed only once at the start of the input",
        });
    }
    Ok(source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect())
}

enum ParsedStatement {
    Insert { key: String, value: String },
    Remove { key: String },
    Get { key: String },
    LowerBound { key: String },
}

fn parse_statement(source: &str, line: usize) -> Result<Option<ParsedStatement>, DslError> {
    if let Some(byte) = source.find('\r') {
        return Err(DslError::at(
            "INVALID_LINE_ENDING",
            line,
            source,
            byte,
            "use LF or CRLF line endings",
        ));
    }
    let logical_end = comment_start(source);
    let statement = &source[..logical_end];
    let mut cursor = skip_space(statement, 0);
    if cursor == statement.len() {
        return Ok(None);
    }
    let operation_start = cursor;
    cursor = token_end(statement, cursor);
    let operation = &statement[operation_start..cursor];
    cursor = skip_space(statement, cursor);
    if cursor == statement.len() {
        return Err(DslError::at(
            "MISSING_KEY",
            line,
            source,
            cursor,
            "add a canonical unsigned decimal key",
        ));
    }
    let key_start = cursor;
    cursor = token_end(statement, cursor);
    let key = &statement[key_start..cursor];
    validate_key(key)
        .map_err(|(code, message)| DslError::at(code, line, source, key_start, message))?;
    cursor = skip_space(statement, cursor);

    match operation {
        "insert" => {
            if cursor == statement.len() {
                return Err(DslError::at(
                    "MISSING_VALUE",
                    line,
                    source,
                    cursor,
                    "insert requires a JSON string value",
                ));
            }
            let encoded = statement[cursor..].trim_end_matches([' ', '\t']);
            let value: String = serde_json::from_str(encoded).map_err(|_| {
                DslError::at(
                    "INVALID_VALUE",
                    line,
                    source,
                    cursor,
                    "value must be one JSON string literal without a newline",
                )
            })?;
            if value.chars().count() > 256 {
                return Err(DslError::at(
                    "VALUE_SCALAR_LIMIT",
                    line,
                    source,
                    cursor,
                    "decoded value must contain at most 256 Unicode scalar values",
                ));
            }
            Ok(Some(ParsedStatement::Insert {
                key: key.to_owned(),
                value,
            }))
        }
        "remove" | "get" | "lower_bound" => {
            if cursor != statement.len() {
                return Err(DslError::at(
                    "TRAILING_INPUT",
                    line,
                    source,
                    cursor,
                    "remove, get, and lower_bound accept only a key",
                ));
            }
            let key = key.to_owned();
            Ok(Some(match operation {
                "remove" => ParsedStatement::Remove { key },
                "get" => ParsedStatement::Get { key },
                _ => ParsedStatement::LowerBound { key },
            }))
        }
        _ => Err(DslError::at(
            "UNKNOWN_OPERATION",
            line,
            source,
            operation_start,
            "use insert, remove, get, or lower_bound",
        )),
    }
}

fn validate_key(key: &str) -> Result<(), (&'static str, &'static str)> {
    if key.is_empty()
        || (key.len() > 1 && key.starts_with('0'))
        || !key.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(("NONCANONICAL_KEY", "key must match 0|[1-9][0-9]{0,19}"));
    }
    if key.len() > 20 || key.parse::<u64>().is_err() {
        return Err((
            "KEY_OUT_OF_RANGE",
            "key must fit in an unsigned 64-bit integer",
        ));
    }
    Ok(())
}

fn comment_start(source: &str) -> usize {
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in source.bytes().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b'#' {
            return index;
        }
    }
    source.len()
}

fn first_non_whitespace(source: &str) -> usize {
    skip_space(source, 0)
}

fn skip_space(source: &str, mut cursor: usize) -> usize {
    while matches!(source.as_bytes().get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    cursor
}

fn token_end(source: &str, mut cursor: usize) -> usize {
    while !matches!(source.as_bytes().get(cursor), None | Some(b' ' | b'\t')) {
        cursor += 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bom_crlf_comments_and_json_escapes() {
        let input = "\u{feff}# initial\r\ninsert 0 \"line\\t#value\" # comment\r\ninsert 18446744073709551615 \"🦀\"\r\n";
        let entries = parse_initial(input.as_bytes()).expect("valid initial DSL");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].value, "line\t#value");
        assert_eq!(entries[1].key, u64::MAX.to_string());
    }

    #[test]
    fn operation_grammar_and_initial_mode_are_distinct() {
        let operations = parse_operations(b"insert 8 \"eight\"\nget 8\nlower_bound 7\nremove 8\n")
            .expect("valid operation DSL");
        assert_eq!(operations.len(), 4);
        assert_eq!(
            parse_initial(b"get 1")
                .expect_err("initial get is invalid")
                .code,
            "INITIAL_INSERT_ONLY"
        );
    }

    #[test]
    fn diagnostics_use_utf16_columns_and_stable_codes() {
        let error = parse_operations("  🦀 1".as_bytes()).expect_err("unknown operation");
        assert_eq!(error.code, "UNKNOWN_OPERATION");
        assert_eq!((error.line, error.column), (1, 3));

        let error = parse_operations(b"insert 01 \"x\"").expect_err("leading zero");
        assert_eq!(error.code, "NONCANONICAL_KEY");

        let error = parse_operations(b"get 18446744073709551616").expect_err("overflow");
        assert_eq!(error.code, "KEY_OUT_OF_RANGE");
    }

    #[test]
    fn rejects_unescaped_or_trailing_value_input() {
        assert_eq!(
            parse_operations(b"insert 1 value")
                .expect_err("value must be JSON")
                .code,
            "INVALID_VALUE"
        );
        assert_eq!(
            parse_operations(b"get 1 extra")
                .expect_err("extra token")
                .code,
            "TRAILING_INPUT"
        );
    }

    #[test]
    fn combined_document_budget_is_checked_without_overflow() {
        assert_eq!(
            validate_document_lengths(MAX_DSL_BYTES / 2, MAX_DSL_BYTES / 2),
            Ok(())
        );
        assert_eq!(
            validate_document_lengths(MAX_DSL_BYTES, 1),
            Err(byte_limit_error())
        );
        assert_eq!(
            validate_document_lengths(usize::MAX, usize::MAX),
            Err(byte_limit_error())
        );
    }
}
