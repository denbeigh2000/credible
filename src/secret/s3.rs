use std::path::PathBuf;

use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::{ByteStream, ByteStreamError};
use aws_sdk_s3::Client;
use serde::Deserialize;
use thiserror::Error;

use crate::IntoSecretBackingImpl;
use crate::secret::SecretBackingImpl;

#[derive(Deserialize, Debug)]
pub struct S3Config {
    bucket: String,
}

#[async_trait]
impl<'a> IntoSecretBackingImpl<'a> for S3Config {
    type Params = S3ObjectParams;
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
}

#[derive(Deserialize, Debug)]
pub struct S3ObjectParams {
    key: PathBuf,
}

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
impl<'a> SecretBackingImpl<'a> for S3SecretBacking {
    type Params = S3ObjectParams;
    type Error = S3SecretBackingError;

    async fn read(&self, params: &Self::Params) -> Result<Vec<u8>, Self::Error> {
        let path_str = params.key.to_str().expect("path not representable as str");
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path_str)
            .send()
            .await?;

        let data = object.body.collect().await?.into_bytes();
        Ok(data.into())
    }

    async fn write(
        &self,
        params: &Self::Params,
        new_encrypted_content: Vec<u8>,
    ) -> Result<(), Self::Error> {
        let path_str = params.key.to_str().expect("path not representable as str");
        let body = ByteStream::from(new_encrypted_content);
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
