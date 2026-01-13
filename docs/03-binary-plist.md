# Section 03: Binary Plist Codec

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)

## Overview

AirPlay 2 uses binary property lists (bplist) extensively for protocol messages. This section implements a sans-IO binary plist encoder and decoder optimized for AirPlay protocol requirements.

While the `plist` crate exists, we implement a focused subset because:
1. AirPlay uses specific plist patterns we can optimize for
2. We need fine-grained control over encoding for protocol compatibility
3. Sans-IO design requires no I/O dependencies in the codec

## Objectives

- Implement binary plist decoding (bplist00 format)
- Implement binary plist encoding
- Support all types used in AirPlay protocol
- Provide convenient conversion to/from Rust types
- Zero-copy parsing where possible

---

## Tasks

### 3.1 Plist Value Types

- [ ] **3.1.1** Define `PlistValue` enum

**File:** `src/protocol/plist/mod.rs`

```rust
//! Binary plist codec for AirPlay protocol messages

mod decode;
mod encode;

pub use decode::{decode, PlistDecodeError};
pub use encode::{encode, PlistEncodeError};

use std::collections::HashMap;

/// A property list value
#[derive(Debug, Clone, PartialEq)]
pub enum PlistValue {
    /// Boolean value
    Boolean(bool),

    /// Unsigned integer (up to 64 bits)
    Integer(i64),

    /// Unsigned integer for large values
    UnsignedInteger(u64),

    /// Floating point number
    Real(f64),

    /// UTF-8 string
    String(String),

    /// Binary data
    Data(Vec<u8>),

    /// Date as seconds since 2001-01-01 00:00:00 UTC
    Date(f64),

    /// Array of values
    Array(Vec<PlistValue>),

    /// Dictionary (key-value pairs)
    Dictionary(HashMap<String, PlistValue>),

    /// UID reference (used internally)
    Uid(u64),
}

impl PlistValue {
    /// Try to get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PlistValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as i64
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PlistValue::Integer(i) => Some(*i),
            PlistValue::UnsignedInteger(u) => (*u).try_into().ok(),
            _ => None,
        }
    }

    /// Try to get as u64
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            PlistValue::Integer(i) => (*i).try_into().ok(),
            PlistValue::UnsignedInteger(u) => Some(*u),
            _ => None,
        }
    }

    /// Try to get as f64
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PlistValue::Real(f) => Some(*f),
            PlistValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get as string reference
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PlistValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as byte slice
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            PlistValue::Data(d) => Some(d),
            _ => None,
        }
    }

    /// Try to get as array reference
    pub fn as_array(&self) -> Option<&[PlistValue]> {
        match self {
            PlistValue::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to get as dictionary reference
    pub fn as_dict(&self) -> Option<&HashMap<String, PlistValue>> {
        match self {
            PlistValue::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    /// Check if value is null/empty
    pub fn is_null(&self) -> bool {
        matches!(self, PlistValue::Data(d) if d.is_empty())
    }
}
```

- [ ] **3.1.2** Implement `From` conversions for common types

**File:** `src/protocol/plist/mod.rs`

```rust
impl From<bool> for PlistValue {
    fn from(v: bool) -> Self {
        PlistValue::Boolean(v)
    }
}

impl From<i32> for PlistValue {
    fn from(v: i32) -> Self {
        PlistValue::Integer(v as i64)
    }
}

impl From<i64> for PlistValue {
    fn from(v: i64) -> Self {
        PlistValue::Integer(v)
    }
}

impl From<u64> for PlistValue {
    fn from(v: u64) -> Self {
        PlistValue::UnsignedInteger(v)
    }
}

impl From<f64> for PlistValue {
    fn from(v: f64) -> Self {
        PlistValue::Real(v)
    }
}

impl From<String> for PlistValue {
    fn from(v: String) -> Self {
        PlistValue::String(v)
    }
}

impl From<&str> for PlistValue {
    fn from(v: &str) -> Self {
        PlistValue::String(v.to_string())
    }
}

impl From<Vec<u8>> for PlistValue {
    fn from(v: Vec<u8>) -> Self {
        PlistValue::Data(v)
    }
}

impl<T: Into<PlistValue>> From<Vec<T>> for PlistValue {
    fn from(v: Vec<T>) -> Self {
        PlistValue::Array(v.into_iter().map(Into::into).collect())
    }
}

impl<K: Into<String>, V: Into<PlistValue>> FromIterator<(K, V)> for PlistValue {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        PlistValue::Dictionary(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect()
        )
    }
}
```

