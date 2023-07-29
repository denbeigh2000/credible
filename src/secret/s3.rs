use std::path::Path;

use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::{ByteStream, ByteStreamError};
use aws_sdk_s3::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

use crate::secret::{SecretBackingImpl, SecretError};
use crate::IntoSecretBackingImpl;

#[derive(Deserialize, Debug)]
pub struct S3Config {
    bucket: String,
}

#[async_trait]
impl IntoSecretBackingImpl for S3Config {
    type Error = S3SecretBackingError;
    type Impl = S3SecretBacking;

    async fn build(self) -> Self::Impl {
        let config = aws_config::load_from_env().await;
        let client = Client::new(&config);

        S3SecretBacking::new(client, self.bucket)
    }
}

#[derive(Error, Debug)]
pub enum S3SecretBackingError {
    #[error("error getting object from s3: {0}")]
    GettingObject(#[from] SdkError<GetObjectError>),
    #[error("error writing object to s3: {0}")]
    UpdatingObject(#[from] SdkError<PutObjectError>),
    #[error("error reading data from s3: {0}")]
    ReadingData(#[from] ByteStreamError),
    #[error("error copying data: {0}")]
    CopyingData(#[from] std::io::Error)
}

impl SecretError for S3SecretBackingError {}

#[derive(Clone)]
pub struct S3SecretBacking {
    client: Client,
    bucket: String,
}

impl S3SecretBacking {
    pub fn new(client: Client, bucket: String) -> Self {
        Self { client, bucket }
    }
}

#[async_trait]
impl SecretBackingImpl for S3SecretBacking {
    type Error = S3SecretBackingError;

    async fn read<W: AsyncWrite + Send + Unpin>(&self, key: &Path, writer: &mut W) -> Result<(), Self::Error> {
        let path_str = key.to_str().expect("path not representable as str");
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path_str)
            .send()
            .await?;


        let mut reader = object.body.into_async_read();
        tokio::io::copy(&mut reader, writer).await?;

        Ok(())
    }

    async fn write<R: AsyncRead + Send + Unpin>(&self, key: &Path, new_encrypted_content: &mut R) -> Result<(), Self::Error> {
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
