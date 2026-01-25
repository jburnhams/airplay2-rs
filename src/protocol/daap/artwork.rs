//! Album artwork for RAOP

/// Artwork image format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkFormat {
    /// JPEG image
    Jpeg,
    /// PNG image
    Png,
}

impl ArtworkFormat {
    /// Get MIME type
    #[must_use]
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }

    /// Detect format from data
    #[must_use]
    pub fn detect(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        // JPEG magic bytes
        if data[0..2] == [0xFF, 0xD8] {
            return Some(Self::Jpeg);
        }

        // PNG magic bytes
        if data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
            return Some(Self::Png);
        }

        None
    }
}

/// Album artwork
#[derive(Debug, Clone)]
pub struct Artwork {
    /// Image data
    pub data: Vec<u8>,
    /// Image format
    pub format: ArtworkFormat,
}

impl Artwork {
    /// Create artwork from JPEG data
    #[must_use]
    pub fn jpeg(data: Vec<u8>) -> Self {
        Self {
            data,
            format: ArtworkFormat::Jpeg,
        }
    }

    /// Create artwork from PNG data
    #[must_use]
    pub fn png(data: Vec<u8>) -> Self {
        Self {
            data,
            format: ArtworkFormat::Png,
        }
    }

    /// Create artwork with auto-detected format
    #[must_use]
    pub fn from_data(data: Vec<u8>) -> Option<Self> {
        let format = ArtworkFormat::detect(&data)?;
        Some(Self { data, format })
    }

    /// Get MIME type for Content-Type header
    #[must_use]
    pub fn mime_type(&self) -> &'static str {
        self.format.mime_type()
    }

    /// Get image dimensions (basic parsing)
    #[must_use]
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match self.format {
            ArtworkFormat::Jpeg => self.jpeg_dimensions(),
            ArtworkFormat::Png => self.png_dimensions(),
        }
    }

    fn jpeg_dimensions(&self) -> Option<(u32, u32)> {
        // Simple JPEG dimension parser
        let mut pos = 2;

        while pos < self.data.len() - 4 {
            if self.data[pos] != 0xFF {
                pos += 1;
                continue;
            }

            let marker = self.data[pos + 1];

            // SOF markers contain dimensions
            if (0xC0..=0xCF).contains(&marker)
                && marker != 0xC4
                && marker != 0xC8
                && marker != 0xCC
                && pos + 9 < self.data.len()
            {
                let height =
                    u32::from(u16::from_be_bytes([self.data[pos + 5], self.data[pos + 6]]));
                let width = u32::from(u16::from_be_bytes([self.data[pos + 7], self.data[pos + 8]]));
                return Some((width, height));
            }

            // Skip to next marker
            if pos + 3 < self.data.len() {
                let len = u16::from_be_bytes([self.data[pos + 2], self.data[pos + 3]]) as usize;
                pos += 2 + len;
            } else {
                break;
            }
        }

        None
    }

    fn png_dimensions(&self) -> Option<(u32, u32)> {
        // PNG IHDR chunk contains dimensions at bytes 16-23
        if self.data.len() < 24 {
            return None;
        }

        let width =
            u32::from_be_bytes([self.data[16], self.data[17], self.data[18], self.data[19]]);
        let height =
            u32::from_be_bytes([self.data[20], self.data[21], self.data[22], self.data[23]]);

        Some((width, height))
    }
}
