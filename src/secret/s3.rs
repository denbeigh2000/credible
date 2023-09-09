use std::path::Path;

use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::{ByteStream, ByteStreamError};
use aws_sdk_s3::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::secret::{SecretError, SecretStorage};
use crate::util::BoxedAsyncReader;
use crate::IntoSecretStorage;

#[derive(Deserialize, Debug)]
pub struct S3Config {
    bucket: String,
    // Required, because AWS require you to specify the correct region for your
    // bucket.
    region: String,
}

#[async_trait]
impl IntoSecretStorage for S3Config {
    type Error = S3SecretStorageError;
    type Impl = S3SecretStorage;

    async fn build(self) -> Self::Impl {
        let region = Region::new(self.region);
        let config = aws_config::from_env().region(region).load().await;
        let client = Client::new(&config);

        S3SecretStorage::new(client, self.bucket)
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

#[async_trait]
impl SecretStorage for S3SecretStorage {
    type Error = S3SecretStorageError;

    async fn read(&self, key: &Path) -> Result<BoxedAsyncReader, Self::Error> {
        let path_str = key.to_str().expect("path not representable as str");
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path_str)
            .send()
            .await?;

        Ok(BoxedAsyncReader::from_async_read(
            object.body.into_async_read(),
        ))
    }

    async fn write<R: AsyncRead + Send + Unpin>(
        &self,
        key: &Path,
        mut new_encrypted_content: R,
    ) -> Result<(), Self::Error> {
        let path_str = key.to_str().expect("path not representable as str");
        let mut buf = Vec::new();
        new_encrypted_content.read_to_end(&mut buf).await?;
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
