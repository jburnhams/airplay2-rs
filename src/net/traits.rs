//! Async I/O traits for runtime abstraction

use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Helper macro for polling
macro_rules! ready {
    ($e:expr) => {
        match $e {
            Poll::Ready(t) => t,
            Poll::Pending => return Poll::Pending,
        }
    };
}

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
