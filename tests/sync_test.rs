use halo::concurrency::sync::GhostRingBuffer;
use std::thread;
use std::sync::Arc;

#[test]
fn test_ring_buffer_basic() {
    let buffer = GhostRingBuffer::<'static, i32>::new(4);
    assert!(buffer.push(1).is_ok());
    assert!(buffer.push(2).is_ok());
    assert_eq!(buffer.pop(), Some(1));
    assert_eq!(buffer.pop(), Some(2));
    assert_eq!(buffer.pop(), None);
}

#[test]
fn test_ring_buffer_full() {
    let buffer = GhostRingBuffer::<'static, i32>::new(2);
    assert!(buffer.push(1).is_ok());
    assert!(buffer.push(2).is_ok());
    assert!(buffer.push(3).is_err()); // Full
    assert_eq!(buffer.pop(), Some(1));
    assert!(buffer.push(3).is_ok());
    assert_eq!(buffer.pop(), Some(2));
    assert_eq!(buffer.pop(), Some(3));
}

#[test]
fn test_ring_buffer_concurrent() {
    let buffer = Arc::new(GhostRingBuffer::<'static, i32>::new(16));
    let b1 = buffer.clone();
    let b2 = buffer.clone();

    thread::scope(|s| {
        s.spawn(move || {
            for i in 0..100 {
                while b1.push(i).is_err() {
                    thread::yield_now();
                }
            }
        });

        s.spawn(move || {
            let mut count = 0;
            let mut sum = 0;
            while count < 100 {
                if let Some(i) = b2.pop() {
                    sum += i;
                    count += 1;
                } else {
                    thread::yield_now();
                }
            }
            assert_eq!(sum, (0..100).sum());
        });
    });
}
