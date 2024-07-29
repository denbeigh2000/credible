use std::future::Future;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::Stream;
use serde::Deserialize;
use tokio::io::AsyncRead;

use crate::util::BoxedAsyncReader;
use crate::wrappers::{GroupWrapper, UserWrapper};

mod process;
pub use process::*;

mod file;
pub use file::*;

mod s3;
pub use s3::*;

mod exposures;
pub use exposures::*;

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

pub trait SecretStorage {
    type Error: SecretError;

    fn read(&self, p: &Path) -> impl Future<Output = Result<BoxedAsyncReader, Self::Error>> + Send;
    fn read_stream(
        &self,
        p: &Path,
    ) -> impl Future<
        Output = Result<
            impl Stream<Item = Result<Bytes, Self::Error>> + Send + Unpin + 'static,
            Self::Error,
        >,
    > + Send;
    fn write<R: AsyncRead + Send + Unpin>(
        &self,
        p: &Path,
        new_encrypted_content: R,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub trait SecretError: std::error::Error {}
