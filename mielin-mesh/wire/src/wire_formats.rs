//! Wire Serialization Formats
//!
//! Format-agnostic serialization with support for Bincode (oxicode), JSON, and
//! Postcard (embedded-optimized, COBS-framed). Includes format negotiation to
//! select the best mutually-supported encoding between endpoints.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Format tag bytes prepended to prefixed payloads ────────────────────────

const TAG_BINCODE: u8 = 0x01;
const TAG_JSON: u8 = 0x02;
const TAG_POSTCARD: u8 = 0x03;

// ─── WireFormat ─────────────────────────────────────────────────────────────

/// Available wire serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum WireFormat {
    /// oxicode (bincode-like) — default, most compact for complex / nested types.
    #[default]
    Bincode,
    /// JSON — human-readable, widely interoperable with non-Rust peers.
    Json,
    /// Postcard — embedded-optimized, `no_std`-friendly, COBS-framed output.
    Postcard,
}

impl WireFormat {
    /// Returns `true` for binary formats (Bincode, Postcard).
    pub fn is_binary(&self) -> bool {
        matches!(self, WireFormat::Bincode | WireFormat::Postcard)
    }

    /// Returns `true` for formats whose encoded bytes are valid UTF-8 text.
    pub fn is_human_readable(&self) -> bool {
        matches!(self, WireFormat::Json)
    }

    /// Typical per-message framing / structural overhead in bytes.
    ///
    /// This is a rough heuristic used when choosing among formats; actual
    /// sizes depend entirely on the payload shape.
    pub fn typical_overhead_bytes(&self) -> usize {
        match self {
            WireFormat::Bincode => 8,
            WireFormat::Json => 64,
            WireFormat::Postcard => 4,
        }
    }

    /// Human-readable name for logging / diagnostics.
    pub fn name(&self) -> &'static str {
        match self {
            WireFormat::Bincode => "bincode",
            WireFormat::Json => "json",
            WireFormat::Postcard => "postcard",
        }
    }

    /// The 1-byte format tag embedded as the first byte of prefixed payloads.
    fn tag_byte(self) -> u8 {
        match self {
            WireFormat::Bincode => TAG_BINCODE,
            WireFormat::Json => TAG_JSON,
            WireFormat::Postcard => TAG_POSTCARD,
        }
    }

    /// Recover a `WireFormat` from its tag byte, returning an error for unknown tags.
    fn from_tag_byte(byte: u8) -> Result<Self, FormatError> {
        match byte {
            TAG_BINCODE => Ok(WireFormat::Bincode),
            TAG_JSON => Ok(WireFormat::Json),
            TAG_POSTCARD => Ok(WireFormat::Postcard),
            unknown => Err(FormatError::UnknownFormat(unknown)),
        }
    }
}

// ─── FormatError ─────────────────────────────────────────────────────────────

/// Errors that can occur during format-aware encoding and decoding.
#[derive(Debug, Error)]
pub enum FormatError {
    #[error("Bincode encode error: {0}")]
    BincodeEncode(String),
    #[error("Bincode decode error: {0}")]
    BindecodeError(String),
    #[error("JSON encode error: {0}")]
    JsonEncode(String),
    #[error("JSON decode error: {0}")]
    JsonDecode(String),
    #[error("Postcard encode error: {0}")]
    PostcardEncode(String),
    #[error("Postcard decode error: {0}")]
    PostcardDecode(String),
    #[error("Unknown format byte: {0:#x}")]
    UnknownFormat(u8),
    #[error("Empty payload")]
    EmptyPayload,
}

// ─── SerializerStats ─────────────────────────────────────────────────────────

/// Accumulated statistics for a single `WireSerializer` instance.
#[derive(Debug, Default, Clone)]
pub struct SerializerStats {
    /// Number of successful `encode` / `encode_raw` calls.
    pub encode_calls: u64,
    /// Number of successful `decode` / `decode_raw` calls.
    pub decode_calls: u64,
    /// Total number of bytes produced by successful encode calls.
    pub total_bytes_encoded: u64,
    /// Total number of bytes consumed by successful decode calls.
    pub total_bytes_decoded: u64,
    /// Number of encode calls that returned an error.
    pub encode_errors: u64,
    /// Number of decode calls that returned an error.
    pub decode_errors: u64,
}

// ─── Low-level encoding helpers ──────────────────────────────────────────────

/// Encode `value` using Bincode (via oxicode).
fn encode_bincode<T: Serialize>(value: &T) -> Result<Vec<u8>, FormatError> {
    oxicode::encode_to_vec(&oxicode::serde::Compat(value))
        .map_err(|e| FormatError::BincodeEncode(e.to_string()))
}

