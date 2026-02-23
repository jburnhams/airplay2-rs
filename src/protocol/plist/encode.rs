use std::collections::HashMap;

use thiserror::Error;

use super::PlistValue;

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

/// Encode a `PlistValue` to binary plist format
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
    /// Size of object references in bytes. Fixed to 2 for now (up to 65535 objects).
    ref_size: u8,
}

/// Key for object caching/deduplication
#[derive(Hash, Eq, PartialEq, Clone)]
enum ObjectKey {
    String(String),
    Data(Vec<u8>),
    Integer(i64),
    Real(u64), // float bits
    Uid(u64),
    Date(u64), // float bits
}

impl Encoder {
    fn new() -> Self {
        Self {
            objects: Vec::new(),
            offsets: Vec::new(),
            object_cache: HashMap::new(),
            ref_size: 2,
        }
    }

    fn encode(&mut self, value: &PlistValue) -> Result<Vec<u8>, PlistEncodeError> {
        // Write magic header
        let mut output = Vec::new();
        output.extend_from_slice(b"bplist00");

        // Encode all objects, starting from root.
        let root_index = self.encode_value(value)?;

        // Check if we exceeded object limit for our ref_size
        if self.offsets.len() > 65535 {
            return Err(PlistEncodeError::TooManyObjects(self.offsets.len()));
        }

        // Copy object data
        let objects_start = output.len();
        output.extend_from_slice(&self.objects);

        // Write offset table
        let offset_table_offset = output.len();
        // Determine size needed for offsets
        // Max offset is objects_start + objects.len()
        let max_offset = self.objects.len();
        let max_absolute_offset = objects_start + max_offset;
        let offset_size = Self::calculate_offset_size(max_absolute_offset);

        for &offset in &self.offsets {
            let adjusted = objects_start as u64 + offset;
            Self::write_sized_int(&mut output, adjusted, offset_size);
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
        // Check cache for primitives
        if let Some(key) = Self::get_object_key(value) {
            if let Some(&index) = self.object_cache.get(&key) {
                return Ok(index);
            }
        }

        // For containers, we must encode children first to get their indices
        let body = match value {
            PlistValue::Array(arr) => {
                let mut refs = Vec::with_capacity(arr.len());
                for item in arr {
                    refs.push(self.encode_value(item)?);
                }
                Some(self.create_array_body(&refs)?)
            }
            PlistValue::Dictionary(dict) => {
                // Keys must be strings. And we should sort them.
                // We need to encode keys and values.
                // Sorted by key string.
                let mut sorted_keys: Vec<&String> = dict.keys().collect();
                sorted_keys.sort();

                let mut key_refs = Vec::with_capacity(dict.len());
                let mut val_refs = Vec::with_capacity(dict.len());

                for k in sorted_keys {
                    // Encode key (String)
                    key_refs.push(self.encode_value(&PlistValue::String(k.clone()))?);
                    // Encode value
                    val_refs.push(self.encode_value(&dict[k])?);
                }

                Some(self.create_dict_body(&key_refs, &val_refs)?)
            }
            _ => None,
        };

        // If it was a primitive, we handle it now. If it was container, we have the body.

        let offset = self.objects.len() as u64;
        self.offsets.push(offset);
        let index = self.offsets.len() - 1;

        if let Some(b) = body {
            self.objects.extend_from_slice(&b);
        } else {
            // Encode primitive
            match value {
                PlistValue::Boolean(b) => self.encode_boolean(*b),
                PlistValue::Integer(i) => self.encode_integer(*i),
                PlistValue::UnsignedInteger(u) => self.encode_unsigned(*u),
                PlistValue::Real(f) => self.encode_real(*f),
                PlistValue::String(s) => self.encode_string(s),
                PlistValue::Data(d) => self.encode_data(d),
                PlistValue::Date(d) => self.encode_date(*d),
                PlistValue::Uid(u) => self.encode_uid(*u),
                PlistValue::Array(_) | PlistValue::Dictionary(_) => unreachable!(),
            }
        }

        // Add to cache if primitive
        if let Some(key) = Self::get_object_key(value) {
            self.object_cache.insert(key, index);
        }

        Ok(index)
    }

    fn get_object_key(value: &PlistValue) -> Option<ObjectKey> {
        match value {
            PlistValue::String(s) => Some(ObjectKey::String(s.clone())),
            PlistValue::Data(d) => Some(ObjectKey::Data(d.clone())),
            PlistValue::Integer(i) => Some(ObjectKey::Integer(*i)),
            PlistValue::Real(f) => Some(ObjectKey::Real(f.to_bits())),
            PlistValue::Date(d) => Some(ObjectKey::Date(d.to_bits())),
            PlistValue::Uid(u) => Some(ObjectKey::Uid(*u)),
            _ => None, // Don't cache containers or others
        }
    }

    // Encoding methods

    fn encode_boolean(&mut self, value: bool) {
        if value {
            self.objects.push(0x09);
        } else {
            self.objects.push(0x08);
        }
    }

    fn encode_integer(&mut self, value: i64) {
        // Determine size needed
        if value >= 0 {
            if value <= 127 {
                self.objects.push(0x10);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "Value checked to be within u8 range"
                )]
                self.objects.push(value as u8);
            } else if value <= 32767 {
                self.objects.push(0x11);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "Value checked to be within u16 range"
                )]
                self.objects
                    .extend_from_slice(&(value as u16).to_be_bytes());
            } else if value <= 2_147_483_647 {
                self.objects.push(0x12);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "Value checked to be within u32 range"
                )]
                self.objects
                    .extend_from_slice(&(value as u32).to_be_bytes());
            } else {
                self.objects.push(0x13);
                self.objects.extend_from_slice(&value.to_be_bytes());
            }
        } else {
            // Negative integers are always 8 bytes in bplist
            self.objects.push(0x13);
            self.objects.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn encode_unsigned(&mut self, value: u64) {
        // If it fits in i64, encode as standard integer
        #[allow(clippy::cast_possible_wrap, reason = "Value checked to fit in i64")]
        if value <= i64::MAX as u64 {
            self.encode_integer(value as i64);
        } else {
            // Use 16 bytes to represent large unsigned as positive integer
            // This ensures it decodes as UnsignedInteger
            self.objects.push(0x14); // 2^4 = 16 bytes
            self.objects.extend_from_slice(&[0u8; 8]); // Upper 64 bits zero
            self.objects.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn encode_real(&mut self, value: f64) {
        // Always use 8 bytes (double)
        self.objects.push(0x23);
        self.objects.extend_from_slice(&value.to_be_bytes());
    }

    fn encode_date(&mut self, value: f64) {
        self.objects.push(0x33);
        self.objects.extend_from_slice(&value.to_be_bytes());
    }

    fn encode_string(&mut self, value: &str) {
        if value.is_ascii() {
            Self::write_header_to(&mut self.objects, 0x5, value.len());
            self.objects.extend_from_slice(value.as_bytes());
        } else {
            // UTF-16 BE
            let u16s: Vec<u16> = value.encode_utf16().collect();
            Self::write_header_to(&mut self.objects, 0x6, u16s.len());
            for c in u16s {
                self.objects.extend_from_slice(&c.to_be_bytes());
            }
        }
    }

    fn encode_data(&mut self, value: &[u8]) {
        Self::write_header_to(&mut self.objects, 0x4, value.len());
        self.objects.extend_from_slice(value);
    }

    fn encode_uid(&mut self, value: u64) {
        // Compact encoding for UID
        let bytes = if value <= 0xFF {
            1
        } else if value <= 0xFFFF {
            2
        } else if value <= 0xFFFF_FFFF {
            4
        } else {
            8
        };

        // We know bytes is small, safe cast
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bytes is 1, 2, 4, or 8, so (bytes-1) fits in u8"
        )]
        let marker = 0x80 | (bytes - 1) as u8;
        self.objects.push(marker);
        match bytes {
            // Using try_from here would be safer
            1 => self.objects.push(u8::try_from(value).unwrap()),
            2 => self
                .objects
                .extend_from_slice(&u16::try_from(value).unwrap().to_be_bytes()),
            4 => self
                .objects
                .extend_from_slice(&u32::try_from(value).unwrap().to_be_bytes()),
            8 => self.objects.extend_from_slice(&value.to_be_bytes()),
            _ => unreachable!(),
        }
    }

    fn create_array_body(&self, refs: &[usize]) -> Result<Vec<u8>, PlistEncodeError> {
        let mut body = Vec::new();
        Self::write_header_to(&mut body, 0xA, refs.len());

        for &r in refs {
            self.write_ref(&mut body, r)?;
        }
        Ok(body)
    }

    fn create_dict_body(
        &self,
        key_refs: &[usize],
        val_refs: &[usize],
    ) -> Result<Vec<u8>, PlistEncodeError> {
        let mut body = Vec::new();
        Self::write_header_to(&mut body, 0xD, key_refs.len());

        for &r in key_refs {
            self.write_ref(&mut body, r)?;
        }
        for &r in val_refs {
            self.write_ref(&mut body, r)?;
        }
        Ok(body)
    }

    // Helpers

    fn write_header_to(output: &mut Vec<u8>, kind: u8, len: usize) {
        if len < 15 {
            // len < 15 fits in u8
            #[allow(clippy::cast_possible_truncation, reason = "len checked to be < 15")]
            output.push((kind << 4) | len as u8);
        } else {
            output.push((kind << 4) | 0xF);
            // Write length as integer
            Self::write_int_to(output, len as u64);
        }
    }

    fn write_int_to(output: &mut Vec<u8>, value: u64) {
        // This is for the count following 0xF. It looks like an Integer object (0x1n...)
        if value <= 0xFF {
            output.push(0x10);
            // value <= 0xFF fits in u8
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Value checked to be within u8 range"
            )]
            output.push(value as u8);
        } else if value <= 0xFFFF {
            output.push(0x11);
            // value <= 0xFFFF fits in u16
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Value checked to be within u16 range"
            )]
            output.extend_from_slice(&(value as u16).to_be_bytes());
        } else if value <= 0xFFFF_FFFF {
            output.push(0x12);
            // value <= 0xFFFFFFFF fits in u32
            #[allow(
                clippy::cast_possible_truncation,
                reason = "Value checked to be within u32 range"
            )]
            output.extend_from_slice(&(value as u32).to_be_bytes());
        } else {
            output.push(0x13);
            output.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn write_ref(&self, output: &mut Vec<u8>, index: usize) -> Result<(), PlistEncodeError> {
        // Write index using self.ref_size bytes
        match self.ref_size {
            // we check if index fits in ref_size?
            // self.ref_size is fixed to 2.
            // But we checked number of objects <= 65535 in encode().
            1 => {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "Index size checked elsewhere"
                )]
                output.push(index as u8);
            }
            2 => {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "Index size checked elsewhere"
                )]
                output.extend_from_slice(&(index as u16).to_be_bytes());
            }
            _ => return Err(PlistEncodeError::ValueTooLarge), // Not supporting > 65535 yet
        }
        Ok(())
    }

    fn write_sized_int(output: &mut Vec<u8>, value: u64, size: u8) {
        match size {
            1 => {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "Value fits in u8 based on size param"
                )]
                output.push(value as u8);
            }
            2 => {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "Value fits in u16 based on size param"
                )]
                output.extend_from_slice(&(value as u16).to_be_bytes());
            }
            4 => {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "Value fits in u32 based on size param"
                )]
                output.extend_from_slice(&(value as u32).to_be_bytes());
            }
            8 => output.extend_from_slice(&value.to_be_bytes()),
            _ => panic!("Invalid size"),
        }
    }

    fn calculate_offset_size(max_offset: usize) -> u8 {
        if max_offset <= 0xFF {
            1
        } else if max_offset <= 0xFFFF {
            2
        } else if max_offset <= 0xFFFF_FFFF {
            4
        } else {
            8
        }
    }

    fn write_trailer(
        &self,
        output: &mut Vec<u8>,
        offset_size: u8,
        num_objects: usize,
        root: usize,
        offset_table_offset: usize,
    ) {
        // 32 bytes
        // 5 unused
        output.extend_from_slice(&[0; 5]);
        // sort version
        output.push(0);
        // offset size
        output.push(offset_size);
        // object ref size
        output.push(self.ref_size);
        // num objects (8 bytes)
        output.extend_from_slice(&(num_objects as u64).to_be_bytes());
        // root index (8 bytes)
        output.extend_from_slice(&(root as u64).to_be_bytes());
        // offset table offset (8 bytes)
        output.extend_from_slice(&(offset_table_offset as u64).to_be_bytes());
    }
}
