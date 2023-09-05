use std::collections::HashMap;
use std::path::Path;

use age::Identity;
use nix::sys::time::TimeValLike;
use nix::time::{clock_gettime, ClockId};
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
    base_mount_point: &Path,
    secret_dir: &Path,
    secrets: &HashMap<String, Secret>,
    exposures: &HashMap<String, Vec<FileExposeArgs>>,
    identities: &[Box<dyn Identity>],
    storage: &S,
) -> Result<(), MountSecretsError>
where
    <S as SecretStorage>::Error: 'static,
{
    // Get time since boot in ms
    let time_ms = clock_gettime(ClockId::CLOCK_MONOTONIC)
        .expect("failed to get time of day")
        .num_milliseconds()
        .to_string();
    let mount_point = base_mount_point.join(time_ms);

    // NOTE: Because we mount a tmpfs, and use the ms since boot in our
    // generation directory, it is highly unlikely that we will run into a
    // collision here. If we do, there's likely some kind of crafted
    // timing attack going on, and we shouldn't write any secrets here.
    // If the directory exists, but isn't mounted, then we'll write to our
    // tmpfs without writing to whatever is currently backing this
    // directory anyway.
    if device_mounted(&mount_point)? {
        return Err(MountSecretsError::AlreadyMounted);
    }

    if !mount_point.exists() {
        let _ = fs::create_dir(&mount_point)
            .await
            .map_err(MountSecretsError::CreatingFilesFailure);
    }

    mount_persistent_ramfs(&mount_point).map_err(MountSecretsError::RamfsCreationFailure)?;
    let file_pairs =
        map_secrets(secrets, exposures.iter()).map_err(MountSecretsError::NoSuchSecret)?;

    expose_files(&mount_point, storage, &file_pairs, identities).await?;

    if secret_dir.exists() {
        tokio::fs::remove_file(secret_dir)
            .await
            .map_err(MountSecretsError::SymlinkCreationFailure)?;
    }
    tokio::fs::symlink(&mount_point, secret_dir)
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
