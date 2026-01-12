//! Lock-free ring buffer for audio data

use ringbuf::{traits::*, HeapRb};

/// Audio ring buffer for lock-free audio distribution
pub struct AudioRingBuffer {
    producer: ringbuf::HeapProd<f32>,
    consumer: ringbuf::HeapCons<f32>,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with the given capacity
    pub fn new(capacity: usize) -> Self {
        let rb = HeapRb::<f32>::new(capacity);
        let (producer, consumer) = rb.split();
        Self { producer, consumer }
    }

    /// Push samples into the buffer (non-blocking)
    pub fn push(&mut self, samples: &[f32]) -> usize {
        self.producer.push_slice(samples)
    }

    /// Pop samples from the buffer (non-blocking)
    pub fn pop(&mut self, buffer: &mut [f32]) -> usize {
        self.consumer.pop_slice(buffer)
    }

    /// Get the number of samples available to read
    pub fn available(&self) -> usize {
        self.consumer.occupied_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_creation() {
        let rb = AudioRingBuffer::new(1024);
        assert_eq!(rb.available(), 0);
    }

    #[test]
    fn test_push_and_pop() {
        let mut rb = AudioRingBuffer::new(1024);

        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let pushed = rb.push(&samples);
        assert_eq!(pushed, 5);
        assert_eq!(rb.available(), 5);

        let mut output = vec![0.0; 5];
        let popped = rb.pop(&mut output);
        assert_eq!(popped, 5);
        assert_eq!(output, samples);
        assert_eq!(rb.available(), 0);
    }

    #[test]
    fn test_partial_pop() {
        let mut rb = AudioRingBuffer::new(1024);

        let samples = vec![1.0, 2.0, 3.0, 4.0];
        rb.push(&samples);

        let mut output = vec![0.0; 2];
        let popped = rb.pop(&mut output);
        assert_eq!(popped, 2);
        assert_eq!(output, vec![1.0, 2.0]);
        assert_eq!(rb.available(), 2);

        let popped = rb.pop(&mut output);
        assert_eq!(popped, 2);
        assert_eq!(output, vec![3.0, 4.0]);
        assert_eq!(rb.available(), 0);
    }

    #[test]
    fn test_overflow_behavior() {
        let mut rb = AudioRingBuffer::new(4);

        // Try to push more than capacity
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let pushed = rb.push(&samples);
        // Should only push what fits
        assert!(pushed <= 4);
    }

    #[test]
    fn test_empty_pop() {
        let mut rb = AudioRingBuffer::new(1024);
        let mut output = vec![0.0; 10];
        let popped = rb.pop(&mut output);
        assert_eq!(popped, 0);
    }

    #[test]
    fn test_audio_range_values() {
        let mut rb = AudioRingBuffer::new(1024);

        // Test with typical audio values (-1.0 to 1.0)
        let samples: Vec<f32> = (-10..=10).map(|i| i as f32 / 10.0).collect();
        let pushed = rb.push(&samples);
        assert_eq!(pushed, 21);

        let mut output = vec![0.0; 21];
        rb.pop(&mut output);

        // Verify values preserved
        assert!((output[0] - (-1.0)).abs() < 0.001);
        assert!((output[10] - 0.0).abs() < 0.001);
        assert!((output[20] - 1.0).abs() < 0.001);
    }
}
