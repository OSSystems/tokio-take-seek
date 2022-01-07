// Copyright (C) 2021 O.S. Systems Sofware LTDA
//
// SPDX-License-Identifier: Apache-2.0

#[macro_use]
mod macros;
mod take;

pub use crate::take::SignedTake;
use std::{
    io::{Result, SeekFrom},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncSeek, ReadBuf};

pub trait AsyncTakeSeekExt: AsyncRead + AsyncSeek + Unpin + Sized {
    fn take_with_seek(self, limit: u64) -> TakeSeek<Self> {
        TakeSeek::new(take::take(self, limit))
    }
}

impl<R: AsyncRead + AsyncSeek + Unpin + Sized> AsyncTakeSeekExt for R {}

pub struct TakeSeek<R> {
    inner: SignedTake<R>,
    start_pos: Option<StartPosition>,
}

enum StartPosition {
    NotReady(SeekFrom),
    Ready(u64),
}

impl<R: AsyncSeek + AsyncRead + Unpin> TakeSeek<R> {
    fn new(inner: SignedTake<R>) -> Self {
        let start_pos = None;
        TakeSeek { inner, start_pos }
    }

    pub fn get_pin_mut<'a>(self: &'a mut Pin<&mut Self>) -> Pin<&'a mut SignedTake<R>> {
        Pin::new(&mut self.inner)
    }

    pub fn get_reader_pin_mut<'a>(self: &'a mut Pin<&mut Self>) -> Pin<&'a mut R> {
        Pin::new(self.inner.get_mut())
    }
}

impl<R: AsyncSeek + AsyncRead + Unpin> AsyncSeek for TakeSeek<R> {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> Result<()> {
        self.start_pos.replace(StartPosition::NotReady(position));
        self.get_reader_pin_mut().start_seek(SeekFrom::Current(0))
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<u64>> {
        match self.start_pos {
            Some(StartPosition::NotReady(real_seek)) => {
                let start_pos = ready!(self.get_reader_pin_mut().poll_complete(cx))?;
                self.start_pos.replace(StartPosition::Ready(start_pos));
                self.get_reader_pin_mut().start_seek(real_seek.clone())?;
                self.get_reader_pin_mut().poll_complete(cx)
            }

            Some(StartPosition::Ready(start_pos)) => {
                let new_pos = ready!(self.get_reader_pin_mut().poll_complete(cx))?;
                let limit = self.inner.limit();
                self.inner.set_limit(limit - (new_pos as i64 - start_pos as i64));
                self.start_pos.take();
                Poll::Ready(Ok(new_pos))
            }

            None => self.get_reader_pin_mut().poll_complete(cx),
        }
    }
}

impl<R: AsyncSeek + AsyncRead + Unpin> AsyncRead for TakeSeek<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        self.get_pin_mut().poll_read(cx, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        fs,
        io::{AsyncReadExt, AsyncSeekExt},
    };

    #[tokio::test]
    async fn basic_seek_and_reed() {
        let file = tempfile::NamedTempFile::new().unwrap();
        //                                        21v               38v
        fs::write(file.path(), "this will be skipped|this will be read|this will be guarded")
            .await
            .unwrap();

        let mut handle = fs::File::open(file.path()).await.unwrap().take_with_seek(38);
        handle.seek(SeekFrom::Start(21)).await.unwrap();

        let mut data = String::default();
        handle.read_to_string(&mut data).await.unwrap();
        assert_eq!(data, "this will be read");
    }

    #[tokio::test]
    async fn multiple_re_reads() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            //                 21v                             53v
            "this will be skipped|this will be read multiple times|this will be guarded",
        )
        .await
        .unwrap();

        let mut handle = fs::File::open(file.path()).await.unwrap().take_with_seek(53);

        let mut data;
        for _ in 1..4 {
            handle.seek(SeekFrom::Start(21)).await.unwrap();
            data = String::default();
            handle.read_to_string(&mut data).await.unwrap();
            assert_eq!(data, "this will be read multiple times");
        }
    }

    #[tokio::test]
    async fn seek_past_limit() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            //             17v                 37v
            "this can be read|this will be guarded",
        )
        .await
        .unwrap();

        let mut handle = fs::File::open(file.path()).await.unwrap().take_with_seek(17);
        handle.seek(SeekFrom::Start(18)).await.unwrap();
        let mut data = [0; 37];
        let res = handle.read(&mut data).await.unwrap();

        assert_eq!(res, 0);
    }

    #[tokio::test]
    async fn seek_past_limit_and_back() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            //             16v                 37v
            "this can be read|this will be guarded",
        )
        .await
        .unwrap();

        let mut handle = fs::File::open(file.path()).await.unwrap().take_with_seek(16);
        handle.seek(SeekFrom::Start(20)).await.unwrap();
        handle.seek(SeekFrom::Start(0)).await.unwrap();

        let mut data = String::default();
        handle.read_to_string(&mut data).await.unwrap();
        assert_eq!(data, "this can be read");
    }
}
