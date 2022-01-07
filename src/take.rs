// Based on tokio's tokio/tokio/src/io/util/take.rs
// commit f8c91f2ead24ed6e268a820405c329032dd30da6

use pin_project_lite::pin_project;
use std::{
    cmp, io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

pin_project! {
    /// A copy of the `tokio::io::Take` that uses a signed interger for inner limit
    #[derive(Debug)]
    #[must_use = "streams do nothing unless you `.await` or poll them"]
    #[cfg_attr(docsrs, doc(cfg(feature = "io-util")))]
    pub struct SignedTake<R> {
        #[pin]
        inner: R,
        // Add '_' to avoid conflicts with `limit` method.
        limit_: i64,
    }
}

pub(super) fn take<R: AsyncRead>(inner: R, limit: u64) -> SignedTake<R> {
    SignedTake { inner, limit_: limit as i64 }
}

impl<R: AsyncRead> SignedTake<R> {
    pub fn limit(&self) -> i64 {
        self.limit_
    }

    pub fn set_limit(&mut self, limit: i64) {
        self.limit_ = limit
    }

    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: AsyncRead> AsyncRead for SignedTake<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), io::Error>> {
        if self.limit_ <= 0 {
            return Poll::Ready(Ok(()));
        }

        let me = self.project();
        let mut b = buf.take(*me.limit_ as usize);
        ready!(me.inner.poll_read(cx, &mut b))?;
        let n = b.filled().len();

        // We need to update the original ReadBuf
        unsafe {
            buf.assume_init(n);
        }
        buf.advance(n);
        *me.limit_ -= n as i64;
        Poll::Ready(Ok(()))
    }
}

impl<R: AsyncBufRead> AsyncBufRead for SignedTake<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let me = self.project();

        // Don't call into inner reader at all at EOF because it may still block
        if *me.limit_ <= 0 {
            return Poll::Ready(Ok(&[]));
        }

        let buf = ready!(me.inner.poll_fill_buf(cx)?);
        let cap = cmp::min(buf.len() as i64, *me.limit_) as usize;
        Poll::Ready(Ok(&buf[..cap]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        // Don't let callers reset the limit by passing an overlarge value
        let amt = cmp::min(amt as i64, *me.limit_) as usize;
        *me.limit_ -= amt as i64;
        me.inner.consume(amt);
    }
}
