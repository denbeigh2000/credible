use std::collections::HashMap;
use std::{marker::PhantomData};
use std::process::ExitStatus;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;

use ::age::Identity;
use nix::unistd::{Group, User};
use serde::Deserialize;
use thiserror::Error;
use tokio::fs::{self, OpenOptions};

mod builder;
pub use builder::SecretManagerBuilder;
mod secret;
use secret::{S3Config, ProcessRunningError, run_process};
pub use secret::{ExposedSecretConfig, Secret, SecretBackingImpl, SecretError};

mod age;

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

use crate::secret::ExposedSecret;

#[derive(Deserialize, Debug)]
pub struct RuntimeKey {
    pub private_key_path: PathBuf,
    pub secret: Secret,
}

#[derive(Deserialize, Debug)]
pub struct SecretManagerConfig {
    pub secret_root: PathBuf,
    pub owner_user: UserWrapper,
    pub owner_group: GroupWrapper,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,
    pub private_key_paths: Vec<PathBuf>,

    pub backing_config: BackingConfig,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum BackingConfig {
    S3(S3Config),
}

pub struct SecretManager<E, I>
where
    E: SecretError,
    I: SecretBackingImpl,
{
    pub secret_root: PathBuf,
    pub owner_user: User,
    pub owner_group: Group,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,
    pub private_key_paths: Vec<PathBuf>,

    pub backing: I,

    _data1: PhantomData<E>,
}

#[async_trait::async_trait]
pub trait IntoSecretBackingImpl {
    type Error: SecretError;
    type Impl: SecretBackingImpl<Error = Self::Error>;

    async fn build(self) -> Self::Impl;
}

impl<E, I> SecretManager<E, I>
where
    E: SecretError + 'static + Sized,
    I: SecretBackingImpl<Error = E>,
    ProcessRunningError: From<<I as SecretBackingImpl>::Error>,
{
    pub fn new(
        secret_root: PathBuf,
        owner_user: User,
        owner_group: Group,
        secrets: Vec<Secret>,
        keys: Vec<RuntimeKey>,
        private_key_paths: Vec<PathBuf>,
        backing: I,
    ) -> Self {
        Self {
            secret_root,
            owner_user,
            owner_group,
            secrets,
            keys,
            private_key_paths,
            backing,

            _data1: Default::default(),
        }
    }

    pub async fn mount_secrets(&self) -> Result<ExitStatus, MountSecretError> {
        if device_mounted(&self.secret_root)? {
            return Err(MountSecretError::AlreadyMounted);
        }

        if !self.secret_root.exists() {
            let _ = fs::create_dir(&self.secret_root)
                .await
                .map_err(MountSecretError::CreatingFilesFailure);
        }

        mount_persistent_ramfs(&self.secret_root)
            .map_err(MountSecretError::RamfsCreationFailure)?;
        let identities = age::get_identities(&self.private_key_paths)?;
        for secret in self.secrets.iter() {
            self.write_secret_to_file(secret, &identities).await?;
        }

        Ok(ExitStatus::from_raw(0))
    }

    pub async fn run_command(&self, argv: &[String], exposures: &[ExposedSecretConfig]) -> Result<ExitStatus, ProcessRunningError> {
        let secrets_map = self.secrets.iter().fold(HashMap::new(), |mut acc, x| {
            acc.insert(x.name.clone(), x);
            acc
        });
        let full_exposures = exposures.iter().map(|e| match secrets_map.get(&e.name) {
            Some(secret) => {
                let secret = (*secret).clone();
                let exposure_type = e.exposure_type.clone();
                Ok(ExposedSecret { secret, exposure_type })
            },
            None => Err(ProcessRunningError::NoSuchSecret(e.name.clone())),
        }).collect::<Result<Vec<ExposedSecret>, ProcessRunningError>>()?;
        let identities = age::get_identities(&self.private_key_paths)?;
        run_process(argv, &full_exposures, &identities, &self.backing).await?;
        unimplemented!()
    }


    async fn write_secret_to_file(
        &self,
        secret: &Secret,
        identities: &[Box<dyn Identity>],
    ) -> Result<PathBuf, MountSecretError> {
        let exp_path = self.secret_root.join(&secret.name);
        let encrypted_bytes = self
            .backing
            .read(&secret.path)
            .await
            .map_err(|e| MountSecretError::ReadFromStoreFailure(Box::new(e)))?;
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .mode(0o0600)
            .open(&exp_path)
            .await
            .map_err(MountSecretError::CreatingFilesFailure)?;

        age::decrypt_bytes(&*encrypted_bytes, &mut file, identities).await?;
        drop(file);
        nix::unistd::chown(
            &exp_path,
            Some(self.owner_user.uid),
            Some(self.owner_group.gid),
        )
        .map_err(MountSecretError::PermissionSettingFailure)?;

        Ok(exp_path)
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
    // NOTE: The type system makes it hard to return a Box<dyn ...Error> trait
    // other than std::error::Error
    #[error("failed to read from backing store: {0}")]
    ReadFromStoreFailure(Box<dyn std::error::Error>),
    #[error("failed to decrypt secret: {0}")]
    DecryptingSecretFailure(#[from] age::DecryptionError),
    #[error("failed to set permissions on secret: errno {0}")]
    PermissionSettingFailure(nix::errno::Errno),
    #[error("failed to create file to write decrypted secret: {0}")]
    CreatingFilesFailure(std::io::Error),
}
