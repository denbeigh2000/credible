use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use age::Identity;
use nix::unistd::{Gid, Uid};
use tokio::fs::{self, OpenOptions};
use tokio::process::Command;

use crate::manager::SecretManager;
use crate::process::ProcessRunningError;
use crate::{GroupWrapper, Secret, SecretError, SecretStorage, UserWrapper};

mod error;
pub use error::{MountSecretsError, UnmountSecretsError};

#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
use darwin::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::*;

// TODO: Make this more modular, so there's not a bunch of copy/paste between
// this and SecretManager
pub struct SystemSecretConfiguration<E, I>
where
    E: SecretError,
    I: SecretStorage,
{
    pub mgr: SecretManager<E, I>,
}

impl<E, I> SystemSecretConfiguration<E, I>
where
    // TODO: This trait lifetime shit is voodoo. Can we make it so that we
    // don't have to concern ourselves with these most of the time?
    E: SecretError + 'static + Sized,
    I: SecretStorage<Error = E>,
    ProcessRunningError: From<<I as SecretStorage>::Error>,
{
    // TODO: Should we just expose these as public functions that accept a
    // SecretManager? Or a SecretStorage+Vec<Secret>?
    pub fn new(mgr: SecretManager<E, I>) -> Self {
        Self { mgr }
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
        let identities = crate::age::get_identities(&self.mgr.private_key_paths)?;
        for secret in self.mgr.secrets.iter() {
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
            .mgr
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

        let mut reader = crate::age::decrypt_bytes(reader, identities).await?;
        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_err(|e| MountSecretsError::ReadFromStoreFailure(Box::new(e)))?;
        drop(file);
        nix::unistd::chown(&exp_path, Some(owner), Some(group))
            .map_err(MountSecretsError::PermissionSettingFailure)?;

        Ok(exp_path)
    }
}
