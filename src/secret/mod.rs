use std::fmt::Display;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use thiserror::Error;

mod s3;
pub use s3::*;

#[derive(Deserialize, Debug)]
pub struct Secret {
    pub name: String,
    pub encrypted_path: PathBuf,
    pub mount_path: PathBuf,
    pub encryption_keys: Vec<String>,

    // TODO: Will this be fine for all providers?
    pub path: PathBuf,
}

#[derive(Error, Debug)]
pub enum ReadError {
    #[error("error reading from S3: {0}")]
    S3(S3SecretBackingError),
}

#[async_trait]
pub trait SecretBackingImpl<'a> {
    type Error: Display;

    async fn read(&self, p: &Path) -> Result<Vec<u8>, Self::Error>;
    async fn write(&self, p: &Path, new_encrypted_content: Vec<u8>) -> Result<(), Self::Error>;
}
