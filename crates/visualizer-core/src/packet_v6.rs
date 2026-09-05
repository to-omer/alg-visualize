//! Generic bounded multipart transport container for plugin-owned V6 payloads.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Exact V6 header size.
pub const PACKET_V6_HEADER_BYTES: usize = 80;
const PACKET_V6_HEADER_BYTES_U16: u16 = 80;
/// Maximum bytes in one transferred packet, including its header.
pub const MAX_PACKET_V6_BYTES: usize = 32 * 1024 * 1024;
/// Maximum bytes in one logical multipart publication.
pub const MAX_PUBLICATION_V6_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of parts in one publication.
pub const MAX_PACKET_V6_PARTS: usize = 16;

const MAGIC: [u8; 4] = *b"AVP6";
const TRANSPORT_VERSION: u16 = 6;
const FLAGS_NONE: u32 = 0;
const MAX_PART_BODY_BYTES: usize = MAX_PACKET_V6_BYTES - PACKET_V6_HEADER_BYTES;

/// Fixed little-endian V6 packet header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketHeaderV6 {
    /// Build-time plugin ordinal.
    pub plugin_ordinal: u32,
    /// Plugin-local payload schema.
    pub payload_schema_version: u32,
    /// Candidate publication identity.
    pub publication_id: u64,
    /// Session generation that owns the publication.
    pub generation: u64,
    /// Zero-based part position.
    pub part_index: u16,
    /// Number of parts in the logical publication.
    pub part_count: u16,
    /// Bytes following this header.
    pub part_bytes: u32,
    /// Bytes in all bodies concatenated by part index.
    pub total_bytes: u32,
    /// SHA-256 of all bodies concatenated by part index.
    pub payload_sha256: [u8; 32],
}

impl PacketHeaderV6 {
    /// Encodes the exact 80-byte little-endian header.
    #[must_use]
    pub fn encode(&self) -> [u8; PACKET_V6_HEADER_BYTES] {
        let mut bytes = [0_u8; PACKET_V6_HEADER_BYTES];
        bytes[0..4].copy_from_slice(&MAGIC);
        bytes[4..6].copy_from_slice(&TRANSPORT_VERSION.to_le_bytes());
        bytes[6..8].copy_from_slice(&PACKET_V6_HEADER_BYTES_U16.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.plugin_ordinal.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.payload_schema_version.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.publication_id.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.generation.to_le_bytes());
        bytes[32..34].copy_from_slice(&self.part_index.to_le_bytes());
        bytes[34..36].copy_from_slice(&self.part_count.to_le_bytes());
        bytes[36..40].copy_from_slice(&self.part_bytes.to_le_bytes());
        bytes[40..44].copy_from_slice(&self.total_bytes.to_le_bytes());
        bytes[44..48].copy_from_slice(&FLAGS_NONE.to_le_bytes());
        bytes[48..80].copy_from_slice(&self.payload_sha256);
        bytes
    }

    /// Decodes and validates an exact V6 header prefix.
    ///
    /// # Errors
    ///
    /// Rejects a short header, wrong magic/version/size, nonzero flags, and
    /// impossible part metadata.
    pub fn decode(bytes: &[u8]) -> Result<Self, PacketV6Error> {
        if bytes.len() < PACKET_V6_HEADER_BYTES {
            return Err(PacketV6Error::HeaderLength);
        }
        if bytes[0..4] != MAGIC {
            return Err(PacketV6Error::Magic);
        }
        if read_u16(bytes, 4) != TRANSPORT_VERSION
            || usize::from(read_u16(bytes, 6)) != PACKET_V6_HEADER_BYTES
        {
            return Err(PacketV6Error::Version);
        }
        if read_u32(bytes, 44) != FLAGS_NONE {
            return Err(PacketV6Error::Flags);
        }
        let part_count = read_u16(bytes, 34);
        let part_index = read_u16(bytes, 32);
        if read_u32(bytes, 8) == 0 {
            return Err(PacketV6Error::ReservedPlugin);
        }
        if part_count == 0
            || usize::from(part_count) > MAX_PACKET_V6_PARTS
            || part_index >= part_count
        {
            return Err(PacketV6Error::PartMetadata);
        }
        let part_bytes = read_u32(bytes, 36);
        let total_bytes = read_u32(bytes, 40);
        if usize::try_from(part_bytes).map_or(true, |size| size > MAX_PART_BODY_BYTES)
            || usize::try_from(total_bytes).map_or(true, |size| size > MAX_PUBLICATION_V6_BYTES)
            || part_bytes > total_bytes
        {
            return Err(PacketV6Error::Limit);
        }
        let mut payload_sha256 = [0_u8; 32];
        payload_sha256.copy_from_slice(&bytes[48..80]);
        Ok(Self {
            plugin_ordinal: read_u32(bytes, 8),
            payload_schema_version: read_u32(bytes, 12),
            publication_id: read_u64(bytes, 16),
            generation: read_u64(bytes, 24),
            part_index,
            part_count,
            part_bytes,
            total_bytes,
            payload_sha256,
        })
    }
}

