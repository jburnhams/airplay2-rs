//! Album artwork handling

/// Album artwork
#[derive(Debug, Clone)]
pub struct Artwork {
    /// Image data (JPEG or PNG)
    pub data: Vec<u8>,
    /// MIME type
    pub mime_type: String,
    /// Width (if known)
    pub width: Option<u32>,
    /// Height (if known)
    pub height: Option<u32>,
}

impl Artwork {
    /// Create from raw image data
    pub fn from_data(data: Vec<u8>) -> Option<Self> {
        let mime_type = detect_image_type(&data)?;

        Some(Self {
            data,
            mime_type,
            width: None,
            height: None,
        })
    }

    /// Check if artwork is JPEG
    pub fn is_jpeg(&self) -> bool {
        self.mime_type == "image/jpeg"
    }

    /// Check if artwork is PNG
    pub fn is_png(&self) -> bool {
        self.mime_type == "image/png"
    }
}

/// Detect image type from magic bytes
fn detect_image_type(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }

    // JPEG: starts with FF D8 FF
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some("image/jpeg".to_string());
    }

    // PNG: starts with 89 50 4E 47
    if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        return Some("image/png".to_string());
    }

    None
}

/// Parse artwork from SET_PARAMETER body
pub fn parse_artwork(content_type: &str, data: &[u8]) -> Option<Artwork> {
    if content_type.contains("image/jpeg") || content_type.contains("image/png") {
        Artwork::from_data(data.to_vec())
    } else {
        None
    }
}
