use std::collections::HashMap;
use std::marker::PhantomData;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use ::age::Identity;
use age::{encrypt_bytes, EncryptionError};
use nix::unistd::{Group, User};
use serde::Deserialize;
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};

mod builder;
pub use builder::SecretManagerBuilder;
mod secret;
use secret::{run_process, S3Config};
pub use secret::{
    ExposedSecretConfig,
    ProcessRunningError,
    Secret,
    SecretBackingImpl,
    SecretError,
};

mod age;

mod wrappers;
use tokio::process::Command;
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
    pub owner_user: UserWrapper,
    pub owner_group: GroupWrapper,
    pub secrets: Vec<Secret>,
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
        private_key_paths: Vec<PathBuf>,
        backing: I,
    ) -> Self {
        Self {
            secret_root,
            owner_user,
            owner_group,
            secrets,
            private_key_paths,
            backing,

            _data1: Default::default(),
        }
    }

    pub async fn create(
        &self,
        secret: &Secret,
        source_file: Option<&Path>,
    ) -> Result<(), CreateUpdateSecretError> {
        // TODO: Check to see if this exists?
        let mut data = match source_file {
            Some(file) => File::open(file)
                .await
                .map_err(CreateUpdateSecretError::ReadSourceData)?,
            None => todo!("Secure tempdir editing"),
        };

        let (mut r, w) = tokio_pipe::pipe().map_err(CreateUpdateSecretError::ReadSourceData)?;
        encrypt_bytes(&mut data, w, &secret.encryption_keys)
            .await
            .map_err(CreateUpdateSecretError::EncryptingSecret)?;
        self.backing
            .write(&secret.path, &mut r)
            .await
            .map_err(|e| CreateUpdateSecretError::WritingToStore(Box::new(e)))?;

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
        let (mut r, w) = tokio_pipe::pipe().map_err(EditSecretError::CreatingPipe)?;
        // NOTE: It would be nice if this supported creating new files, too
        self.backing
            .read(&secret.path, w)
            .await
            .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;
        let temp_file = NamedTempFile::new().map_err(EditSecretError::CreatingTempFile)?;
        let temp_file_path = temp_file.path();
        // Scope ensures temp file is closed after we write decrypted data
        {
            let mut temp_file_handle = File::create(temp_file_path)
                .await
                .map_err(EditSecretError::OpeningTempFile)?;
            age::decrypt_bytes(&mut r, &mut temp_file_handle, &identities).await?;
        }
        let editor_result = Command::new(editor)
            .arg(temp_file_path)
            .status()
            .await
            .map_err(EditSecretError::InvokingEditor)?;

        if !editor_result.success() {
            return Err(EditSecretError::EditorBadExit(editor_result));
        }

        let (mut r, w) = tokio_pipe::pipe().map_err(EditSecretError::CreatingPipe)?;
        let mut temp_file_handle = File::open(temp_file_path)
            .await
            .map_err(EditSecretError::OpeningTempFile)?;
        age::encrypt_bytes(&mut temp_file_handle, w, &secret.encryption_keys).await?;
        self.backing
            .write(&secret.path, &mut r)
            .await
            .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;

        Ok(ExitStatus::from_raw(0))
    }

    pub async fn mount(
        &self,
        mount_point: &Path,
        secret_dir: &Path,
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
            self.write_secret_to_file(secret, &identities).await?;
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
        exposures: &[ExposedSecretConfig],
    ) -> Result<ExitStatus, ProcessRunningError> {
        let secrets_map = self.secrets.iter().fold(HashMap::new(), |mut acc, x| {
            acc.insert(x.name.clone(), x);
            acc
        });
        let full_exposures = exposures
            .iter()
            .map(|e| match secrets_map.get(&e.name) {
                Some(secret) => {
                    let secret = (*secret).clone();
                    let exposure_type = e.exposure_type.clone();
                    Ok(ExposedSecret {
                        secret,
                        exposure_type,
                    })
                }
                None => Err(ProcessRunningError::NoSuchSecret(e.name.clone())),
            })
            .collect::<Result<Vec<ExposedSecret>, ProcessRunningError>>()?;
        let identities = age::get_identities(&self.private_key_paths)?;
        let status = run_process(argv, &full_exposures, &identities, &self.backing).await?;

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

        let mut file = File::open(source_file)
            .await
            .map_err(UploadSecretError::ReadingSourceFile)?;

        let (mut r, w) = tokio_pipe::pipe().map_err(UploadSecretError::CreatingPipe)?;
        let encrypt_fut = age::encrypt_bytes(&mut file, w, &secret.encryption_keys);
        let backing_fut = self.backing.write(&secret.path, &mut r);
        // NOTE: Have to break up the call + await so the blocking write in
        // encrypt_bytes doesn't hang
        let (encrypt_result, write_result) = futures::future::join(encrypt_fut, backing_fut).await;
        encrypt_result?;
        write_result.map_err(|e| UploadSecretError::WritingToStore(Box::new(e)))?;
        drop(file);

        Ok(ExitStatus::from_raw(0))
    }

    async fn write_secret_to_file(
        &self,
        secret: &Secret,
        identities: &[Box<dyn Identity>],
    ) -> Result<PathBuf, MountSecretsError> {
        let exp_path = self.secret_root.join(&secret.name);
        let (mut r, w) = tokio_pipe::pipe().map_err(MountSecretsError::DataPipeError)?;
        self.backing
            .read(&secret.path, w)
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

        age::decrypt_bytes(&mut r, &mut file, identities).await?;
        drop(file);
        nix::unistd::chown(
            &exp_path,
            Some(self.owner_user.uid),
            Some(self.owner_group.gid),
        )
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
    EncryptingSecret(EncryptionError),
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
