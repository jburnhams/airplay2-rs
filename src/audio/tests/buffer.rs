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

#[test]
fn test_buffer_clear_and_wrap() {
    let buffer = AudioRingBuffer::new(10);
    // Write 8 bytes
    buffer.write(&[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(buffer.available(), 8);

    // Read 6 bytes to advance the read pointer, causing the next write to wrap
    let mut out = vec![0u8; 6];
    buffer.read(&mut out);
    assert_eq!(out, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(buffer.available(), 2);

    // Write 5 bytes, this should wrap around the end of the buffer
    let written = buffer.write(&[9, 10, 11, 12, 13]);
    assert_eq!(written, 5);
    assert_eq!(buffer.available(), 7);

    // Clear the buffer
    buffer.clear();
    assert_eq!(buffer.available(), 0);
    assert_eq!(buffer.free(), 9); // capacity 10 - 1 = 9

    // Write again after clear
    let written = buffer.write(&[100, 101, 102]);
    assert_eq!(written, 3);
    assert_eq!(buffer.available(), 3);

    let mut out = vec![0u8; 3];
    buffer.read(&mut out);
    assert_eq!(out, vec![100, 101, 102]);
}

#[test]
fn test_buffer_wrapping_randomized() {
    use rand::Rng;

    let buffer = AudioRingBuffer::new(100);
    let mut rng = rand::thread_rng();

    let mut current_val: u8 = 0;
    let mut expected_val: u8 = 0;

    for _ in 0..1000 {
        // Randomly choose write or read action
        // Bias towards writing if empty, reading if full
        let available = buffer.available();
        let should_write = if available == 0 {
            true
        } else if buffer.free() == 0 {
            false
        } else {
            rng.gen_bool(0.5)
        };

        if should_write {
            let space = buffer.free();
            let write_size = rng.gen_range(1..=space.max(1));
            let mut data = Vec::with_capacity(write_size);
            for _ in 0..write_size {
                data.push(current_val);
                current_val = current_val.wrapping_add(1);
            }
            buffer.write(&data);
        } else {
            let available = buffer.available();
            let read_size = rng.gen_range(1..=available.max(1));
            let mut out = vec![0u8; read_size];
            let n = buffer.read(&mut out);

            for &b in &out[..n] {
                assert_eq!(b, expected_val);
                expected_val = expected_val.wrapping_add(1);
            }
        }
    }
}
