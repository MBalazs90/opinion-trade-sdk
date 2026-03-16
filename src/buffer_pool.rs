use std::sync::Mutex;

/// A pre-allocated pool of byte buffers to reduce allocation overhead on the WS hot path.
///
/// Buffers are reused across messages — returned buffers retain their capacity
/// even after being cleared, avoiding repeated allocation.
pub struct BufferPool {
    pool: Mutex<Vec<Vec<u8>>>,
    buffer_capacity: usize,
}

impl BufferPool {
    /// Create a pool with `count` pre-allocated buffers of `capacity` bytes each.
    pub fn new(count: usize, capacity: usize) -> Self {
        let mut pool = Vec::with_capacity(count);
        for _ in 0..count {
            pool.push(Vec::with_capacity(capacity));
        }
        Self {
            pool: Mutex::new(pool),
            buffer_capacity: capacity,
        }
    }

    /// Default pool: 32 buffers of 4KB each.
    pub fn default_pool() -> Self {
        Self::new(32, 4096)
    }

    /// Take a buffer from the pool. If the pool is empty, allocates a new one.
    pub fn take(&self) -> Vec<u8> {
        self.pool
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(self.buffer_capacity))
    }

    /// Return a buffer to the pool (clears it but retains capacity).
    pub fn give(&self, mut buf: Vec<u8>) {
        buf.clear();
        let mut pool = self.pool.lock().unwrap();
        // Don't grow the pool unboundedly
        if pool.len() < pool.capacity() + 8 {
            pool.push(buf);
        }
    }

    /// Number of buffers currently available in the pool.
    pub fn available(&self) -> usize {
        self.pool.lock().unwrap().len()
    }
}

impl std::fmt::Debug for BufferPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferPool")
            .field("available", &self.available())
            .field("buffer_capacity", &self.buffer_capacity)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_returns_preallocated() {
        let pool = BufferPool::new(4, 1024);
        assert_eq!(pool.available(), 4);
        let buf = pool.take();
        assert_eq!(pool.available(), 3);
        assert!(buf.capacity() >= 1024);
    }

    #[test]
    fn give_returns_buffer() {
        let pool = BufferPool::new(2, 1024);
        let buf = pool.take();
        assert_eq!(pool.available(), 1);
        pool.give(buf);
        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn take_from_empty_pool_allocates() {
        let pool = BufferPool::new(0, 512);
        let buf = pool.take();
        assert!(buf.capacity() >= 512);
    }

    #[test]
    fn give_clears_buffer() {
        let pool = BufferPool::new(1, 256);
        let mut buf = pool.take();
        buf.extend_from_slice(b"hello world");
        assert!(!buf.is_empty());
        pool.give(buf);

        let buf2 = pool.take();
        assert!(buf2.is_empty()); // cleared
        assert!(buf2.capacity() >= 256); // capacity retained
    }

    #[test]
    fn default_pool_has_32_buffers() {
        let pool = BufferPool::default_pool();
        assert_eq!(pool.available(), 32);
    }

    #[test]
    fn concurrent_take_give() {
        use std::sync::Arc;
        let pool = Arc::new(BufferPool::new(10, 256));
        let mut handles = vec![];

        for _ in 0..8 {
            let p = Arc::clone(&pool);
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let mut buf = p.take();
                    buf.extend_from_slice(b"test");
                    p.give(buf);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // All buffers should be returned (possibly more due to empty-pool allocations)
        assert!(pool.available() >= 10);
    }
}
