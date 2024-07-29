use std::collections::HashMap;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::Poll;

use futures::Stream;
use tokio::io::AsyncRead;

use crate::secret::{EnvExposeArgs, FileExposeArgs};
use crate::{ExposureSpec, Secret};

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

/// Maps (name, exposure_set) pairs into (Secret, exposure_set) pairs.
/// Required because we can't use generics in a closure, and ideally I want to
/// avoid copy-pasting this block
pub fn map_secrets<'a, A, I>(
    secrets: &'a HashMap<String, Secret>,
    items: I,
) -> Result<Vec<(&'a Secret, &'a Vec<A>)>, String>
where
    I: Iterator<Item = (&'a String, &'a Vec<A>)>,
    A: 'static,
{
    items
        .map(|(name, item)| {
            secrets
                .get(name.as_str())
                .map(|secret| (secret, item))
                .ok_or_else(|| name.into())
        })
        .collect::<Result<Vec<_>, _>>()
}

pub fn partition_specs<I: IntoIterator<Item = ExposureSpec>>(
    items: I,
) -> (Vec<FileExposeArgs>, Vec<EnvExposeArgs>) {
    items
        .into_iter()
        .fold((vec![], vec![]), |(mut fs, mut es), item| {
            match item {
                ExposureSpec::Env(s) => es.push(s),
                ExposureSpec::File(s) => fs.push(*s),
            };

            (fs, es)
        })
}

pub struct OneshotStream<E: Unpin>(Option<bytes::Bytes>, PhantomData<E>);

impl<E: Unpin> OneshotStream<E> {
    pub fn new(b: bytes::Bytes) -> Self {
        Self(Some(b), PhantomData::default())
    }

    pub fn new_from_slice(b: &[u8]) -> Self {
        Self::new(bytes::Bytes::copy_from_slice(b))
    }
}

impl<E: Unpin> Stream for OneshotStream<E> {
    type Item = Result<bytes::Bytes, E>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let o = Ok(self.get_mut().0.take());
        Poll::Ready(o.transpose())
    }
}