/// One decoded V6 header and its exact body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketPartV6 {
    /// Validated header.
    pub header: PacketHeaderV6,
    /// Plugin-owned body bytes.
    pub body: Vec<u8>,
}

impl PacketPartV6 {
    /// Concatenates the fixed header and body for transfer.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(PACKET_V6_HEADER_BYTES + self.body.len());
        packet.extend_from_slice(&self.header.encode());
        packet.extend_from_slice(&self.body);
        packet
    }

    /// Decodes one complete transferred packet.
    ///
    /// # Errors
    ///
    /// Rejects header failures and a body length that differs from the header.
    pub fn decode(packet: &[u8]) -> Result<Self, PacketV6Error> {
        if packet.len() > MAX_PACKET_V6_BYTES {
            return Err(PacketV6Error::Limit);
        }
        let header = PacketHeaderV6::decode(packet)?;
        let body = packet
            .get(PACKET_V6_HEADER_BYTES..)
            .ok_or(PacketV6Error::HeaderLength)?;
        if body.len() != usize::try_from(header.part_bytes).map_err(|_| PacketV6Error::Limit)? {
            return Err(PacketV6Error::PartLength);
        }
        Ok(Self {
            header,
            body: body.to_vec(),
        })
    }
}

/// Splits one plugin payload into bounded V6 packet parts.
///
/// # Errors
///
/// Rejects reserved plugin ordinal zero, a zero target size, or an aggregate
/// that exceeds the publication and part-count limits.
pub fn encode_publication_v6(
    plugin_ordinal: u32,
    payload_schema_version: u32,
    publication_id: u64,
    generation: u64,
    payload: &[u8],
    target_part_body_bytes: usize,
) -> Result<Vec<PacketPartV6>, PacketV6Error> {
    if plugin_ordinal == 0 {
        return Err(PacketV6Error::ReservedPlugin);
    }
    if target_part_body_bytes == 0 || target_part_body_bytes > MAX_PART_BODY_BYTES {
        return Err(PacketV6Error::Limit);
    }
    if payload.len() > MAX_PUBLICATION_V6_BYTES {
        return Err(PacketV6Error::Limit);
    }
    let part_count = payload.len().max(1).div_ceil(target_part_body_bytes);
    if part_count > MAX_PACKET_V6_PARTS {
        return Err(PacketV6Error::Limit);
    }
    let part_count_u16 = u16::try_from(part_count).map_err(|_| PacketV6Error::Limit)?;
    let total_bytes = u32::try_from(payload.len()).map_err(|_| PacketV6Error::Limit)?;
    let payload_sha256: [u8; 32] = Sha256::digest(payload).into();
    let mut parts = Vec::with_capacity(part_count);
    for part_index in 0..part_count {
        let start = part_index * target_part_body_bytes;
        let end = payload.len().min(start + target_part_body_bytes);
        let body = payload[start..end].to_vec();
        parts.push(PacketPartV6 {
            header: PacketHeaderV6 {
                plugin_ordinal,
                payload_schema_version,
                publication_id,
                generation,
                part_index: u16::try_from(part_index).map_err(|_| PacketV6Error::Limit)?,
                part_count: part_count_u16,
                part_bytes: u32::try_from(body.len()).map_err(|_| PacketV6Error::Limit)?,
                total_bytes,
                payload_sha256,
            },
            body,
        });
    }
    Ok(parts)
}

