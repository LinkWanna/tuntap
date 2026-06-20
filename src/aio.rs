//! Integration of TUN/TAP into tokio.
//!
//! See the [`Async`](struct.Async.html) structure.
//!
//! # Examples
//!
//! ```rust,no_run
//! # use tun_tap::*;
//! # use tun_tap::aio::Async;
//! # use tokio::io::AsyncReadExt;
//! # #[tokio::main]
//! # async fn main() {
//! let iface = Iface::new("mytun%d", Mode::Tun).unwrap();
//! let mut async_iface = Async::new(iface).unwrap();
//! let mut buf = vec![0u8; 1504];
//! let n = async_iface.read(&mut buf).await.unwrap();
//! # }
//! ```

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::Iface;

/// A wrapper around [`Iface`](../struct.Iface.html) for use with tokio.
///
/// This turns the synchronous `Iface` into an asynchronous reader/writer.
/// Implements [`AsyncRead`] and [`AsyncWrite`], so it can be used with
/// standard tokio utilities like `AsyncReadExt::read` or `copy_bidirectional`.
///
/// Equivalent to the old `Stream + Sink` API — just use `read` and `write`
/// directly via [`AsyncReadExt`](tokio::io::AsyncReadExt) /
/// [`AsyncWriteExt`](tokio::io::AsyncWriteExt).
pub struct Async {
    inner: AsyncFd<Iface>,
}

impl Async {
    /// Consumes an `Iface` and wraps it in a new `Async`.
    ///
    /// Sets the underlying fd to non-blocking mode automatically.
    ///
    /// # Errors
    ///
    /// This fails with an error in case of low-level OS errors.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tun_tap::*;
    /// # use tun_tap::aio::Async;
    /// # use tokio::io::AsyncReadExt;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let iface = Iface::new("mytun%d", Mode::Tun).unwrap();
    /// let name = iface.name().to_owned();
    /// // Bring the interface up by `ip addr add IP dev $name; ip link set up dev $name`
    /// let mut async_iface = Async::new(iface).unwrap();
    /// let mut buf = vec![0u8; 1504];
    /// let n = async_iface.read(&mut buf).await.unwrap();
    /// # }
    /// ```
    pub fn new(iface: Iface) -> io::Result<Self> {
        iface.set_non_blocking()?;
        Ok(Async {
            inner: AsyncFd::new(iface)?,
        })
    }

    /// Returns a shared reference to the underlying [`Iface`].
    pub fn get_ref(&self) -> &Iface {
        self.inner.get_ref()
    }

    /// Returns a mutable reference to the underlying [`Iface`].
    pub fn get_mut(&mut self) -> &mut Iface {
        self.inner.get_mut()
    }

    /// Consumes this `Async` and returns the underlying [`Iface`].
    pub fn into_inner(self) -> Iface {
        self.inner.into_inner()
    }
}

impl AsyncRead for Async {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = std::task::ready!(self.inner.poll_read_ready(cx))?;
            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|inner| inner.get_ref().recv(unfilled)) {
                Ok(Ok(n)) => {
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(e)) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Ok(Err(e)) => return Poll::Ready(Err(e)),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncWrite for Async {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let mut guard = std::task::ready!(self.inner.poll_write_ready(cx))?;
            match guard.try_io(|inner| inner.get_ref().send(buf)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
