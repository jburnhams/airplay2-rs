use crate::protocol::daap::{Artwork, ArtworkFormat};

#[test]
fn test_detect_format() {
    let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
    assert_eq!(ArtworkFormat::detect(&jpeg_data), Some(ArtworkFormat::Jpeg));

    let png_data = vec![0x89, 0x50, 0x4E, 0x47];
    assert_eq!(ArtworkFormat::detect(&png_data), Some(ArtworkFormat::Png));

    let invalid_data = vec![0x00, 0x00, 0x00, 0x00];
    assert_eq!(ArtworkFormat::detect(&invalid_data), None);
}

#[test]
fn test_jpeg_dimensions() {
    // Minimal JPEG header structure to test dimension parsing
    let mut data = vec![0xFF, 0xD8]; // SOI

    // APP0
    data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    data.extend_from_slice(b"JFIF\0");
    data.extend_from_slice(&[0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]);

    // SOF0 (Baseline DCT)
    // Marker: FF C0
    // Length: 00 11 (17 bytes)
    // Precision: 08
    // Height: 01 90 (400)
    // Width: 02 58 (600)
    // Components: 03 ...
    data.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0x90, 0x02, 0x58, 0x03]);

    let artwork = Artwork::jpeg(data);
    assert_eq!(artwork.dimensions(), Some((600, 400)));
}

#[test]
fn test_png_dimensions() {
    // Minimal PNG header
    let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    // IHDR chunk
    // Length: 00 00 00 0D (13)
    // Type: IHDR
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D]);
    data.extend_from_slice(b"IHDR");

    // Width: 00 00 03 20 (800)
    // Height: 00 00 02 58 (600)
    data.extend_from_slice(&[0x00, 0x00, 0x03, 0x20]);
    data.extend_from_slice(&[0x00, 0x00, 0x02, 0x58]);

    // Bit depth, color type, etc.
    data.extend_from_slice(&[0x08, 0x02, 0x00, 0x00, 0x00]);

    // CRC (ignored by our parser but added for completeness padding)
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    let artwork = Artwork::png(data);
    assert_eq!(artwork.dimensions(), Some((800, 600)));
}
