use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use ::aws_smithy_types::byte_stream::ByteStream;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::ByteStreamError;
use aws_sdk_s3::Client;
use futures::{AsyncReadExt, Stream, TryStreamExt};
use serde::Deserialize;
use thiserror::Error;

use crate::secret::{SecretError, SecretStorage};
use crate::IntoSecretStorage;

#[derive(Deserialize, Debug)]
pub struct S3Config {
    bucket: String,
    // Required, because AWS require you to specify the correct region for your
    // bucket.
    region: String,
}

impl IntoSecretStorage for S3Config {
    type Error = S3SecretStorageError;
    type Impl = S3SecretStorage;

    async fn build(self) -> Result<Self::Impl, Self::Error> {
        let region = Region::new(self.region);
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(region)
            .load()
            .await;
        let client = Client::new(&config);

        Ok(S3SecretStorage::new(client, self.bucket))
    }
}

#[derive(Error, Debug)]
pub enum S3SecretStorageError {
    #[error("error getting object from s3: {0}")]
    GettingObject(#[from] SdkError<GetObjectError>),
    #[error("error writing object to s3: {0}")]
    UpdatingObject(#[from] SdkError<PutObjectError>),
    #[error("error reading data from s3: {0}")]
    ReadingData(#[from] ByteStreamError),
    #[error("error copying data: {0}")]
    CopyingData(#[from] std::io::Error),
}

impl SecretError for S3SecretStorageError {}

#[derive(Clone)]
pub struct S3SecretStorage {
    client: Client,
    bucket: String,
}

impl S3SecretStorage {
    pub fn new(client: Client, bucket: String) -> Self {
        Self { client, bucket }
    }
}

struct S3GetObjectWrapper(Pin<Box<ByteStream>>);

impl Stream for S3GetObjectWrapper {
    type Item = Result<bytes::Bytes, S3SecretStorageError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // let mut p = tokio::pin!(self);
        // let p = tokio::pin!(self);
        // let p = tokio::pin!(self);
        // self.0.0.poll_next(cx)
        // (*(self.0)).poll_next(cx)
        // (self.as_mut).0.poll_next(cx)
        // let mut p = *self.0;

        self.0
            .as_mut()
            .poll_next(cx)
            .map(|opt| opt.map(|item| item.map_err(S3SecretStorageError::ReadingData)))
    }
}

impl SecretStorage for S3SecretStorage {
    // TODO: We need to have better formatting/more specific error types for
    // what goes wrong, because the Display impl on the s3 crate's error types
    // does not produce much user-actionable information
    type Error = S3SecretStorageError;

    async fn read_stream(
        &self,
        p: &Path,
    ) -> Result<
        impl Stream<Item = Result<bytes::Bytes, Self::Error>> + Send + Unpin + 'static,
        Self::Error,
    > {
        let path_str = p.to_str().expect("path not representable as str");
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path_str)
            .send()
            .await?;

        let s: ByteStream = object.body;

        Ok(S3GetObjectWrapper(Box::pin(s)))
    }

    async fn write<R>(&self, key: &Path, new_encrypted_content: R) -> Result<(), Self::Error>
    where
        R: Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + Sync + Unpin + 'static,
    {
        let path_str = key.to_str().expect("path not representable as str");
        let mut buf = Vec::new();
        new_encrypted_content
            .into_async_read()
            .read_to_end(&mut buf)
            .await?;
        let body = ByteStream::from(buf);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(path_str)
            .body(body)
            .send()
            .await?;

        Ok(())
    }
}
