use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use ::age::Identity;
use nix::unistd::{Gid, Uid};
use serde::Deserialize;
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

mod builder;
pub use builder::SecretManagerBuilder;
mod secret;
use secret::{run_process, ExposureSpec, S3Config};
pub use secret::{
    CliExposureSpec,
    Exposures,
    ProcessRunningError,
    Secret,
    SecretError,
    SecretStorage,
};

mod process_utils;

mod age;
use crate::age::{encrypt_bytes, EncryptionError};

mod wrappers;
pub use wrappers::{GroupWrapper, UserWrapper};

pub(crate) mod util;

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
    pub secrets: Vec<Secret>,
    pub storage: StorageConfig,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum StorageConfig {
    S3(S3Config),
}

pub struct SecretManager<E, I>
where
    E: SecretError,
    I: SecretStorage,
{
    pub secrets: Vec<Secret>,
    pub private_key_paths: Vec<PathBuf>,

    pub storage: I,

    _data1: PhantomData<E>,
}

#[async_trait::async_trait]
pub trait IntoSecretStorage {
    type Error: SecretError;
    type Impl: SecretStorage<Error = Self::Error>;

    async fn build(self) -> Self::Impl;
}

impl<E, I> SecretManager<E, I>
where
    E: SecretError + 'static + Sized,
    I: SecretStorage<Error = E>,
    ProcessRunningError: From<<I as SecretStorage>::Error>,
{
    pub fn new(secrets: Vec<Secret>, private_key_paths: Vec<PathBuf>, storage: I) -> Self {
        Self {
            secrets,
            private_key_paths,
            storage,

            _data1: Default::default(),
        }
    }

    pub async fn create(
        &self,
        secret: &Secret,
        source_file: Option<&Path>,
    ) -> Result<(), CreateUpdateSecretError> {
        // TODO: Check to see if this exists?
        let data = match source_file {
            Some(file) => File::open(file)
                .await
                .map_err(CreateUpdateSecretError::ReadSourceData)?,
            None => todo!("Secure tempdir editing"),
        };

        let (reader, fut) = encrypt_bytes(data, &secret.encryption_keys)
            .await
            .map_err(CreateUpdateSecretError::EncryptingSecret)?;
        self.storage
            .write(&secret.path, reader)
            .await
            .map_err(|e| CreateUpdateSecretError::WritingToStore(Box::new(e)))?;

        fut.await
            .map_err(|e| CreateUpdateSecretError::EncryptingSecret(EncryptionError::SpawningThread(e)))??;

        Ok(())
    }

    pub async fn edit(
        &self,
        secret_name: &str,
        editor: &str,
    ) -> Result<ExitStatus, EditSecretError> {
        let secret = self
            .secrets
            .iter()
            .find(|s| s.name == secret_name)
            .ok_or_else(|| EditSecretError::NoSuchSecret(secret_name.to_string()))?;
        let identities = age::get_identities(&self.private_key_paths)?;
        // NOTE: It would be nice if this supported creating new files, too
        let reader = self
            .storage
            .read(&secret.path)
            .await
            .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;
        let temp_file = NamedTempFile::new().map_err(EditSecretError::CreatingTempFile)?;
        let temp_file_path = temp_file.path();
        // Scope ensures temp file is closed after we write decrypted data
        {
            let mut temp_file_handle = File::create(temp_file_path)
                .await
                .map_err(EditSecretError::OpeningTempFile)?;
            let mut reader = age::decrypt_bytes(reader, &identities).await?;
            tokio::io::copy(&mut reader, &mut temp_file_handle)
                .await
                .map_err(EditSecretError::OpeningTempFile)?;
        }
        let editor_result = Command::new(editor)
            .arg(temp_file_path)
            .status()
            .await
            .map_err(EditSecretError::InvokingEditor)?;

        if !editor_result.success() {
            return Err(EditSecretError::EditorBadExit(editor_result));
        }

        let temp_file_handle = File::open(temp_file_path)
            .await
            .map_err(EditSecretError::OpeningTempFile)?;
        let (reader, fut) = age::encrypt_bytes(temp_file_handle, &secret.encryption_keys).await?;
        self.storage
            .write(&secret.path, reader)
            .await
            .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;

        fut.await
            .map_err(|e| EditSecretError::EncryptingSecret(EncryptionError::SpawningThread(e)))?
            .map_err(EditSecretError::EncryptingSecret)?;

        Ok(ExitStatus::from_raw(0))
    }

    pub async fn mount(
        &self,
        mount_point: &Path,
        secret_dir: &Path,
        owner: &Option<UserWrapper>,
        group: &Option<GroupWrapper>,
    ) -> Result<ExitStatus, MountSecretsError> {
        if device_mounted(mount_point)? {
            return Err(MountSecretsError::AlreadyMounted);
        }

        if !mount_point.exists() {
            let _ = fs::create_dir(mount_point)
                .await
                .map_err(MountSecretsError::CreatingFilesFailure);
        }

        mount_persistent_ramfs(mount_point).map_err(MountSecretsError::RamfsCreationFailure)?;
        let identities = age::get_identities(&self.private_key_paths)?;
        for secret in self.secrets.iter() {
            let secret_owner = secret
                .owner_user
                .as_ref()
                .or(owner.as_ref())
                .map(|u| u.as_ref().uid)
                .unwrap_or_else(Uid::current);

            let secret_group = secret
                .owner_group
                .as_ref()
                .or(group.as_ref())
                .map(|g| g.as_ref().gid)
                .unwrap_or_else(Gid::current);

            self.write_secret_to_file(mount_point, secret, &identities, secret_owner, secret_group)
                .await?;

            if let Some(p) = secret.mount_path.as_ref() {
                let dest_path = mount_point.join(&secret.name);
                tokio::fs::symlink(&dest_path, p)
                    .await
                    .map_err(MountSecretsError::SymlinkCreationFailure)?;
            }
        }
        tokio::fs::symlink(mount_point, secret_dir)
            .await
            .map_err(MountSecretsError::SymlinkCreationFailure)?;

        Ok(ExitStatus::from_raw(0))
    }

    pub async fn unmount(
        &self,
        mount_point: &Path,
        secret_dir: &Path,
    ) -> Result<ExitStatus, UnmountSecretsError> {
        if !device_mounted(mount_point)? {
            return Ok(ExitStatus::from_raw(0));
        }

        Command::new("umount")
            .arg(mount_point)
            .status()
            .await
            .map_err(UnmountSecretsError::InvokingCommand)?;
        tokio::fs::remove_file(secret_dir)
            .await
            .map_err(UnmountSecretsError::RemovingSymlink)?;

        Ok(ExitStatus::from_raw(0))
    }

    pub async fn run_command(
        &self,
        argv: &[String],
        exposure_flags: Vec<CliExposureSpec>,
        config_files: &[PathBuf],
    ) -> Result<ExitStatus, ProcessRunningError> {
        let secrets_map = self.secrets.iter().fold(HashMap::new(), |mut acc, x| {
            acc.insert(x.name.clone(), x);
            acc
        });
        let mut exposures = Exposures::default();
        for path in config_files {
            let mut f = File::open(&path)
                .await
                .map_err(ProcessRunningError::ReadingMountConfigFiles)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .await
                .map_err(ProcessRunningError::ReadingMountConfigFiles)?;
            let data: HashMap<String, HashSet<ExposureSpec>> = serde_yaml::from_slice(&buf)
                .map_err(ProcessRunningError::DecodingMountConfigFiles)?;
            exposures.add_config(data);
        }

        let mut cli_exposure_map: HashMap<String, HashSet<ExposureSpec>> = HashMap::new();
        for exposure in exposure_flags {
            let (name, exp) = exposure.into();
            match cli_exposure_map.get_mut(&name) {
                Some(v) => v.insert(exp),
                None => cli_exposure_map
                    .insert(name, HashSet::from([exp]))
                    .is_some(),
            };
        }
        exposures.add_config(cli_exposure_map);

        let identities = age::get_identities(&self.private_key_paths)?;
        let status =
            run_process(argv, &secrets_map, &exposures, &identities, &self.storage).await?;

        Ok(status)
    }

    pub async fn upload(
        &self,
        secret_name: &str,
        source_file: &Path,
    ) -> Result<ExitStatus, UploadSecretError> {
        let secret = self
            .secrets
            .iter()
            .find(|s| s.name == secret_name)
            .ok_or_else(|| UploadSecretError::NoSuchSecret(secret_name.to_string()))?;

        let file = File::open(source_file)
            .await
            .map_err(UploadSecretError::ReadingSourceFile)?;

        let (reader, handle) = age::encrypt_bytes(file, &secret.encryption_keys).await?;
        self.storage
            .write(&secret.path, reader)
            .await
            .map_err(|e| UploadSecretError::WritingToStore(Box::new(e)))?;

        handle.await
            .map_err(|e| UploadSecretError::EncryptingData(EncryptionError::SpawningThread(e)))?
            .map_err(UploadSecretError::EncryptingData)?;

        Ok(ExitStatus::from_raw(0))
    }

    async fn write_secret_to_file(
        &self,
        root: &Path,
        secret: &Secret,
        identities: &[Box<dyn Identity>],
        owner: nix::unistd::Uid,
        group: nix::unistd::Gid,
    ) -> Result<PathBuf, MountSecretsError> {
        let exp_path = root.join(&secret.name);
        let reader = self
            .storage
            .read(&secret.path)
            .await
            .map_err(|e| MountSecretsError::ReadFromStoreFailure(Box::new(e)))?;
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .mode(0o0600)
            .open(&exp_path)
            .await
            .map_err(MountSecretsError::CreatingFilesFailure)?;

        let mut reader = age::decrypt_bytes(reader, identities).await?;
        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_err(|e| MountSecretsError::ReadFromStoreFailure(Box::new(e)))?;
        drop(file);
        nix::unistd::chown(&exp_path, Some(owner), Some(group))
            .map_err(MountSecretsError::PermissionSettingFailure)?;

        Ok(exp_path)
    }
}

