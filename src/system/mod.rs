use std::collections::HashMap;
use std::path::Path;

use age::Identity;
use tokio::fs;
use tokio::process::Command;

use crate::secret::{expose_files, FileExposeArgs};
use crate::util::map_secrets;
use crate::{Secret, SecretStorage};

mod error;
pub use error::{MountSecretsError, UnmountSecretsError};

#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

pub async fn mount<S: SecretStorage>(
    mount_point: &Path,
    secret_dir: &Path,
    secrets: &HashMap<String, Secret>,
    exposures: &HashMap<String, Vec<FileExposeArgs>>,
    identities: &[Box<dyn Identity>],
    storage: &S,
) -> Result<(), MountSecretsError>
where
    <S as SecretStorage>::Error: 'static,
{
    // TODO: Need to bring in directory work from other branch
    if device_mounted(secret_dir)? {
        return Err(MountSecretsError::AlreadyMounted);
    }

    if !secret_dir.exists() {
        let _ = fs::create_dir(secret_dir)
            .await
            .map_err(MountSecretsError::CreatingFilesFailure);
    }

    mount_persistent_ramfs(secret_dir).map_err(MountSecretsError::RamfsCreationFailure)?;
    let file_pairs =
        map_secrets(secrets, exposures.iter()).map_err(MountSecretsError::NoSuchSecret)?;

    expose_files(secret_dir, storage, &file_pairs, identities).await?;

    tokio::fs::symlink(mount_point, secret_dir)
        .await
        .map_err(MountSecretsError::SymlinkCreationFailure)?;

    Ok(())
}

pub async fn unmount(mount_point: &Path, secret_dir: &Path) -> Result<(), UnmountSecretsError> {
    if !device_mounted(mount_point)? {
        return Ok(());
    }

    Command::new("umount")
        .arg(mount_point)
        .status()
        .await
        .map_err(UnmountSecretsError::InvokingCommand)?;
    tokio::fs::remove_file(secret_dir)
        .await
        .map_err(UnmountSecretsError::RemovingSymlink)?;

    Ok(())
}