- [ ] **3.1.3** Implement dictionary builder for convenient construction

**File:** `src/protocol/plist/mod.rs`

```rust
/// Builder for creating plist dictionaries
#[derive(Debug, Default)]
pub struct DictBuilder {
    map: HashMap<String, PlistValue>,
}

impl DictBuilder {
    /// Create a new dictionary builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a key-value pair
    pub fn insert(mut self, key: impl Into<String>, value: impl Into<PlistValue>) -> Self {
        self.map.insert(key.into(), value.into());
        self
    }

    /// Insert if value is Some
    pub fn insert_opt<V: Into<PlistValue>>(
        mut self,
        key: impl Into<String>,
        value: Option<V>,
    ) -> Self {
        if let Some(v) = value {
            self.map.insert(key.into(), v.into());
        }
        self
    }

    /// Build the dictionary
    pub fn build(self) -> PlistValue {
        PlistValue::Dictionary(self.map)
    }
}

/// Convenience macro for creating plist dictionaries
#[macro_export]
macro_rules! plist_dict {
    ($($key:expr => $value:expr),* $(,)?) => {
        $crate::protocol::plist::DictBuilder::new()
            $(.insert($key, $value))*
            .build()
    };
}
```

---

### 3.2 Binary Plist Decoder

- [ ] **3.2.1** Implement decoder error types

**File:** `src/protocol/plist/decode.rs`

```rust
use thiserror::Error;

/// Errors that can occur during plist decoding
#[derive(Debug, Error)]
pub enum PlistDecodeError {
    #[error("invalid magic: expected 'bplist00', got {0:?}")]
    InvalidMagic([u8; 8]),

    #[error("buffer too small: need {needed} bytes, have {have}")]
    BufferTooSmall { needed: usize, have: usize },

    #[error("invalid trailer")]
    InvalidTrailer,

    #[error("invalid object type marker: 0x{0:02x}")]
    InvalidObjectMarker(u8),

    #[error("invalid offset: {0}")]
    InvalidOffset(u64),

    #[error("string is not valid UTF-8")]
    InvalidUtf8,

    #[error("unsupported object type: {0}")]
    UnsupportedType(String),

    #[error("circular reference detected")]
    CircularReference,

    #[error("integer overflow")]
    IntegerOverflow,
}
```

- [ ] **3.2.2** Implement trailer parsing

**File:** `src/protocol/plist/decode.rs`

```rust
/// Binary plist trailer (last 32 bytes)
#[derive(Debug)]
struct Trailer {
    /// Unused padding bytes
    _unused: [u8; 5],
    /// Sort version (unused)
    _sort_version: u8,
    /// Size of offset table entries (1, 2, 4, or 8)
    offset_size: u8,
    /// Size of object reference entries (1, 2, 4, or 8)
    object_ref_size: u8,
    /// Number of objects in file
    num_objects: u64,
    /// Index of root object
    root_object_index: u64,
    /// Offset of offset table
    offset_table_offset: u64,
}

impl Trailer {
    fn parse(data: &[u8]) -> Result<Self, PlistDecodeError> {
        if data.len() < 32 {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: 32,
                have: data.len(),
            });
        }

        let trailer = &data[data.len() - 32..];

        Ok(Self {
            _unused: [0; 5], // bytes 0-4
            _sort_version: trailer[5],
            offset_size: trailer[6],
            object_ref_size: trailer[7],
            num_objects: u64::from_be_bytes(trailer[8..16].try_into().unwrap()),
            root_object_index: u64::from_be_bytes(trailer[16..24].try_into().unwrap()),
            offset_table_offset: u64::from_be_bytes(trailer[24..32].try_into().unwrap()),
        })
    }
}
```