/// Decode `data` using Bincode (via oxicode).
fn decode_bincode<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, FormatError> {
    let (compat, _): (oxicode::serde::Compat<T>, _) = oxicode::decode_from_slice(data)
        .map_err(|e| FormatError::BindecodeError(e.to_string()))?;
    Ok(compat.0)
}

/// Encode `value` as compact JSON bytes (no trailing newline).
fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, FormatError> {
    serde_json::to_vec(value).map_err(|e| FormatError::JsonEncode(e.to_string()))
}

/// Decode `data` from JSON bytes.
fn decode_json<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, FormatError> {
    serde_json::from_slice(data).map_err(|e| FormatError::JsonDecode(e.to_string()))
}

/// Encode `value` using Postcard (COBS framing, `alloc`).
fn encode_postcard<T: Serialize>(value: &T) -> Result<Vec<u8>, FormatError> {
    postcard::to_allocvec(value).map_err(|e| FormatError::PostcardEncode(e.to_string()))
}

/// Decode `data` using Postcard.
fn decode_postcard<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, FormatError> {
    postcard::from_bytes(data).map_err(|e| FormatError::PostcardDecode(e.to_string()))
}

/// Dispatch encoding to the selected format without a prefix byte.
fn encode_raw_with_format<T: Serialize>(
    format: WireFormat,
    value: &T,
) -> Result<Vec<u8>, FormatError> {
    match format {
        WireFormat::Bincode => encode_bincode(value),
        WireFormat::Json => encode_json(value),
        WireFormat::Postcard => encode_postcard(value),
    }
}

/// Dispatch decoding to the selected format.
fn decode_raw_with_format<T: for<'de> Deserialize<'de>>(
    format: WireFormat,
    data: &[u8],
) -> Result<T, FormatError> {
    match format {
        WireFormat::Bincode => decode_bincode(data),
        WireFormat::Json => decode_json(data),
        WireFormat::Postcard => decode_postcard(data),
    }
}

// ─── WireSerializer ──────────────────────────────────────────────────────────

/// Format-agnostic serialization interface.
///
/// Each instance is bound to a single `WireFormat` and accumulates call
/// statistics. The `encode` / `decode` pair prepend / parse a 1-byte format
/// tag, enabling self-describing payloads. Use `encode_raw` / `decode_raw`
/// when both sides share implicit knowledge of the format.
pub struct WireSerializer {
    format: WireFormat,
    stats: SerializerStats,
}

impl WireSerializer {
    /// Create a serializer for the given format.
    pub fn new(format: WireFormat) -> Self {
        Self {
            format,
            stats: SerializerStats::default(),
        }
    }

    /// Convenience constructor for the Bincode format.
    pub fn bincode() -> Self {
        Self::new(WireFormat::Bincode)
    }

    /// Convenience constructor for the JSON format.
    pub fn json() -> Self {
        Self::new(WireFormat::Json)
    }

    /// Convenience constructor for the Postcard format.
    pub fn postcard() -> Self {
        Self::new(WireFormat::Postcard)
    }

    /// Serialize `value` to bytes, prepending a 1-byte format tag.
    ///
    /// The resulting layout is: `[tag: u8] ++ [encoded payload]`.
    /// The tag lets `decode` automatically determine which codec to use.
    pub fn encode<T: Serialize>(&mut self, value: &T) -> Result<Vec<u8>, FormatError> {
        let payload = match encode_raw_with_format(self.format, value) {
            Ok(p) => p,
            Err(e) => {
                self.stats.encode_errors += 1;
                return Err(e);
            }
        };

        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(self.format.tag_byte());
        out.extend_from_slice(&payload);

        self.stats.encode_calls += 1;
        self.stats.total_bytes_encoded += out.len() as u64;
        Ok(out)
    }

    /// Deserialize from a format-prefixed byte slice.
    ///
    /// The first byte is read as the format tag; the remainder is handed to
    /// the appropriate codec. This is a static method — no per-instance state
    /// is modified, because the format is carried by the data itself.
    pub fn decode<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, FormatError> {
        if data.is_empty() {
            return Err(FormatError::EmptyPayload);
        }
        let format = WireFormat::from_tag_byte(data[0])?;
        decode_raw_with_format(format, &data[1..])
    }

