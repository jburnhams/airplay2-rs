use crate::net::traits::AsyncRead;
use std::io::{Cursor, Result};
use std::pin::Pin;
use std::task::{Context, Poll};
#[cfg(feature = "tokio-runtime")]
use std::time::Duration;

// Mock reader for testing
struct MockReader {
    data: Cursor<Vec<u8>>,
}

impl AsyncRead for MockReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        use std::io::Read;
        Poll::Ready(self.data.read(buf))
    }
}

#[test]
fn test_mock_reader() {
    let mut reader = MockReader {
        data: Cursor::new(vec![1, 2, 3, 4]),
    };
    // Basic check that it implements the trait
    let _ = Pin::new(&mut reader);
}

#[cfg(feature = "tokio-runtime")]
mod tokio_tests {
    use super::*;
    use crate::net::Runtime;
    use crate::net::tokio_impl::{connect_tcp, spawn};

    #[tokio::test]
    async fn test_tcp_connect_invalid() {
        let result = connect_tcp("invalid:99999").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sleep() {
        let start = std::time::Instant::now();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(start.elapsed() >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_timeout_success() {
        let result = tokio::time::timeout(Duration::from_secs(1), async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_timeout_expired() {
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            tokio::time::sleep(Duration::from_secs(1)),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_runtime_helpers() {
        let start = Runtime::now();
        Runtime::sleep(Duration::from_millis(10)).await;
        assert!(start.elapsed() >= Duration::from_millis(10));

        let result = Runtime::timeout(Duration::from_secs(1), async { 42 }).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        let result = Runtime::timeout(
            Duration::from_millis(10),
            Runtime::sleep(Duration::from_secs(1)),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_spawn() {
        let handle = spawn(async { 42 });
        assert_eq!(handle.await.unwrap(), 42);
    }
}