#[derive(Error, Debug)]
pub enum MountSecretsError {
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
    #[error("failed to write secret to file: {0}")]
    WritingToFileFailure(std::io::Error),
    #[error("failed to create data pipe: {0}")]
    DataPipeError(std::io::Error),
    #[error("failed to create symlink: {0}")]
    SymlinkCreationFailure(std::io::Error),
}

#[derive(Error, Debug)]
pub enum UnmountSecretsError {
    #[error("error checking if device mounted: {0}")]
    CheckMountedError(#[from] CheckMountedError),
    #[error("error invoking umount: {0}")]
    InvokingCommand(std::io::Error),
    #[error("error removing symlink: {0}")]
    RemovingSymlink(std::io::Error),
}

#[derive(Error, Debug)]
pub enum CreateUpdateSecretError {
    #[error("error reading source data: {0}")]
    ReadSourceData(std::io::Error),
    #[error("failed to write to backing store: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
    #[error("error encrypting secret: {0}")]
    EncryptingSecret(#[from] EncryptionError),
}

#[derive(Error, Debug)]
pub enum UploadSecretError {
    #[error("no configured secret with name {0}")]
    NoSuchSecret(String),
    #[error("error creating pipe: {0}")]
    CreatingPipe(std::io::Error),
    #[error("error reading source file: {0}")]
    ReadingSourceFile(std::io::Error),
    #[error("error encrypting secret: {0}")]
    EncryptingData(#[from] age::EncryptionError),
    #[error("error writing encrpyted data to store: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
}

#[derive(Error, Debug)]
pub enum EditSecretError {
    #[error("no secret named {0}")]
    NoSuchSecret(String),
    #[error("error creating tempfile: {0}")]
    CreatingTempFile(std::io::Error),
    #[error("error opening tempfile: {0}")]
    OpeningTempFile(std::io::Error),
    #[error("error creating pipe: {0}")]
    CreatingPipe(std::io::Error),
    #[error("error fetching existing secret from store: {0}")]
    FetchingFromStore(Box<dyn std::error::Error>),
    #[error("error decrypting existing secret: {0}")]
    DecryptingSecret(#[from] age::DecryptionError),
    #[error("error encrypting updated secret: {0}")]
    EncryptingSecret(#[from] age::EncryptionError),
    #[error("error uploading updated secret: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
    #[error("error invoking editor: {0}")]
    InvokingEditor(std::io::Error),
    #[error("editor exited with non-success status: {0}")]
    EditorBadExit(ExitStatus),
}