/// Validates and joins a complete logical V6 publication.
///
/// Parts may arrive out of order. Every repeated header field, exact part
/// length, aggregate length, part index, and digest is independently checked.
///
/// # Errors
///
/// Rejects missing, duplicate, incompatible, oversized, or corrupted parts.
pub fn assemble_publication_v6(parts: Vec<PacketPartV6>) -> Result<Vec<u8>, PacketV6Error> {
    let expected = parts
        .first()
        .ok_or(PacketV6Error::MissingPart)?
        .header
        .clone();
    if parts.len() != usize::from(expected.part_count) || parts.len() > MAX_PACKET_V6_PARTS {
        return Err(PacketV6Error::MissingPart);
    }
    let mut ordered = BTreeMap::new();
    for part in parts {
        let header = &part.header;
        if header.plugin_ordinal != expected.plugin_ordinal
            || header.payload_schema_version != expected.payload_schema_version
            || header.publication_id != expected.publication_id
            || header.generation != expected.generation
            || header.part_count != expected.part_count
            || header.total_bytes != expected.total_bytes
            || header.payload_sha256 != expected.payload_sha256
        {
            return Err(PacketV6Error::IncompatiblePart);
        }
        if part.body.len()
            != usize::try_from(header.part_bytes).map_err(|_| PacketV6Error::Limit)?
        {
            return Err(PacketV6Error::PartLength);
        }
        if ordered.insert(header.part_index, part.body).is_some() {
            return Err(PacketV6Error::DuplicatePart);
        }
    }
    let capacity = usize::try_from(expected.total_bytes).map_err(|_| PacketV6Error::Limit)?;
    if capacity > MAX_PUBLICATION_V6_BYTES {
        return Err(PacketV6Error::Limit);
    }
    let mut payload = Vec::with_capacity(capacity);
    for part_index in 0..expected.part_count {
        payload.extend_from_slice(ordered.get(&part_index).ok_or(PacketV6Error::MissingPart)?);
        if payload.len() > capacity {
            return Err(PacketV6Error::PartLength);
        }
    }
    if payload.len() != capacity {
        return Err(PacketV6Error::PartLength);
    }
    let digest: [u8; 32] = Sha256::digest(&payload).into();
    if digest != expected.payload_sha256 {
        return Err(PacketV6Error::Digest);
    }
    Ok(payload)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// V6 transport validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PacketV6Error {
    /// Packet does not contain the fixed header.
    #[error("V6 packet header is truncated")]
    HeaderLength,
    /// Magic does not identify V6.
    #[error("V6 packet magic is invalid")]
    Magic,
    /// Transport or header-size revision is unsupported.
    #[error("V6 packet version is invalid")]
    Version,
    /// Reserved flag bits are nonzero.
    #[error("V6 packet flags are invalid")]
    Flags,
    /// Plugin ordinal zero is reserved.
    #[error("V6 packet plugin ordinal zero is reserved")]
    ReservedPlugin,
    /// Part index/count metadata is impossible.
    #[error("V6 packet part metadata is invalid")]
    PartMetadata,
    /// A packet or publication exceeds its hard limit.
    #[error("V6 packet limit exceeded")]
    Limit,
    /// Body length differs from the header or aggregate.
    #[error("V6 packet body length is invalid")]
    PartLength,
    /// A logical publication is incomplete.
    #[error("V6 publication is missing a part")]
    MissingPart,
    /// A logical publication repeats a part index.
    #[error("V6 publication contains a duplicate part")]
    DuplicatePart,
    /// Repeated logical-publication fields disagree.
    #[error("V6 publication parts are incompatible")]
    IncompatiblePart,
    /// Aggregate body digest is invalid.
    #[error("V6 publication digest is invalid")]
    Digest,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0, "fixture hex must have byte pairs");
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).expect("fixture hex is ASCII");
                u8::from_str_radix(pair, 16).expect("fixture hex is lowercase hexadecimal")
            })
            .collect()
    }

    #[test]
    fn fixed_header_offsets_round_trip() {
        let header = PacketHeaderV6 {
            plugin_ordinal: 2,
            payload_schema_version: 3,
            publication_id: 0x0102_0304_0506_0708,
            generation: 0x1112_1314_1516_1718,
            part_index: 1,
            part_count: 2,
            part_bytes: 7,
            total_bytes: 11,
            payload_sha256: [0xa5; 32],
        };
        let encoded = header.encode();

        assert_eq!(&encoded[0..4], b"AVP6");
        assert_eq!(&encoded[4..8], &[6, 0, 80, 0]);
        assert_eq!(&encoded[16..24], &0x0102_0304_0506_0708_u64.to_le_bytes());
        assert_eq!(&encoded[44..48], &[0; 4]);
        assert_eq!(&encoded[48..80], &[0xa5; 32]);
        assert_eq!(PacketHeaderV6::decode(&encoded), Ok(header));
    }

    #[test]
    fn multipart_round_trip_accepts_out_of_order_parts() {
        let payload = b"a bounded deterministic multipart payload";
        let mut parts =
            encode_publication_v6(2, 1, 91, 7, payload, 9).expect("fixture publication is valid");
        parts.reverse();

        assert_eq!(
            assemble_publication_v6(parts),
            Ok(payload.as_slice().to_vec())
        );
    }

    #[test]
    fn v6_bytes_match_the_cross_language_golden_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/contracts/packet-v6.json"))
                .expect("V6 fixture is valid JSON");
        let publication_id = fixture["publicationId"]
            .as_str()
            .expect("publication ID is a string")
            .parse()
            .expect("publication ID is canonical u64");
        let generation = fixture["generation"]
            .as_str()
            .expect("generation is a string")
            .parse()
            .expect("generation is canonical u64");
        let payload = fixture["payloadUtf8"]
            .as_str()
            .expect("payload is a string")
            .as_bytes();
        let parts = encode_publication_v6(
            u32::try_from(
                fixture["pluginOrdinal"]
                    .as_u64()
                    .expect("plugin ordinal is unsigned"),
            )
            .expect("plugin ordinal fits u32"),
            u32::try_from(
                fixture["payloadSchemaVersion"]
                    .as_u64()
                    .expect("payload schema is unsigned"),
            )
            .expect("payload schema fits u32"),
            publication_id,
            generation,
            payload,
            usize::try_from(
                fixture["targetPartBodyBytes"]
                    .as_u64()
                    .expect("part size is unsigned"),
            )
            .expect("part size fits usize"),
        )
        .expect("golden publication is valid");
        let expected = fixture["partsHex"]
            .as_array()
            .expect("fixture parts are an array");

        assert_eq!(parts.len(), expected.len());
        for (part, expected_hex) in parts.iter().zip(expected) {
            assert_eq!(
                part.encode(),
                decode_hex(expected_hex.as_str().expect("part hex is a string"))
            );
        }
    }

    #[test]
    fn empty_payload_is_one_valid_part() {
        let parts = encode_publication_v6(2, 1, 1, 1, &[], 4)
            .expect("empty logical payload is representable");

        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].header.part_count, 1);
        assert_eq!(assemble_publication_v6(parts), Ok(Vec::new()));
    }

    #[test]
    fn duplicate_missing_and_corrupt_parts_are_rejected() {
        let parts = encode_publication_v6(2, 1, 5, 8, b"0123456789", 4)
            .expect("fixture publication is valid");
        assert_eq!(
            assemble_publication_v6(vec![parts[0].clone(), parts[0].clone(), parts[2].clone()]),
            Err(PacketV6Error::DuplicatePart)
        );
        assert_eq!(
            assemble_publication_v6(parts[..2].to_vec()),
            Err(PacketV6Error::MissingPart)
        );

        let mut corrupt = parts;
        corrupt[1].body[0] ^= 0xff;
        assert_eq!(assemble_publication_v6(corrupt), Err(PacketV6Error::Digest));
    }

    #[test]
    fn packet_decode_rejects_reserved_flags_and_wrong_lengths() {
        let part = encode_publication_v6(2, 1, 5, 8, b"abc", 4)
            .expect("fixture publication is valid")
            .remove(0);
        let mut packet = part.encode();
        packet[44] = 1;
        assert_eq!(PacketPartV6::decode(&packet), Err(PacketV6Error::Flags));

        let mut packet = part.encode();
        packet.pop();
        assert_eq!(
            PacketPartV6::decode(&packet),
            Err(PacketV6Error::PartLength)
        );
    }
}
