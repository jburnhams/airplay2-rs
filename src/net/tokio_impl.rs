//! Tokio runtime implementation

use super::traits::{AsyncRead, AsyncWrite};
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

// Re-export tokio types for convenience
pub use tokio::net::TcpStream;
pub use tokio::net::UdpSocket;
pub use tokio::time::{sleep, timeout, Instant};

// We don't re-export Duration here to avoid shadowing std::time::Duration if both are imported
// users can use std::time::Duration

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        let mut read_buf = tokio::io::ReadBuf::new(buf);
        match tokio::io::AsyncRead::poll_read(self, cx, &mut read_buf) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize>> {
        tokio::io::AsyncWrite::poll_write(self, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        tokio::io::AsyncWrite::poll_flush(self, cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(self, cx)
    }
}

/// TCP connection helper
pub async fn connect_tcp(addr: &str) -> Result<TcpStream> {
    TcpStream::connect(addr).await
}

/// UDP socket helper
pub async fn bind_udp(addr: &str) -> Result<UdpSocket> {
    UdpSocket::bind(addr).await
}

/// Spawn a task
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future)
}

/// Spawn a blocking task
pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(f)
}
