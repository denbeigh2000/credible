use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite};

pub struct BoxedAsyncReader {
    inner: Box<dyn AsyncRead + Unpin + 'static>,
}

impl BoxedAsyncReader {
    pub fn from_async_read<R: AsyncRead + Unpin + 'static>(reader: R) -> Self {
        Self {
            inner: Box::new(reader),
        }
    }
}

impl AsyncRead for BoxedAsyncReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut inner = &mut self.inner;
        Pin::new(&mut inner).poll_read(cx, buf)
    }
}

pub struct BoxedAsyncWriter {
    inner: Box<dyn AsyncWrite + Unpin + 'static>,
}

impl BoxedAsyncWriter {
    pub fn from_async_write<W: AsyncWrite + Unpin + 'static>(writer: W) -> Self {
        Self {
            inner: Box::new(writer),
        }
    }
}

impl AsyncWrite for BoxedAsyncWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let mut inner = &mut self.inner;
        Pin::new(&mut inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut inner = &mut self.inner;
        Pin::new(&mut inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut inner = &mut self.inner;
        Pin::new(&mut inner).poll_shutdown(cx)
    }
}