    /// Serialize `value` **without** a format prefix (raw payload only).
    pub fn encode_raw<T: Serialize>(&mut self, value: &T) -> Result<Vec<u8>, FormatError> {
        let result = encode_raw_with_format(self.format, value);
        match &result {
            Ok(bytes) => {
                self.stats.encode_calls += 1;
                self.stats.total_bytes_encoded += bytes.len() as u64;
            }
            Err(_) => {
                self.stats.encode_errors += 1;
            }
        }
        result
    }

    /// Deserialize from a raw byte slice using **this** serializer's format.
    pub fn decode_raw<T: for<'de> Deserialize<'de>>(
        &mut self,
        data: &[u8],
    ) -> Result<T, FormatError> {
        let result = decode_raw_with_format(self.format, data);
        match &result {
            Ok(_) => {
                self.stats.decode_calls += 1;
                self.stats.total_bytes_decoded += data.len() as u64;
            }
            Err(_) => {
                self.stats.decode_errors += 1;
            }
        }
        result
    }

    /// Returns the format this serializer was constructed with.
    pub fn format(&self) -> WireFormat {
        self.format
    }

    /// Read-only view of accumulated statistics.
    pub fn stats(&self) -> &SerializerStats {
        &self.stats
    }

    /// Reset all statistics counters to zero without changing the format.
    pub fn reset_stats(&mut self) {
        self.stats = SerializerStats::default();
    }

    /// Encode `value` in every available format and report the resulting sizes.
    ///
    /// Useful for choosing the optimal format for a given payload shape in
    /// benchmarks or adaptive negotiation logic.
    pub fn benchmark_formats<T: Serialize>(value: &T) -> FormatBenchmark {
        let bincode_bytes = encode_bincode(value).map(|v| v.len()).unwrap_or(usize::MAX);
        let json_bytes = encode_json(value).map(|v| v.len()).unwrap_or(usize::MAX);
        let postcard_bytes = encode_postcard(value).map(|v| v.len()).unwrap_or(usize::MAX);

        // Identify the smallest and largest formats.
        // Tie-breaking rule: Postcard is preferred over Bincode when equal
        // (it has lower framing overhead for embedded/constrained targets);
        // Json is always deprioritized as it carries field-name overhead.
        // We iterate in preference order: Postcard < Bincode < Json.
        let sizes_by_preference = [
            (WireFormat::Postcard, postcard_bytes),
            (WireFormat::Bincode, bincode_bytes),
            (WireFormat::Json, json_bytes),
        ];

        let (smallest_format, _) = sizes_by_preference
            .iter()
            .min_by_key(|(_, s)| *s)
            .copied()
            .unwrap_or((WireFormat::Postcard, postcard_bytes));

        // For largest: Json is expected to win; if Json ties, Bincode before Postcard.
        let sizes_largest_preference = [
            (WireFormat::Json, json_bytes),
            (WireFormat::Bincode, bincode_bytes),
            (WireFormat::Postcard, postcard_bytes),
        ];

        let (largest_format, _) = sizes_largest_preference
            .iter()
            .max_by_key(|(_, s)| *s)
            .copied()
            .unwrap_or((WireFormat::Json, json_bytes));

        let json_to_bincode_ratio = if bincode_bytes == 0 {
            1.0_f32
        } else {
            json_bytes as f32 / bincode_bytes as f32
        };

        FormatBenchmark {
            bincode_bytes,
            json_bytes,
            postcard_bytes,
            smallest_format,
            largest_format,
            json_to_bincode_ratio,
        }
    }
}

// ─── FormatBenchmark ─────────────────────────────────────────────────────────

/// Result of encoding the same value in every supported format.
#[derive(Debug, Clone)]
pub struct FormatBenchmark {
    /// Number of bytes produced by Bincode encoding.
    pub bincode_bytes: usize,
    /// Number of bytes produced by JSON encoding.
    pub json_bytes: usize,
    /// Number of bytes produced by Postcard encoding.
    pub postcard_bytes: usize,
    /// The format that produced the fewest bytes.
    pub smallest_format: WireFormat,
    /// The format that produced the most bytes.
    pub largest_format: WireFormat,
    /// `json_bytes / bincode_bytes` ratio (higher → JSON is less efficient).
    pub json_to_bincode_ratio: f32,
}

// ─── FormatNegotiation ───────────────────────────────────────────────────────

/// Format negotiation advertisement — extend the version-negotiation concept
/// to include the set of encoding formats an endpoint supports and prefers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatNegotiation {
    /// All formats this endpoint is willing to use, in preference order.
    pub supported_formats: Vec<WireFormat>,
    /// The format this endpoint most prefers.
    pub preferred_format: WireFormat,
}

