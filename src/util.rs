use std::pin::Pin;

use tokio::io::AsyncRead;

pub struct BoxedAsyncReader {
    inner: Box<dyn AsyncRead + Unpin + Send + 'static>,
}

impl BoxedAsyncReader {
    pub fn from_async_read<R: AsyncRead + Unpin + Send + 'static>(reader: R) -> Self {
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
