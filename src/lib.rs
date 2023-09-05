use std::path::PathBuf;

use serde::Deserialize;

pub mod system;
pub use system::{MountSecretsError, UnmountSecretsError};
mod secret;
use secret::S3Config;
pub use secret::{CliExposureSpec, Exposures, Secret, SecretError, SecretStorage};

mod process_utils;

mod age;

mod process;
pub use process::ProcessRunningError;

pub mod cli;

mod wrappers;
pub use wrappers::{GroupWrapper, UserWrapper};

pub(crate) mod util;

#[derive(Deserialize, Debug)]
pub struct RuntimeKey {
    pub private_key_path: PathBuf,
    pub secret: Secret,
}

#[derive(Deserialize, Debug)]
pub struct SecretManagerConfig {
    pub secrets: Vec<Secret>,
    pub storage: StorageConfig,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum StorageConfig {
    S3(S3Config),
}

#[async_trait::async_trait]
pub trait IntoSecretStorage {
    type Error: SecretError;
    type Impl: SecretStorage<Error = Self::Error>;

    async fn build(self) -> Self::Impl;
}
