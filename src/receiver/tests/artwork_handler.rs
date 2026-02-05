use crate::receiver::artwork_handler::Artwork;

#[test]
fn test_artwork_detection() {
    // JPEG magic bytes
    let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    let artwork = Artwork::from_data(jpeg_data).unwrap();
    assert!(artwork.is_jpeg());

    // PNG magic bytes
    let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
    let artwork = Artwork::from_data(png_data).unwrap();
    assert!(artwork.is_png());
}

#[test]
fn test_artwork_detection_short_jpeg() {
    // Short JPEG buffer (just magic bytes)
    let jpeg_data = vec![0xFF, 0xD8, 0xFF];
    let artwork = Artwork::from_data(jpeg_data).unwrap();
    assert!(artwork.is_jpeg());
}