- [ ] **3.2.3** Implement main decoder

**File:** `src/protocol/plist/decode.rs`

```rust
use super::PlistValue;
use std::collections::{HashMap, HashSet};

/// Decode binary plist data into a PlistValue
pub fn decode(data: &[u8]) -> Result<PlistValue, PlistDecodeError> {
    // Check magic header
    if data.len() < 8 {
        return Err(PlistDecodeError::BufferTooSmall {
            needed: 8,
            have: data.len(),
        });
    }

    let magic = &data[0..8];
    if magic != b"bplist00" {
        let mut arr = [0u8; 8];
        arr.copy_from_slice(magic);
        return Err(PlistDecodeError::InvalidMagic(arr));
    }

    let trailer = Trailer::parse(data)?;
    let mut decoder = Decoder::new(data, &trailer)?;

    decoder.decode_object(trailer.root_object_index, &mut HashSet::new())
}

struct Decoder<'a> {
    data: &'a [u8],
    offset_table: Vec<u64>,
    object_ref_size: usize,
}

impl<'a> Decoder<'a> {
    fn new(data: &'a [u8], trailer: &Trailer) -> Result<Self, PlistDecodeError> {
        let offset_table = Self::parse_offset_table(data, trailer)?;

        Ok(Self {
            data,
            offset_table,
            object_ref_size: trailer.object_ref_size as usize,
        })
    }

    fn parse_offset_table(data: &[u8], trailer: &Trailer) -> Result<Vec<u64>, PlistDecodeError> {
        // Parse offset table implementation
        let start = trailer.offset_table_offset as usize;
        let entry_size = trailer.offset_size as usize;
        let count = trailer.num_objects as usize;

        let mut offsets = Vec::with_capacity(count);

        for i in 0..count {
            let offset_start = start + i * entry_size;
            let offset = Self::read_sized_int(
                &data[offset_start..offset_start + entry_size],
                entry_size,
            )?;
            offsets.push(offset);
        }

        Ok(offsets)
    }

    fn read_sized_int(data: &[u8], size: usize) -> Result<u64, PlistDecodeError> {
        match size {
            1 => Ok(data[0] as u64),
            2 => Ok(u16::from_be_bytes(data[..2].try_into().unwrap()) as u64),
            4 => Ok(u32::from_be_bytes(data[..4].try_into().unwrap()) as u64),
            8 => Ok(u64::from_be_bytes(data[..8].try_into().unwrap())),
            _ => Err(PlistDecodeError::InvalidTrailer),
        }
    }

    fn decode_object(
        &self,
        index: u64,
        seen: &mut HashSet<u64>,
    ) -> Result<PlistValue, PlistDecodeError> {
        // Circular reference detection
        if !seen.insert(index) {
            return Err(PlistDecodeError::CircularReference);
        }

        let offset = self.offset_table.get(index as usize)
            .ok_or(PlistDecodeError::InvalidOffset(index))?;

        let pos = *offset as usize;
        let marker = self.data[pos];

        let value = self.decode_value(marker, pos)?;

        seen.remove(&index);
        Ok(value)
    }

    fn decode_value(&self, marker: u8, pos: usize) -> Result<PlistValue, PlistDecodeError> {
        let high_nibble = marker >> 4;
        let low_nibble = marker & 0x0F;

        match high_nibble {
            0x0 => self.decode_singleton(low_nibble),
            0x1 => self.decode_integer(pos, low_nibble),
            0x2 => self.decode_real(pos, low_nibble),
            0x3 => self.decode_date(pos),
            0x4 => self.decode_data(pos, low_nibble),
            0x5 => self.decode_ascii_string(pos, low_nibble),
            0x6 => self.decode_utf16_string(pos, low_nibble),
            0x8 => self.decode_uid(pos, low_nibble),
            0xA => self.decode_array(pos, low_nibble),
            0xD => self.decode_dictionary(pos, low_nibble),
            _ => Err(PlistDecodeError::InvalidObjectMarker(marker)),
        }
    }

    // Individual decode methods...
    fn decode_singleton(&self, nibble: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_integer(&self, pos: usize, size_exp: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_real(&self, pos: usize, size_exp: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_date(&self, pos: usize) -> Result<PlistValue, PlistDecodeError>;
    fn decode_data(&self, pos: usize, length_nibble: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_ascii_string(&self, pos: usize, len: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_utf16_string(&self, pos: usize, len: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_uid(&self, pos: usize, len: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_array(&self, pos: usize, count_nibble: u8) -> Result<PlistValue, PlistDecodeError>;
    fn decode_dictionary(&self, pos: usize, count_nibble: u8) -> Result<PlistValue, PlistDecodeError>;
}
```

