use std::marker::PhantomData;
use std::path::PathBuf;

use nix::unistd::{Group, User};
use serde::Deserialize;
use thiserror::Error;
use tokio::fs;

mod secret;
use secret::{S3Config, S3SecretBacking};
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
        I: IntoSecretBackingImpl<'a>,
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
    E: SecretError,
    I: SecretBackingImpl<'a, Error = E>,
{
    pub async fn mount_secrets(&self) -> Result<u32, MountSecretError<E>> {
        if device_mounted(&self.secret_root)? {
            return Err(MountSecretError::AlreadyMounted);
        }

        if !self.secret_root.exists() {
            tokio::fs::create_dir(self.secret_root).await;
        }
        // Set owners/permissions (every time)

        mount_ramfs(&self.secret_root).map_err(MountSecretError::RamfsCreationFailure)?;
        for secret in self.secrets.iter() {
            let exp_path = self.secret_root.join(secret.name);
            let encrypted_bytes = self.backing.read(&secret.path).await?;
            // let file = fs::write(exp_path,
            // create file, set permission
            // decrypt secret
            // write to file
            // set permission
        }
        Ok(0)
    }

    // TODO: Own error type
    pub fn unmount_secrets(&self) -> Result<(), MountSecretError<E>> {
        Ok(())
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
pub enum MountSecretError<E: SecretError> {
    #[error("mount point already in use, unmount first")]
    AlreadyMounted,
    #[error("failed to check if mounted: {0}")]
    MountCheckFailure(#[from] CheckMountedError),
    #[error("failed to create ramfs: {0}")]
    RamfsCreationFailure(MountRamfsError),
    #[error("failed to read from backing store: {0}")]
    ReadFromStoreFailure(#[from] E),
}
