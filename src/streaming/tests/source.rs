use crate::audio::AudioFormat;
use crate::streaming::{AudioSource, CallbackSource, SilenceSource, SliceSource};

#[test]
fn test_slice_source() {
    let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut source = SliceSource::new(data.clone(), AudioFormat::CD_QUALITY);

    let mut buffer = vec![0u8; 4];
    let n = source.read(&mut buffer).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buffer, vec![1, 2, 3, 4]);

    let n = source.read(&mut buffer).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buffer, vec![5, 6, 7, 8]);

    let n = source.read(&mut buffer).unwrap();
    assert_eq!(n, 0); // EOF
}

#[test]
fn test_silence_source() {
    let mut source = SilenceSource::new(AudioFormat::CD_QUALITY);

    let mut buffer = vec![255u8; 100];
    let n = source.read(&mut buffer).unwrap();

    assert_eq!(n, 100);
    assert!(buffer.iter().all(|&b| b == 0));
}

#[test]
fn test_callback_source() {
    let format = AudioFormat::CD_QUALITY;
    let mut counter = 0;
    let mut source = CallbackSource::new(format, move |buf: &mut [u8]| {
        counter += 1;
        buf.fill(counter);
        Ok(buf.len())
    });

    let mut buffer = vec![0u8; 4];
    source.read(&mut buffer).unwrap();
    assert_eq!(buffer, vec![1, 1, 1, 1]);

    source.read(&mut buffer).unwrap();
    assert_eq!(buffer, vec![2, 2, 2, 2]);
}