- [ ] **3.2.4** Implement each decode method (singleton, integer, real, etc.)

---

### 3.3 Binary Plist Encoder

- [ ] **3.3.1** Implement encoder error types

**File:** `src/protocol/plist/encode.rs`

```rust
use thiserror::Error;

/// Errors that can occur during plist encoding
#[derive(Debug, Error)]
pub enum PlistEncodeError {
    #[error("value too large to encode")]
    ValueTooLarge,

    #[error("too many objects: {0}")]
    TooManyObjects(usize),

    #[error("string encoding error")]
    StringEncodingError,
}
```

- [ ] **3.3.2** Implement encoder structure

**File:** `src/protocol/plist/encode.rs`

```rust
use super::PlistValue;
use std::collections::HashMap;

/// Encode a PlistValue to binary plist format
pub fn encode(value: &PlistValue) -> Result<Vec<u8>, PlistEncodeError> {
    let mut encoder = Encoder::new();
    encoder.encode(value)
}

struct Encoder {
    /// Object data bytes
    objects: Vec<u8>,
    /// Offset of each object in the objects buffer
    offsets: Vec<u64>,
    /// Map of already-encoded objects to their index (for deduplication)
    object_cache: HashMap<ObjectKey, usize>,
}

/// Key for object caching/deduplication
#[derive(Hash, Eq, PartialEq)]
enum ObjectKey {
    String(String),
    Data(Vec<u8>),
    Integer(i64),
    // Primitives that can be deduplicated
}

impl Encoder {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
            offsets: Vec::new(),
            object_cache: HashMap::new(),
        }
    }

    fn encode(&mut self, value: &PlistValue) -> Result<Vec<u8>, PlistEncodeError> {
        // Write magic header
        let mut output = Vec::new();
        output.extend_from_slice(b"bplist00");

        // Encode all objects, starting from root
        let root_index = self.encode_value(value)?;

        // Copy object data
        let objects_start = output.len();
        output.extend_from_slice(&self.objects);

        // Write offset table
        let offset_table_offset = output.len();
        let offset_size = self.calculate_offset_size(offset_table_offset);

        for &offset in &self.offsets {
            let adjusted = objects_start as u64 + offset;
            self.write_sized_int(&mut output, adjusted, offset_size);
        }

        // Write trailer
        self.write_trailer(
            &mut output,
            offset_size,
            self.offsets.len(),
            root_index,
            offset_table_offset,
        );

        Ok(output)
    }

    fn encode_value(&mut self, value: &PlistValue) -> Result<usize, PlistEncodeError> {
        let index = self.offsets.len();
        let offset = self.objects.len() as u64;
        self.offsets.push(offset);

        match value {
            PlistValue::Boolean(b) => self.encode_boolean(*b),
            PlistValue::Integer(i) => self.encode_integer(*i),
            PlistValue::UnsignedInteger(u) => self.encode_unsigned(*u),
            PlistValue::Real(f) => self.encode_real(*f),
            PlistValue::String(s) => self.encode_string(s),
            PlistValue::Data(d) => self.encode_data(d),
            PlistValue::Date(d) => self.encode_date(*d),
            PlistValue::Array(a) => self.encode_array(a)?,
            PlistValue::Dictionary(d) => self.encode_dictionary(d)?,
            PlistValue::Uid(u) => self.encode_uid(*u),
        }

        Ok(index)
    }

    // Encoding methods for each type
    fn encode_boolean(&mut self, value: bool);
    fn encode_integer(&mut self, value: i64);
    fn encode_unsigned(&mut self, value: u64);
    fn encode_real(&mut self, value: f64);
    fn encode_string(&mut self, value: &str);
    fn encode_data(&mut self, value: &[u8]);
    fn encode_date(&mut self, value: f64);
    fn encode_array(&mut self, value: &[PlistValue]) -> Result<(), PlistEncodeError>;
    fn encode_dictionary(&mut self, value: &HashMap<String, PlistValue>) -> Result<(), PlistEncodeError>;
    fn encode_uid(&mut self, value: u64);

    fn write_sized_int(&self, output: &mut Vec<u8>, value: u64, size: u8);
    fn calculate_offset_size(&self, max_offset: usize) -> u8;
    fn write_trailer(&self, output: &mut Vec<u8>, offset_size: u8, num_objects: usize, root: usize, offset_table_offset: usize);
}
```

