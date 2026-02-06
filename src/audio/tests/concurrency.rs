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

#[test]
fn test_spsc_stress() {
    let capacity = 1024 * 16;
    let buffer = Arc::new(AudioRingBuffer::new(capacity));
    let buffer_reader = buffer.clone();

    // 10 MB total
    let total_bytes = 1024 * 1024 * 10;
    // Use smaller chunk size to force more partial writes/reads
    let chunk_size = 512;

    let writer_handle = thread::spawn(move || {
        let mut written_total = 0;
        let mut val: u8 = 0;

        while written_total < total_bytes {
            // Prepare a chunk of data
            let mut data = vec![0u8; chunk_size];
            for b in &mut data {
                *b = val;
                val = val.wrapping_add(1);
            }

            let mut chunk_written = 0;
            while chunk_written < chunk_size {
                let n = buffer.write(&data[chunk_written..]);
                chunk_written += n;
                if n == 0 {
                    thread::yield_now();
                }
            }
            written_total += chunk_size;
        }
    });

    let reader_handle = thread::spawn(move || {
        let mut read_total = 0;
        let mut temp_buf = vec![0u8; chunk_size];
        let mut expected_val: u8 = 0;

        while read_total < total_bytes {
            let n = buffer_reader.read(&mut temp_buf);
            if n > 0 {
                for &b in &temp_buf[..n] {
                    assert_eq!(b, expected_val, "Data corruption at index {read_total}");
                    expected_val = expected_val.wrapping_add(1);
                    read_total += 1;
                }
            } else {
                thread::yield_now();
            }
        }
    });

    writer_handle.join().unwrap();
    reader_handle.join().unwrap();
}
