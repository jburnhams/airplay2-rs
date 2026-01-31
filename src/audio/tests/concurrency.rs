use crate::audio::buffer::AudioRingBuffer;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_concurrent_producer_consumer() {
    let capacity = 1024 * 1024; // 1MB buffer
    let buffer = Arc::new(AudioRingBuffer::new(capacity));
    let buffer_clone = buffer.clone();

    let iteration_count = 1000;
    let chunk_size = 1024;

    // Producer thread
    let producer = thread::spawn(move || {
        for i in 0..iteration_count {
            #[allow(clippy::cast_possible_truncation)]
            let data = vec![(i % 255) as u8; chunk_size];
            let mut total_written = 0;
            while total_written < chunk_size {
                let written = buffer.write(&data[total_written..]);
                total_written += written;
                if written == 0 {
                    thread::sleep(Duration::from_micros(10));
                }
            }
        }
    });

    // Consumer thread
    let consumer = thread::spawn(move || {
        let mut total_read_bytes = 0;
        let expected_total = iteration_count * chunk_size;
        let mut temp_buf = vec![0u8; chunk_size];

        while total_read_bytes < expected_total {
            let read = buffer_clone.read(&mut temp_buf);
            if read > 0 {
                // Verify data
                for (j, byte) in temp_buf.iter().enumerate().take(read) {
                    let byte_index = total_read_bytes + j;
                    let chunk_index = byte_index / chunk_size;
                    #[allow(clippy::cast_possible_truncation)]
                    let expected_val = (chunk_index % 255) as u8;
                    assert_eq!(*byte, expected_val, "Mismatch at byte {byte_index}");
                }
                total_read_bytes += read;
            } else {
                thread::sleep(Duration::from_micros(10));
            }
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}
