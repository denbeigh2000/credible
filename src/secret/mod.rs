use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::wrappers::{GroupWrapper, UserWrapper};

mod s3;
pub use s3::*;

mod process;
pub use process::*;

#[derive(Deserialize, Debug, Clone)]
pub struct Secret {
    pub name: String,
    #[serde(alias = "encryptionKeys")]
    pub encryption_keys: Vec<String>,

    // TODO: Will this be fine for all providers?
    pub path: PathBuf,
    #[serde(alias = "mountPath")]
    pub mount_path: Option<PathBuf>,

    #[serde(alias = "ownerUser")]
    pub owner_user: Option<UserWrapper>,
    #[serde(alias = "ownerGroup")]
    pub owner_group: Option<GroupWrapper>,
}

#[async_trait]
pub trait SecretStorage {
    type Error: SecretError;

    async fn read<W: AsyncWrite + Send + Unpin>(&self, p: &Path, writer: W) -> Result<(), Self::Error>;
    async fn write<R: AsyncRead + Send + Unpin>(&self, p: &Path, new_encrypted_content: R) -> Result<(), Self::Error>;
}

pub trait SecretError: std::error::Error {}
