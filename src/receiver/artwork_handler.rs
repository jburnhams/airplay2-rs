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
    #[must_use]
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
    #[must_use]
    pub fn is_jpeg(&self) -> bool {
        self.mime_type == "image/jpeg"
    }

    /// Check if artwork is PNG
    #[must_use]
    pub fn is_png(&self) -> bool {
        self.mime_type == "image/png"
    }
}

/// Detect image type from magic bytes
fn detect_image_type(data: &[u8]) -> Option<String> {
    match data {
        // JPEG: starts with FF D8 FF
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg".to_string()),
        // PNG: starts with 89 50 4E 47
        [0x89, 0x50, 0x4E, 0x47, ..] => Some("image/png".to_string()),
        _ => None,
    }
}

/// Parse artwork from `SET_PARAMETER` body
#[must_use]
pub fn parse_artwork(content_type: &str, data: &[u8]) -> Option<Artwork> {
    if content_type.contains("image/jpeg") || content_type.contains("image/png") {
        Artwork::from_data(data.to_vec())
    } else {
        None
    }
}
