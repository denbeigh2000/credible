use std::marker::PhantomData;
use std::path::PathBuf;

use ::age::Identity;
use nix::unistd::{Group, User};
use serde::Deserialize;
use thiserror::Error;
use tokio::fs::{self, OpenOptions};

mod secret;
use secret::S3Config;
pub use secret::{Secret, SecretBackingImpl, SecretError};

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

    pub backing_config: BackingConfig,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum BackingConfig {
    S3(S3Config),
}

pub struct SecretManager<'a, E, I>
where
    E: SecretError,
    I: SecretBackingImpl<'a>,
{
    pub secret_root: PathBuf,
    pub owner_user: User,
    pub owner_group: Group,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,

    pub backing: I,

    _data1: PhantomData<&'a ()>,
    _data2: PhantomData<E>,
}

#[async_trait::async_trait]
pub trait IntoSecretBackingImpl<'a> {
    type Error: SecretError;
    type Impl: SecretBackingImpl<'a, Error = Self::Error>;

    async fn build(self) -> Self::Impl;
}

#[derive(Default)]
pub struct SecretManagerBuilder {
    secret_root: Option<PathBuf>,
    owner_user: Option<User>,
    owner_group: Option<Group>,
    secrets: Option<Vec<Secret>>,
    keys: Option<Vec<RuntimeKey>>,
}

impl SecretManagerBuilder {
    pub fn set_secret_root(self, secret_root: PathBuf) -> Self {
        Self {
            secret_root: Some(secret_root),
            ..self
        }
    }

    pub fn set_owner_user(self, user: User) -> Self {
        Self {
            owner_user: Some(user),
            ..self
        }
    }

    pub fn set_owner_group(self, group: Group) -> Self {
        Self {
            owner_group: Some(group),
            ..self
        }
    }

    pub fn set_secrets(self, secrets: Vec<Secret>) -> Self {
        Self {
            secrets: Some(secrets),
            ..self
        }
    }

    pub fn set_keys(self, keys: Vec<RuntimeKey>) -> Self {
        Self {
            keys: Some(keys),
            ..self
        }
    }

    pub async fn build<'a, I>(
        self,
        imp: I,
    ) -> SecretManager<
        'a,
        <I as IntoSecretBackingImpl<'a>>::Error,
        <I as IntoSecretBackingImpl<'a>>::Impl,
    >
    where
        I: IntoSecretBackingImpl<'a> + 'static,
        <I as IntoSecretBackingImpl<'a>>::Error: 'static,
        <I as IntoSecretBackingImpl<'a>>::Impl: 'static,
    {
        let backing = imp.build().await;
        SecretManager::new(
            // TODO: Where is our descryption key???
            self.secret_root.unwrap(),
            self.owner_user.unwrap(),
            self.owner_group.unwrap(),
            self.secrets.unwrap(),
            self.keys.unwrap(),
            backing,
        )
    }
}

impl<'a, E, I> SecretManager<'a, E, I>
where
    E: SecretError + 'static + Sized,
    I: SecretBackingImpl<'a, Error = E>,
{
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

    pub async fn mount_secrets(&self) -> Result<u32, MountSecretError> {
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
        // TODO: need to pass identity paths in from config
        let identities = age::get_identities(&[""])?;
        for secret in self.secrets.iter() {
            self.write_secret_to_file(secret, &identities).await?;
        }
        Ok(0)
    }

    pub fn new(
        secret_root: PathBuf,
        owner_user: User,
        owner_group: Group,
        secrets: Vec<Secret>,
        keys: Vec<RuntimeKey>,
        backing: I,
    ) -> Self {
        Self {
            secret_root,
            owner_user,
            owner_group,
            secrets,
            keys,
            backing,

            _data1: Default::default(),
            _data2: Default::default(),
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
