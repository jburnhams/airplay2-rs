use crate::audio::buffer::*;

#[test]
fn test_write_read_simple() {
    let buffer = AudioRingBuffer::new(1024);

    let data = vec![1u8, 2, 3, 4, 5];
    let written = buffer.write(&data);
    assert_eq!(written, 5);
    assert_eq!(buffer.available(), 5);

    let mut output = vec![0u8; 5];
    let read = buffer.read(&mut output);
    assert_eq!(read, 5);
    assert_eq!(output, data);
}

#[test]
fn test_wraparound() {
    let buffer = AudioRingBuffer::new(8);

    // Write 5 bytes
    buffer.write(&[1, 2, 3, 4, 5]);
    // Read 3 bytes
    let mut out = vec![0u8; 3];
    buffer.read(&mut out);
    assert_eq!(out, vec![1, 2, 3]);

    // Write 5 more (should wrap)
    buffer.write(&[6, 7, 8, 9, 10]);

    // Read all
    let mut out = vec![0u8; 7];
    let n = buffer.read(&mut out);
    assert_eq!(n, 7);
    assert_eq!(out, vec![4, 5, 6, 7, 8, 9, 10]);
}

#[test]
fn test_peek() {
    let buffer = AudioRingBuffer::new(1024);
    buffer.write(&[1, 2, 3, 4, 5]);

    let mut out = vec![0u8; 3];
    let peeked = buffer.peek(&mut out);
    assert_eq!(peeked, 3);
    assert_eq!(out, vec![1, 2, 3]);

    // Data should still be there
    assert_eq!(buffer.available(), 5);
}
