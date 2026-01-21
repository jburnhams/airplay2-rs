//! Network abstraction layer
//!
//! This module provides runtime-agnostic networking primitives.

mod traits;

#[cfg(feature = "tokio-runtime")]
mod tokio_impl;

// async-std support temporarily disabled/removed
// #[cfg(feature = "async-std-runtime")]
// mod async_std_impl;

#[cfg(test)]
mod tests;

pub use traits::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Flush, Read, ReadExact, WriteAll,
};

// Re-export the active runtime's types
#[cfg(feature = "tokio-runtime")]
pub use tokio_impl::*;

// #[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
// pub use async_std_impl::*;

use std::future::Future;

/// Runtime abstraction for common operations
pub struct Runtime;

impl Runtime {
    /// Sleep for the specified duration
    #[cfg(feature = "tokio-runtime")]
    pub async fn sleep(duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }

    /*
    #[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
    pub async fn sleep(duration: std::time::Duration) {
        async_std::task::sleep(duration).await
    }
    */

    /// Run a future with a timeout
    ///
    /// # Errors
    ///
    /// Returns `TimeoutError` if the future does not complete within the specified duration.
    #[cfg(feature = "tokio-runtime")]
    pub async fn timeout<F, T>(duration: std::time::Duration, future: F) -> Result<T, TimeoutError>
    where
        F: Future<Output = T>,
    {
        tokio::time::timeout(duration, future)
            .await
            .map_err(|_| TimeoutError)
    }

    /*
    #[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
    pub async fn timeout<F, T>(duration: std::time::Duration, future: F) -> Result<T, TimeoutError>
    where
        F: Future<Output = T>,
    {
        async_std::future::timeout(duration, future)
            .await
            .map_err(|_| TimeoutError)
    }
    */

    /// Get current timestamp
    #[must_use]
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