- [ ] **3.3.3** Implement each encode method

---

### 3.4 AirPlay-Specific Helpers

- [ ] **3.4.1** Implement track info serialization

**File:** `src/protocol/plist/airplay.rs`

```rust
use super::{PlistValue, DictBuilder};
use crate::types::TrackInfo;

/// Convert TrackInfo to plist dictionary for AirPlay protocol
pub fn track_info_to_plist(track: &TrackInfo) -> PlistValue {
    DictBuilder::new()
        .insert("Content-Location", &track.url)
        .insert("title", &track.title)
        .insert("artist", &track.artist)
        .insert_opt("album", track.album.as_deref())
        .insert_opt("artworkURL", track.artwork_url.as_deref())
        .insert_opt("duration", track.duration_secs)
        .insert_opt("trackNumber", track.track_number.map(|n| n as i64))
        .insert_opt("discNumber", track.disc_number.map(|n| n as i64))
        .build()
}

/// Parse playback state from device response plist
pub fn parse_playback_info(plist: &PlistValue) -> Option<PlaybackInfo> {
    let dict = plist.as_dict()?;

    // Parse position, rate, duration, etc.
    // Implementation details based on protocol analysis
    todo!()
}
```

---

## Unit Tests

### Test File: `src/protocol/plist/decode.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Sample binary plists for testing (captured from real AirPlay traffic)

    const SIMPLE_DICT: &[u8] = include_bytes!("../../../tests/fixtures/simple_dict.bplist");
    const NESTED_DICT: &[u8] = include_bytes!("../../../tests/fixtures/nested_dict.bplist");
    const WITH_DATA: &[u8] = include_bytes!("../../../tests/fixtures/with_data.bplist");

    #[test]
    fn test_decode_empty_dict() {
        let data = create_bplist_empty_dict();
        let result = decode(&data).unwrap();

        assert!(matches!(result, PlistValue::Dictionary(d) if d.is_empty()));
    }

    #[test]
    fn test_decode_boolean_true() {
        let data = create_bplist_bool(true);
        let result = decode(&data).unwrap();

        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn test_decode_boolean_false() {
        let data = create_bplist_bool(false);
        let result = decode(&data).unwrap();

        assert_eq!(result.as_bool(), Some(false));
    }

    #[test]
    fn test_decode_integers() {
        for value in [0i64, 1, 127, 128, 255, 256, 65535, 65536, i64::MAX] {
            let data = create_bplist_int(value);
            let result = decode(&data).unwrap();

            assert_eq!(result.as_i64(), Some(value));
        }
    }

    #[test]
    fn test_decode_string_ascii() {
        let data = create_bplist_string("hello");
        let result = decode(&data).unwrap();

        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn test_decode_string_unicode() {
        let data = create_bplist_string("hello 🎵 world");
        let result = decode(&data).unwrap();

        assert_eq!(result.as_str(), Some("hello 🎵 world"));
    }

    #[test]
    fn test_decode_array() {
        let data = create_bplist_array(vec![1, 2, 3]);
        let result = decode(&data).unwrap();

        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_i64(), Some(1));
    }

    #[test]
    fn test_decode_nested_dict() {
        let data = NESTED_DICT;
        let result = decode(data).unwrap();

        let dict = result.as_dict().unwrap();
        let inner = dict.get("inner").and_then(|v| v.as_dict());
        assert!(inner.is_some());
    }

    #[test]
    fn test_decode_invalid_magic() {
        let data = b"notplist";
        let result = decode(data);

        assert!(matches!(result, Err(PlistDecodeError::InvalidMagic(_))));
    }

    #[test]
    fn test_decode_too_small() {
        let data = b"short";
        let result = decode(data);

        assert!(matches!(result, Err(PlistDecodeError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_decode_circular_reference() {
        // Create a plist with circular reference
        let data = create_circular_plist();
        let result = decode(&data);

        assert!(matches!(result, Err(PlistDecodeError::CircularReference)));
    }

    // Helper functions to create test plists
    fn create_bplist_empty_dict() -> Vec<u8> { todo!() }
    fn create_bplist_bool(v: bool) -> Vec<u8> { todo!() }
    fn create_bplist_int(v: i64) -> Vec<u8> { todo!() }
    fn create_bplist_string(s: &str) -> Vec<u8> { todo!() }
    fn create_bplist_array(v: Vec<i32>) -> Vec<u8> { todo!() }
    fn create_circular_plist() -> Vec<u8> { todo!() }
}
```

### Test File: `src/protocol/plist/encode.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_boolean() {
        let value = PlistValue::Boolean(true);
        let encoded = encode(&value).unwrap();

        // Verify magic header
        assert_eq!(&encoded[0..8], b"bplist00");

        // Round-trip test
        let decoded = super::super::decode::decode(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn test_encode_integers() {
        for value in [0i64, 1, 127, 128, 255, 256, 65535, -1, -128, i64::MAX, i64::MIN] {
            let plist = PlistValue::Integer(value);
            let encoded = encode(&plist).unwrap();
            let decoded = super::super::decode::decode(&encoded).unwrap();

            assert_eq!(decoded.as_i64(), Some(value), "Failed for value: {}", value);
        }
    }

    #[test]
    fn test_encode_string() {
        let value = PlistValue::String("hello world".to_string());
        let encoded = encode(&value).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        assert_eq!(decoded.as_str(), Some("hello world"));
    }

    #[test]
    fn test_encode_string_unicode() {
        let value = PlistValue::String("日本語 🎵".to_string());
        let encoded = encode(&value).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        assert_eq!(decoded.as_str(), Some("日本語 🎵"));
    }

    #[test]
    fn test_encode_data() {
        let data = vec![0x00, 0x01, 0x02, 0xFF];
        let value = PlistValue::Data(data.clone());
        let encoded = encode(&value).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        assert_eq!(decoded.as_bytes(), Some(data.as_slice()));
    }

    #[test]
    fn test_encode_array() {
        let value = PlistValue::Array(vec![
            PlistValue::Integer(1),
            PlistValue::Integer(2),
            PlistValue::String("three".to_string()),
        ]);

        let encoded = encode(&value).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        let arr = decoded.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_i64(), Some(1));
        assert_eq!(arr[2].as_str(), Some("three"));
    }

    #[test]
    fn test_encode_dictionary() {
        let mut dict = HashMap::new();
        dict.insert("key1".to_string(), PlistValue::Integer(42));
        dict.insert("key2".to_string(), PlistValue::String("value".to_string()));

        let value = PlistValue::Dictionary(dict);
        let encoded = encode(&value).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        let d = decoded.as_dict().unwrap();
        assert_eq!(d.get("key1").and_then(|v| v.as_i64()), Some(42));
        assert_eq!(d.get("key2").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_encode_nested_structures() {
        let inner = plist_dict! {
            "nested_key" => "nested_value"
        };

        let outer = plist_dict! {
            "outer_key" => inner,
            "number" => 123i64
        };

        let encoded = encode(&outer).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        let d = decoded.as_dict().unwrap();
        let inner_dict = d.get("outer_key").and_then(|v| v.as_dict()).unwrap();
        assert_eq!(inner_dict.get("nested_key").and_then(|v| v.as_str()), Some("nested_value"));
    }

    #[test]
    fn test_roundtrip_complex() {
        // Create a complex structure similar to AirPlay messages
        let value = plist_dict! {
            "Content-Location" => "http://example.com/audio.mp3",
            "Start-Position" => 0.0f64,
            "trackInfo" => plist_dict! {
                "title" => "Test Track",
                "artist" => "Test Artist",
                "album" => "Test Album",
                "duration" => 180.5f64
            }
        };

        let encoded = encode(&value).unwrap();
        let decoded = super::super::decode::decode(&encoded).unwrap();

        assert_eq!(value, decoded);
    }
}
```

### Test File: `src/protocol/plist/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plist_value_accessors() {
        let value = PlistValue::Integer(42);
        assert_eq!(value.as_i64(), Some(42));
        assert_eq!(value.as_str(), None);
        assert_eq!(value.as_bool(), None);
    }

    #[test]
    fn test_plist_value_from_conversions() {
        assert!(matches!(PlistValue::from(true), PlistValue::Boolean(true)));
        assert!(matches!(PlistValue::from(42i64), PlistValue::Integer(42)));
        assert!(matches!(PlistValue::from(3.14f64), PlistValue::Real(f) if (f - 3.14).abs() < f64::EPSILON));
        assert!(matches!(PlistValue::from("hello"), PlistValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_dict_builder() {
        let dict = DictBuilder::new()
            .insert("key1", "value1")
            .insert("key2", 42i64)
            .insert_opt("key3", Some("present"))
            .insert_opt::<String>("key4", None)
            .build();

        let d = dict.as_dict().unwrap();
        assert_eq!(d.len(), 3);
        assert!(d.contains_key("key1"));
        assert!(d.contains_key("key2"));
        assert!(d.contains_key("key3"));
        assert!(!d.contains_key("key4"));
    }

    #[test]
    fn test_plist_dict_macro() {
        let dict = plist_dict! {
            "name" => "test",
            "count" => 5i64,
        };

        let d = dict.as_dict().unwrap();
        assert_eq!(d.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(d.get("count").and_then(|v| v.as_i64()), Some(5));
    }
}
```

---

## Integration Tests

### Test: Decode real AirPlay protocol messages

```rust
// tests/protocol/plist_integration.rs

#[test]
fn test_decode_airplay_play_request() {
    // Binary plist captured from real AirPlay PLAY request
    let data = include_bytes!("fixtures/airplay_play_request.bplist");
    let result = decode(data).unwrap();

    let dict = result.as_dict().unwrap();
    assert!(dict.contains_key("Content-Location"));
    assert!(dict.contains_key("Start-Position"));
}

#[test]
fn test_decode_airplay_status_response() {
    // Binary plist from /playback-info response
    let data = include_bytes!("fixtures/airplay_status.bplist");
    let result = decode(data).unwrap();

    let dict = result.as_dict().unwrap();
    // Verify expected fields
}
```

---

## Acceptance Criteria

- [ ] Decoder handles all plist types used by AirPlay
- [ ] Encoder produces valid binary plist output
- [ ] Round-trip encode/decode preserves all data
- [ ] Decoder rejects malformed input with clear errors
- [ ] Circular reference detection works
- [ ] Unicode strings handled correctly (UTF-8 and UTF-16)
- [ ] Large integers (64-bit) work correctly
- [ ] Performance: Decode 10KB plist in < 1ms
- [ ] All unit tests pass
- [ ] Integration tests with captured AirPlay data pass

---

## Notes

- Consider fuzzing the decoder with cargo-fuzz
- May want to add streaming decode for very large plists (unlikely in AirPlay)
- String deduplication in encoder can reduce output size
- The `plist` crate could be used as reference for edge cases
