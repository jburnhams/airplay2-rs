#[derive(Debug, PartialEq)]
pub enum DmapTestValue {
    String(String),
    Int(i64),
    Container(Vec<(String, DmapTestValue)>),
    Raw(Vec<u8>),
}

/// Helper to decode DMAP data for verification
pub fn decode_dmap_full(data: &[u8]) -> Result<Vec<(String, DmapTestValue)>, String> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        if pos + 8 > data.len() {
            return Err("Unexpected end of data header".to_string());
        }

        let tag = std::str::from_utf8(&data[pos..pos + 4])
            .map_err(|_| "Invalid tag encoding".to_string())?
            .to_string();

        let len = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;

        pos += 8;

        if pos + len > data.len() {
            return Err("Unexpected end of data body".to_string());
        }

        let value_bytes = &data[pos..pos + len];

        // Heuristic decoding based on tag
        let value = match tag.as_str() {
            // Containers
            "mlcl" | "mlit" | "adbs" => {
                let inner = decode_dmap_full(value_bytes)?;
                DmapTestValue::Container(inner)
            }
            // Known integers (variable length)
            "astn" | "asdn" | "asyr" | "astm" => match len {
                1 => DmapTestValue::Int(i64::from(value_bytes[0])),
                2 => DmapTestValue::Int(i64::from(i16::from_be_bytes([
                    value_bytes[0],
                    value_bytes[1],
                ]))),
                4 => DmapTestValue::Int(i64::from(i32::from_be_bytes([
                    value_bytes[0],
                    value_bytes[1],
                    value_bytes[2],
                    value_bytes[3],
                ]))),
                8 => DmapTestValue::Int(i64::from_be_bytes([
                    value_bytes[0],
                    value_bytes[1],
                    value_bytes[2],
                    value_bytes[3],
                    value_bytes[4],
                    value_bytes[5],
                    value_bytes[6],
                    value_bytes[7],
                ])),
                _ => return Err(format!("Invalid integer length for {tag}: {len}")),
            },
            // Known strings
            "minm" | "asar" | "asal" | "asgn" => {
                let s = String::from_utf8(value_bytes.to_vec())
                    .map_err(|_| "Invalid UTF-8 string".to_string())?;
                DmapTestValue::String(s)
            }
            _ => {
                // For other tags, try to decode as string if ASCII, otherwise Raw
                if !value_bytes.is_empty()
                    && value_bytes
                        .iter()
                        .all(|&b| b.is_ascii_graphic() || b == b' ')
                {
                    if let Ok(s) = String::from_utf8(value_bytes.to_vec()) {
                        DmapTestValue::String(s)
                    } else {
                        DmapTestValue::Raw(value_bytes.to_vec())
                    }
                } else {
                    DmapTestValue::Raw(value_bytes.to_vec())
                }
            }
        };

        result.push((tag, value));
        pos += len;
    }

    Ok(result)
}
