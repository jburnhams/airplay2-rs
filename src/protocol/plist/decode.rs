use std::collections::{HashMap, HashSet};

use thiserror::Error;

use super::PlistValue;

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

/// Decode binary plist data into a `PlistValue`
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
    let decoder = Decoder::new(data, &trailer)?;

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
        let start = usize::try_from(trailer.offset_table_offset)
            .map_err(|_| PlistDecodeError::InvalidTrailer)?;
        let entry_size = trailer.offset_size as usize;
        let count =
            usize::try_from(trailer.num_objects).map_err(|_| PlistDecodeError::InvalidTrailer)?;

        if start + count * entry_size > data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: start + count * entry_size,
                have: data.len(),
            });
        }

        let mut offsets = Vec::with_capacity(count);

        for i in 0..count {
            let offset_start = start + i * entry_size;
            let offset =
                Self::read_sized_int(&data[offset_start..offset_start + entry_size], entry_size)?;
            offsets.push(offset);
        }

        Ok(offsets)
    }

    fn read_sized_int(data: &[u8], size: usize) -> Result<u64, PlistDecodeError> {
        match size {
            1 => Ok(u64::from(data[0])),
            2 => Ok(u64::from(u16::from_be_bytes(data[..2].try_into().unwrap()))),
            4 => Ok(u64::from(u32::from_be_bytes(data[..4].try_into().unwrap()))),
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

        let index_usize =
            usize::try_from(index).map_err(|_| PlistDecodeError::InvalidOffset(index))?;
        let offset = *self
            .offset_table
            .get(index_usize)
            .ok_or(PlistDecodeError::InvalidOffset(index))?;

        let pos = usize::try_from(offset).map_err(|_| PlistDecodeError::InvalidOffset(offset))?;
        if pos >= self.data.len() {
            return Err(PlistDecodeError::InvalidOffset(offset));
        }
        let marker = self.data[pos];

        let value = self.decode_value(marker, pos + 1, seen)?;

        seen.remove(&index);
        Ok(value)
    }

    fn decode_value(
        &self,
        marker: u8,
        pos: usize,
        seen: &mut HashSet<u64>,
    ) -> Result<PlistValue, PlistDecodeError> {
        let high_nibble = marker >> 4;
        let low_nibble = marker & 0x0F;

        match high_nibble {
            0x0 => Self::decode_singleton(low_nibble),
            0x1 => self.decode_integer(pos, low_nibble),
            0x2 => self.decode_real(pos, low_nibble),
            0x3 => self.decode_date(pos),
            0x4 => self.decode_data(pos, low_nibble),
            0x5 => self.decode_ascii_string(pos, low_nibble),
            0x6 => self.decode_utf16_string(pos, low_nibble),
            0x8 => self.decode_uid(pos, low_nibble),
            0xA => self.decode_array(pos, low_nibble, seen),
            0xD => self.decode_dictionary(pos, low_nibble, seen),
            _ => Err(PlistDecodeError::InvalidObjectMarker(marker)),
        }
    }

    fn decode_singleton(nibble: u8) -> Result<PlistValue, PlistDecodeError> {
        match nibble {
            0x0 | 0xF => Ok(PlistValue::Data(vec![])),
            0x8 => Ok(PlistValue::Boolean(false)),
            0x9 => Ok(PlistValue::Boolean(true)),
            _ => Err(PlistDecodeError::InvalidObjectMarker(nibble)),
        }
    }

    fn decode_integer(&self, pos: usize, size_exp: u8) -> Result<PlistValue, PlistDecodeError> {
        let bytes_len = 1 << size_exp;
        if pos + bytes_len > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: pos + bytes_len,
                have: self.data.len(),
            });
        }
        let int_bytes = &self.data[pos..pos + bytes_len];

        match bytes_len {
            #[allow(
                clippy::cast_possible_wrap,
                reason = "Interpret byte as signed integer"
            )]
            1 => Ok(PlistValue::Integer(i64::from(int_bytes[0] as i8))),
            2 => Ok(PlistValue::Integer(i64::from(i16::from_be_bytes(
                int_bytes.try_into().unwrap(),
            )))),
            4 => Ok(PlistValue::Integer(i64::from(i32::from_be_bytes(
                int_bytes.try_into().unwrap(),
            )))),
            8 => Ok(PlistValue::Integer(i64::from_be_bytes(
                int_bytes.try_into().unwrap(),
            ))),
            16 => {
                let val = u128::from_be_bytes(int_bytes.try_into().unwrap());
                if val <= u128::from(u64::MAX) {
                    #[allow(clippy::cast_possible_truncation, reason = "Checked to fit in u64")]
                    Ok(PlistValue::UnsignedInteger(val as u64))
                } else {
                    Err(PlistDecodeError::IntegerOverflow)
                }
            }
            _ => Err(PlistDecodeError::IntegerOverflow),
        }
    }

    fn decode_real(&self, pos: usize, size_exp: u8) -> Result<PlistValue, PlistDecodeError> {
        let bytes_len = 1 << size_exp;
        if pos + bytes_len > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: pos + bytes_len,
                have: self.data.len(),
            });
        }
        let real_bytes = &self.data[pos..pos + bytes_len];

        match bytes_len {
            4 => Ok(PlistValue::Real(f64::from(f32::from_be_bytes(
                real_bytes.try_into().unwrap(),
            )))),
            8 => Ok(PlistValue::Real(f64::from_be_bytes(
                real_bytes.try_into().unwrap(),
            ))),
            _ => Err(PlistDecodeError::UnsupportedType(
                "Real size not 4 or 8".into(),
            )),
        }
    }

    fn decode_date(&self, pos: usize) -> Result<PlistValue, PlistDecodeError> {
        if pos + 8 > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: pos + 8,
                have: self.data.len(),
            });
        }
        let date_bytes = &self.data[pos..pos + 8];
        let val = f64::from_be_bytes(date_bytes.try_into().unwrap());
        Ok(PlistValue::Date(val))
    }

    fn decode_size(&self, pos: usize, nibble: u8) -> Result<(usize, usize), PlistDecodeError> {
        if nibble == 0xF {
            if pos >= self.data.len() {
                return Err(PlistDecodeError::BufferTooSmall {
                    needed: pos + 1,
                    have: self.data.len(),
                });
            }
            let marker = self.data[pos];
            let next_nibble = marker & 0x0F;
            if (marker >> 4) != 0x1 {
                return Err(PlistDecodeError::InvalidObjectMarker(marker));
            }
            let size_exp = next_nibble;
            let bytes_len = 1 << size_exp;

            if pos + 1 + bytes_len > self.data.len() {
                return Err(PlistDecodeError::BufferTooSmall {
                    needed: pos + 1 + bytes_len,
                    have: self.data.len(),
                });
            }

            let int_val = match bytes_len {
                1 => u64::from(self.data[pos + 1]),
                2 => u64::from(u16::from_be_bytes(
                    self.data[pos + 1..pos + 1 + 2].try_into().unwrap(),
                )),
                4 => u64::from(u32::from_be_bytes(
                    self.data[pos + 1..pos + 1 + 4].try_into().unwrap(),
                )),
                8 => u64::from_be_bytes(self.data[pos + 1..pos + 1 + 8].try_into().unwrap()),
                _ => return Err(PlistDecodeError::IntegerOverflow),
            };

            let size = usize::try_from(int_val).map_err(|_| PlistDecodeError::IntegerOverflow)?;
            Ok((size, pos + 1 + bytes_len))
        } else {
            Ok((nibble as usize, pos))
        }
    }

    fn decode_data(&self, pos: usize, length_nibble: u8) -> Result<PlistValue, PlistDecodeError> {
        let (len, data_start) = self.decode_size(pos, length_nibble)?;

        if data_start + len > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: data_start + len,
                have: self.data.len(),
            });
        }

        Ok(PlistValue::Data(
            self.data[data_start..data_start + len].to_vec(),
        ))
    }

    fn decode_ascii_string(
        &self,
        pos: usize,
        len_nibble: u8,
    ) -> Result<PlistValue, PlistDecodeError> {
        let (len, str_start) = self.decode_size(pos, len_nibble)?;

        if str_start + len > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: str_start + len,
                have: self.data.len(),
            });
        }

        let s = std::str::from_utf8(&self.data[str_start..str_start + len])
            .map_err(|_| PlistDecodeError::InvalidUtf8)?;

        Ok(PlistValue::String(s.to_string()))
    }

    fn decode_utf16_string(
        &self,
        pos: usize,
        len_nibble: u8,
    ) -> Result<PlistValue, PlistDecodeError> {
        let (len, str_start) = self.decode_size(pos, len_nibble)?;

        let byte_len = len * 2;

        if str_start + byte_len > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: str_start + byte_len,
                have: self.data.len(),
            });
        }

        let bytes = &self.data[str_start..str_start + byte_len];
        let u16s: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes(c.try_into().unwrap()))
            .collect();

        let s = String::from_utf16(&u16s).map_err(|_| PlistDecodeError::InvalidUtf8)?;

        Ok(PlistValue::String(s))
    }

    fn decode_uid(&self, pos: usize, len_nibble: u8) -> Result<PlistValue, PlistDecodeError> {
        let len = (len_nibble + 1) as usize;
        if pos + len > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: pos + len,
                have: self.data.len(),
            });
        }

        let mut val = 0u64;
        for i in 0..len {
            val = (val << 8) | u64::from(self.data[pos + i]);
        }

        Ok(PlistValue::Uid(val))
    }

    fn decode_array(
        &self,
        pos: usize,
        count_nibble: u8,
        seen: &mut HashSet<u64>,
    ) -> Result<PlistValue, PlistDecodeError> {
        let (count, refs_start) = self.decode_size(pos, count_nibble)?;

        if refs_start + count * self.object_ref_size > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: refs_start + count * self.object_ref_size,
                have: self.data.len(),
            });
        }

        let mut items = Vec::with_capacity(count);

        for i in 0..count {
            let curr_ref_offset = refs_start + i * self.object_ref_size;
            let index = Self::read_sized_int(
                &self.data[curr_ref_offset..curr_ref_offset + self.object_ref_size],
                self.object_ref_size,
            )?;

            items.push(self.decode_object(index, seen)?);
        }

        Ok(PlistValue::Array(items))
    }

    fn decode_dictionary(
        &self,
        pos: usize,
        count_nibble: u8,
        seen: &mut HashSet<u64>,
    ) -> Result<PlistValue, PlistDecodeError> {
        let (count, refs_start) = self.decode_size(pos, count_nibble)?;

        if refs_start + count * 2 * self.object_ref_size > self.data.len() {
            return Err(PlistDecodeError::BufferTooSmall {
                needed: refs_start + count * 2 * self.object_ref_size,
                have: self.data.len(),
            });
        }

        let mut dict = HashMap::with_capacity(count);

        for i in 0..count {
            let key_ref_start = refs_start + i * self.object_ref_size;
            let val_ref_start = refs_start + (count + i) * self.object_ref_size;

            let key_index = Self::read_sized_int(
                &self.data[key_ref_start..key_ref_start + self.object_ref_size],
                self.object_ref_size,
            )?;

            let val_index = Self::read_sized_int(
                &self.data[val_ref_start..val_ref_start + self.object_ref_size],
                self.object_ref_size,
            )?;

            let key_val = self.decode_object(key_index, seen)?;
            let PlistValue::String(key_str) = key_val else {
                return Err(PlistDecodeError::UnsupportedType(
                    "Dictionary key must be a string".into(),
                ));
            };

            let val_val = self.decode_object(val_index, seen)?;
            dict.insert(key_str, val_val);
        }

        Ok(PlistValue::Dictionary(dict))
    }
}
