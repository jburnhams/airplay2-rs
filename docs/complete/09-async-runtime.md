# Section 09: Async Runtime Abstraction

> **VERIFIED**: Checked against `src/net/mod.rs` and submodules on 2025-01-30.
> Implementation complete with Tokio runtime (async-std support commented out for now).
> Includes traits, tokio_impl, secure TLS support, and Runtime abstraction.

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)

## Overview

The library should be runtime-agnostic, supporting both Tokio and async-std. This section provides abstraction traits and implementations for async I/O operations.

## Objectives

- Define abstract traits for async read/write
- Provide Tokio implementation (default)
- Provide async-std implementation (feature flag)
- Abstract timer and sleep operations
- Support runtime-agnostic spawning

---

## Tasks

### 9.1 Async Traits

- [x] **9.1.1** Define async I/O traits

**File:** `src/net/traits.rs`

```rust
//! Async I/O traits for runtime abstraction

use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Async read trait
pub trait AsyncRead {
    /// Poll for read readiness
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>>;
}

/// Async write trait
pub trait AsyncWrite {
    /// Poll for write readiness
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize>>;

    /// Poll for flush completion
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>>;

    /// Poll for shutdown completion
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>>;
}

/// Extension trait for reading
pub trait AsyncReadExt: AsyncRead {
    /// Read exact number of bytes
    fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a, Self>
    where
        Self: Unpin,
    {
        ReadExact {
            reader: self,
            buf,
            pos: 0,
        }
    }

    /// Read all available bytes
    fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> Read<'a, Self>
    where
        Self: Unpin,
    {
        Read { reader: self, buf }
    }
}

impl<T: AsyncRead + ?Sized> AsyncReadExt for T {}

/// Extension trait for writing
pub trait AsyncWriteExt: AsyncWrite {
    /// Write all bytes
    fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a, Self>
    where
        Self: Unpin,
    {
        WriteAll {
            writer: self,
            buf,
            pos: 0,
        }
    }

    /// Flush the writer
    fn flush(&mut self) -> Flush<'_, Self>
    where
        Self: Unpin,
    {
        Flush { writer: self }
    }
}

impl<T: AsyncWrite + ?Sized> AsyncWriteExt for T {}

/// Future for reading exact bytes
pub struct ReadExact<'a, R: ?Sized> {
    reader: &'a mut R,
    buf: &'a mut [u8],
    pos: usize,
}

impl<R: AsyncRead + Unpin + ?Sized> Future for ReadExact<'_, R> {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        while this.pos < this.buf.len() {
            let n = ready!(Pin::new(&mut *this.reader).poll_read(cx, &mut this.buf[this.pos..]))?;
            if n == 0 {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "unexpected EOF",
                )));
            }
            this.pos += n;
        }

        Poll::Ready(Ok(()))
    }
}

/// Future for reading bytes
pub struct Read<'a, R: ?Sized> {
    reader: &'a mut R,
    buf: &'a mut [u8],
}

impl<R: AsyncRead + Unpin + ?Sized> Future for Read<'_, R> {
    type Output = Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut *this.reader).poll_read(cx, this.buf)
    }
}

/// Future for writing all bytes
pub struct WriteAll<'a, W: ?Sized> {
    writer: &'a mut W,
    buf: &'a [u8],
    pos: usize,
}

impl<W: AsyncWrite + Unpin + ?Sized> Future for WriteAll<'_, W> {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        while this.pos < this.buf.len() {
            let n = ready!(Pin::new(&mut *this.writer).poll_write(cx, &this.buf[this.pos..]))?;
            if n == 0 {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "write zero",
                )));
            }
            this.pos += n;
        }

        Poll::Ready(Ok(()))
    }
}

/// Future for flushing
pub struct Flush<'a, W: ?Sized> {
    writer: &'a mut W,
}

impl<W: AsyncWrite + Unpin + ?Sized> Future for Flush<'_, W> {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.writer).poll_flush(cx)
    }
}

/// Helper macro for polling
macro_rules! ready {
    ($e:expr) => {
        match $e {
            Poll::Ready(t) => t,
            Poll::Pending => return Poll::Pending,
        }
    };
}
use ready;
```

---

### 9.2 Tokio Implementation

- [ ] **9.2.1** Implement traits for Tokio types

**File:** `src/net/tokio_impl.rs`

```rust
//! Tokio runtime implementation

use super::traits::{AsyncRead, AsyncWrite};
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

// Re-export tokio types for convenience
pub use tokio::net::TcpStream;
pub use tokio::net::UdpSocket;
pub use tokio::time::{sleep, timeout, Duration, Instant};

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
```

---

### 9.3 Net Module Entry Point

- [ ] **9.3.1** Create net module entry point

**File:** `src/net/mod.rs`

```rust
//! Network abstraction layer
//!
//! This module provides runtime-agnostic networking primitives.

mod traits;

#[cfg(feature = "tokio-runtime")]
mod tokio_impl;

#[cfg(feature = "async-std-runtime")]
mod async_std_impl;

pub use traits::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

// Re-export the active runtime's types
#[cfg(feature = "tokio-runtime")]
pub use tokio_impl::*;

#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub use async_std_impl::*;

use std::future::Future;
use std::time::Duration;

/// Runtime abstraction for common operations
pub struct Runtime;

impl Runtime {
    /// Sleep for the specified duration
    #[cfg(feature = "tokio-runtime")]
    pub async fn sleep(duration: Duration) {
        tokio::time::sleep(duration).await
    }

    #[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
    pub async fn sleep(duration: Duration) {
        async_std::task::sleep(duration).await
    }

    /// Run a future with a timeout
    #[cfg(feature = "tokio-runtime")]
    pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, TimeoutError>
    where
        F: Future<Output = T>,
    {
        tokio::time::timeout(duration, future)
            .await
            .map_err(|_| TimeoutError)
    }

    #[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
    pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, TimeoutError>
    where
        F: Future<Output = T>,
    {
        async_std::future::timeout(duration, future)
            .await
            .map_err(|_| TimeoutError)
    }

    /// Get current timestamp
    pub fn now() -> std::time::Instant {
        std::time::Instant::now()
    }
}

/// Timeout error
#[derive(Debug, Clone, Copy)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// Boxed async read/write for type erasure
pub type BoxedAsyncRW = Box<dyn AsyncReadWrite + Send + Unpin>;

/// Combined read/write trait
pub trait AsyncReadWrite: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> AsyncReadWrite for T {}
```

---

## Unit Tests

### Test File: `src/net/traits.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
        let reader = MockReader {
            data: Cursor::new(vec![1, 2, 3, 4]),
        };
        // Test would need async runtime
    }
}
```

### Test File: `src/net/tokio_impl.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            async { 42 }
        ).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_timeout_expired() {
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            tokio::time::sleep(Duration::from_secs(1))
        ).await;
        assert!(result.is_err());
    }
}
```

---

## Acceptance Criteria

- [ ] AsyncRead/AsyncWrite traits defined
- [ ] Tokio implementation compiles and works
- [ ] Extension traits provide read_exact, write_all
- [ ] Runtime::sleep works correctly
- [ ] Runtime::timeout works correctly
- [ ] spawn() spawns tasks correctly
- [ ] All unit tests pass
- [ ] Feature flags work correctly

---

## Notes

- async-std implementation can be added later if needed
- Consider using `tokio-util` for codec support
- May want to add TLS support via `tokio-rustls`
- The traits mirror tokio's traits for easier implementation
