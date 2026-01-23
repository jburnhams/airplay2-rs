//! Audio ring buffer implementation

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Lock-free ring buffer for audio samples
///
/// # Safety
/// This structure uses `UnsafeCell` for interior mutability.
/// It is designed for Single-Producer Single-Consumer (SPSC) usage.
/// The `write` method must only be called by one thread at a time.
/// The `read`, `peek`, `skip` methods must only be called by one thread at a time (the consumer).
pub struct AudioRingBuffer {
    /// Buffer storage
    data: UnsafeCell<Vec<u8>>,
    /// Buffer capacity in bytes
    capacity: usize,
    /// Read position
    read_pos: AtomicUsize,
    /// Write position
    write_pos: AtomicUsize,
    /// High watermark for buffering
    high_watermark: usize,
    /// Low watermark (trigger underrun warning)
    low_watermark: usize,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with given capacity
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            data: UnsafeCell::new(vec![0u8; capacity]),
            capacity,
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
            high_watermark: capacity * 3 / 4,
            low_watermark: capacity / 4,
        }
    }

    /// Create with custom watermarks
    ///
    /// # Panics
    ///
    /// Panics if `low >= high` or `high > capacity`.
    #[must_use]
    pub fn with_watermarks(capacity: usize, low: usize, high: usize) -> Self {
        assert!(low < high && high <= capacity);
        Self {
            data: UnsafeCell::new(vec![0u8; capacity]),
            capacity,
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
            high_watermark: high,
            low_watermark: low,
        }
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get current fill level
    pub fn available(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);

        if write >= read {
            write - read
        } else {
            self.capacity - read + write
        }
    }

    /// Get free space
    pub fn free(&self) -> usize {
        self.capacity - self.available() - 1
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.free() == 0
    }

    /// Check if below low watermark
    pub fn is_underrunning(&self) -> bool {
        self.available() < self.low_watermark
    }

    /// Check if above high watermark
    pub fn is_ready(&self) -> bool {
        self.available() >= self.high_watermark
    }

    /// Write data to buffer
    ///
    /// Returns number of bytes written
    ///
    /// # Safety
    /// This method is not thread-safe with respect to other writers.
    /// Only one producer thread should call this.
    pub fn write(&self, data: &[u8]) -> usize {
        let available_space = self.free();
        let to_write = data.len().min(available_space);

        if to_write == 0 {
            return 0;
        }

        let write_pos = self.write_pos.load(Ordering::Acquire);

        // Safety: We access the UnsafeCell.
        // We assume SPSC, so we are the only writer.
        // The reader reads from other parts of the buffer (controlled by atomic indices),
        // or same parts but temporal safety is ensured by indices.
        // `data` is a Vec<u8>. We need the pointer to its buffer.
        let data_ptr = unsafe { (*self.data.get()).as_mut_ptr() };

        let first_part = (self.capacity - write_pos).min(to_write);
        let second_part = to_write - first_part;

        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr.add(write_pos), first_part);

            if second_part > 0 {
                std::ptr::copy_nonoverlapping(data.as_ptr().add(first_part), data_ptr, second_part);
            }
        }

        let new_write_pos = (write_pos + to_write) % self.capacity;
        self.write_pos.store(new_write_pos, Ordering::Release);

        to_write
    }

    /// Read data from buffer
    ///
    /// Returns number of bytes read
    pub fn read(&self, output: &mut [u8]) -> usize {
        let available = self.available();
        let to_read = output.len().min(available);

        if to_read == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);

        let first_part = (self.capacity - read_pos).min(to_read);
        let second_part = to_read - first_part;

        // Safety: We access the UnsafeCell for reading.
        // We assume SPSC. The writer writes to other parts.
        let buffer_slice = unsafe { (*self.data.get()).as_slice() };

        output[..first_part].copy_from_slice(&buffer_slice[read_pos..read_pos + first_part]);

        if second_part > 0 {
            output[first_part..to_read].copy_from_slice(&buffer_slice[..second_part]);
        }

        let new_read_pos = (read_pos + to_read) % self.capacity;
        self.read_pos.store(new_read_pos, Ordering::Release);

        to_read
    }

    /// Peek at data without consuming
    pub fn peek(&self, output: &mut [u8]) -> usize {
        let available = self.available();
        let to_peek = output.len().min(available);

        if to_peek == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);

        let first_part = (self.capacity - read_pos).min(to_peek);
        let second_part = to_peek - first_part;

        // Safety: We access the UnsafeCell for reading.
        let buffer_slice = unsafe { (*self.data.get()).as_slice() };

        output[..first_part].copy_from_slice(&buffer_slice[read_pos..read_pos + first_part]);

        if second_part > 0 {
            output[first_part..to_peek].copy_from_slice(&buffer_slice[..second_part]);
        }

        to_peek
    }

    /// Skip/discard bytes from read position
    pub fn skip(&self, count: usize) -> usize {
        let available = self.available();
        let to_skip = count.min(available);

        let read_pos = self.read_pos.load(Ordering::Acquire);
        let new_read_pos = (read_pos + to_skip) % self.capacity;
        self.read_pos.store(new_read_pos, Ordering::Release);

        to_skip
    }

    /// Clear the buffer
    pub fn clear(&self) {
        self.read_pos.store(0, Ordering::Release);
        self.write_pos.store(0, Ordering::Release);
    }
}

// Thread safety
unsafe impl Send for AudioRingBuffer {}
unsafe impl Sync for AudioRingBuffer {}