impl FormatNegotiation {
    /// Construct a negotiation from an explicit list and preference.
    pub fn new(supported: Vec<WireFormat>, preferred: WireFormat) -> Self {
        Self {
            supported_formats: supported,
            preferred_format: preferred,
        }
    }

    /// Default client advertisement: supports all formats, prefers Postcard
    /// (smallest wire size for embedded / bandwidth-constrained clients).
    pub fn default_client() -> Self {
        Self::new(
            vec![WireFormat::Postcard, WireFormat::Bincode, WireFormat::Json],
            WireFormat::Postcard,
        )
    }

    /// Default server advertisement: supports all formats, prefers Bincode
    /// (most compact for the complex, nested messages servers handle).
    pub fn default_server() -> Self {
        Self::new(
            vec![WireFormat::Bincode, WireFormat::Postcard, WireFormat::Json],
            WireFormat::Bincode,
        )
    }

    /// Find the best mutually-supported format between `self` and `remote`.
    ///
    /// Selection algorithm:
    /// 1. If both sides share the same preferred format, use it.
    /// 2. Otherwise, walk `self.supported_formats` in order and return the
    ///    first one that appears in `remote.supported_formats`.
    /// 3. Return `None` if the intersection is empty.
    pub fn negotiate(&self, remote: &FormatNegotiation) -> Option<WireFormat> {
        // Fast path: both prefer the same format and it's mutually supported.
        if self.preferred_format == remote.preferred_format
            && remote.supported_formats.contains(&self.preferred_format)
            && self.supported_formats.contains(&remote.preferred_format)
        {
            return Some(self.preferred_format);
        }

        // Walk own preference order, pick first mutually supported format.
        for fmt in &self.supported_formats {
            if remote.supported_formats.contains(fmt) {
                return Some(*fmt);
            }
        }

        None
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // A simple payload used across many tests.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Payload {
        id: u32,
        name: String,
        values: Vec<f32>,
    }

    impl Payload {
        fn sample() -> Self {
            Self {
                id: 42,
                name: "mesh-node".to_string(),
                values: vec![1.1, 2.2, 3.3],
            }
        }
    }

    // ── WireFormat property tests ────────────────────────────────────────────

    #[test]
    fn test_format_bincode_is_binary() {
        assert!(WireFormat::Bincode.is_binary());
        assert!(!WireFormat::Bincode.is_human_readable());
    }

    #[test]
    fn test_format_json_is_human_readable() {
        assert!(WireFormat::Json.is_human_readable());
        assert!(!WireFormat::Json.is_binary());
    }

    #[test]
    fn test_format_postcard_is_binary() {
        assert!(WireFormat::Postcard.is_binary());
        assert!(!WireFormat::Postcard.is_human_readable());
    }

    // ── Roundtrip tests ──────────────────────────────────────────────────────

