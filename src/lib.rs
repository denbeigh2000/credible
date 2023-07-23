use std::path::{Path, PathBuf};

use nix::unistd::{Group, User};
use serde::Deserialize;
use thiserror::Error;

mod wrappers;
pub use wrappers::{GroupWrapper, UserWrapper};

#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
use darwin::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::*;

#[derive(Deserialize, Debug, Clone)]
pub struct Secret {
    pub name: String,
    pub encrypted_path: PathBuf,
    pub mount_path: PathBuf,
    pub encryption_keys: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RuntimeKey {
    pub private_key_path: PathBuf,
    pub secret: Secret,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SecretManagerConfig {
    pub secret_root: PathBuf,
    pub owner_user: UserWrapper,
    pub owner_group: GroupWrapper,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,
}

#[derive(Debug, Clone)]
pub struct SecretManager {
    pub secret_root: PathBuf,
    pub owner_user: User,
    pub owner_group: Group,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,
}

impl From<SecretManagerConfig> for SecretManager {
    fn from(value: SecretManagerConfig) -> Self {
        Self {
            secret_root: value.secret_root,
            owner_user: value.owner_user.into(),
            owner_group: value.owner_group.into(),
            secrets: value.secrets,
            keys: value.keys,
        }
    }
}

#[derive(Error, Debug)]
pub enum MountSecretError {
    #[error("mount point already in use, unmount first")]
    AlreadyMounted,
    #[error("failed to check if mounted: {0}")]
    MountCheckFailure(#[from] CheckMountedError),
    #[error("failed to create ramfs: {0}")]
    RamfsCreationFailure(MountRamfsError),
}

impl SecretManager {
    pub fn mount_secrets(&self) -> Result<u32, MountSecretError> {
        if device_mounted(&self.secret_root)? {
            return Err(MountSecretError::AlreadyMounted);
        }

        mount_ramfs(&self.secret_root).map_err(MountSecretError::RamfsCreationFailure)?;
        // TODO: decrypt and symlink secrets
        Ok(0)
    }

    // TODO: Own error type
    pub fn unmount_secrets(&self) -> Result<(), MountSecretError> {
        Ok(())
    }
}