    #[test]
    fn test_serializer_bincode_roundtrip() {
        let mut s = WireSerializer::bincode();
        let original = Payload::sample();
        let encoded = s.encode_raw(&original).expect("bincode encode failed");
        let decoded: Payload = s.decode_raw(&encoded).expect("bincode decode failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_serializer_json_roundtrip() {
        let mut s = WireSerializer::json();
        let original = Payload::sample();
        let encoded = s.encode_raw(&original).expect("json encode failed");
        let decoded: Payload = s.decode_raw(&encoded).expect("json decode failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_serializer_postcard_roundtrip() {
        let mut s = WireSerializer::postcard();
        let original = Payload::sample();
        let encoded = s.encode_raw(&original).expect("postcard encode failed");
        let decoded: Payload = s.decode_raw(&encoded).expect("postcard decode failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_serializer_prefixed_encode_decode() {
        // Each format should auto-detect itself from the prefix.
        for format in [WireFormat::Bincode, WireFormat::Json, WireFormat::Postcard] {
            let mut s = WireSerializer::new(format);
            let original = Payload::sample();
            let encoded = s.encode(&original).expect("encode failed");
            // The first byte must match the format tag.
            assert_eq!(encoded[0], format.tag_byte());
            let decoded: Payload = WireSerializer::decode(&encoded).expect("decode failed");
            assert_eq!(original, decoded, "roundtrip failed for {}", format.name());
        }
    }

    // ── Statistics tests ─────────────────────────────────────────────────────

    #[test]
    fn test_serializer_stats_increment() {
        let mut s = WireSerializer::bincode();
        let p = Payload::sample();
        for _ in 0..3 {
            s.encode(&p).unwrap();
        }
        assert_eq!(s.stats().encode_calls, 3);
    }

    #[test]
    fn test_serializer_stats_bytes() {
        let mut s = WireSerializer::json();
        let p = Payload::sample();
        s.encode(&p).unwrap();
        assert!(s.stats().total_bytes_encoded > 0);
    }

    #[test]
    fn test_serializer_reset_stats() {
        let mut s = WireSerializer::postcard();
        let p = Payload::sample();
        s.encode(&p).unwrap();
        s.reset_stats();
        let stats = s.stats();
        assert_eq!(stats.encode_calls, 0);
        assert_eq!(stats.decode_calls, 0);
        assert_eq!(stats.total_bytes_encoded, 0);
        assert_eq!(stats.total_bytes_decoded, 0);
        assert_eq!(stats.encode_errors, 0);
        assert_eq!(stats.decode_errors, 0);
    }

    // A struct with many small integer fields that exercise postcard's varint
    // compression advantage.  Bincode uses fixed-width integers; postcard uses
    // varint, so for values < 128 each u64 field is 1 byte vs 8 bytes.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct SmallInts {
        a: u64,
        b: u64,
        c: u64,
        d: u64,
        e: u64,
        f: u64,
        g: u64,
        h: u64,
    }

    impl SmallInts {
        fn sample() -> Self {
            // All values fit in 1 byte as varints (< 128).
            Self { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8 }
        }
    }

    // ── Benchmark tests ──────────────────────────────────────────────────────

    #[test]
    fn test_format_benchmark_json_biggest() {
        let p = Payload::sample();
        let bench = WireSerializer::benchmark_formats(&p);
        // JSON includes field names and is always the largest of the three.
        assert!(bench.json_bytes >= bench.bincode_bytes);
        assert_eq!(bench.largest_format, WireFormat::Json);
    }

    #[test]
    fn test_format_benchmark_postcard_smallest() {
        // Use a struct with many small u64 fields.
        // Postcard encodes u64 values < 128 as 1-byte varints.
        // oxicode (v0.1.1) uses the same varint scheme as postcard,
        // so the two formats produce identical byte counts for simple types.
        // The tie-breaking rule in benchmark_formats prefers Postcard over Bincode.
        let p = SmallInts::sample();
        let bench = WireSerializer::benchmark_formats(&p);
        // Both Postcard and Bincode are ≤ JSON for this payload.
        assert!(
            bench.postcard_bytes <= bench.json_bytes,
            "postcard ({}) should be <= json ({}) for small-int payloads",
            bench.postcard_bytes,
            bench.json_bytes
        );
        // Postcard wins the tie (tie-breaking preference: Postcard > Bincode > Json).
        assert_eq!(
            bench.smallest_format,
            WireFormat::Postcard,
            "Postcard should win tie-breaking as smallest format"
        );
    }

    // ── Negotiation tests ────────────────────────────────────────────────────

    #[test]
    fn test_negotiation_find_common_format() {
        // Both support all formats; the initiator's preferred format wins
        // unless the responder's preferred format matches.
        let client = FormatNegotiation::new(
            vec![WireFormat::Json, WireFormat::Bincode, WireFormat::Postcard],
            WireFormat::Json,
        );
        let server = FormatNegotiation::new(
            vec![WireFormat::Json, WireFormat::Bincode, WireFormat::Postcard],
            WireFormat::Json,
        );
        let result = client.negotiate(&server);
        assert_eq!(result, Some(WireFormat::Json));
    }

    #[test]
    fn test_negotiation_no_common() {
        // Non-overlapping format sets → no deal.
        let side_a = FormatNegotiation::new(vec![WireFormat::Json], WireFormat::Json);
        let side_b = FormatNegotiation::new(vec![WireFormat::Postcard], WireFormat::Postcard);
        assert_eq!(side_a.negotiate(&side_b), None);
    }

    #[test]
    fn test_negotiation_client_server_defaults() {
        let client = FormatNegotiation::default_client(); // prefers Postcard
        let server = FormatNegotiation::default_server(); // prefers Bincode

        let result = client.negotiate(&server);
        assert!(result.is_some(), "Should find a common format");
        let fmt = result.unwrap();
        // Both support Postcard and Bincode; client preference (Postcard) wins
        // because it appears first in client's list and server supports it.
        assert_eq!(fmt, WireFormat::Postcard);
    }

    #[test]
    fn test_decode_unknown_format_byte() {
        let data = vec![0xFF_u8, 0x01, 0x02];
        let result = WireSerializer::decode::<Payload>(&data);
        assert!(matches!(result, Err(FormatError::UnknownFormat(0xFF))));
    }
}
