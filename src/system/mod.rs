use std::collections::HashMap;
use std::path::Path;

use age::Identity;
use nix::sys::time::TimeValLike;
use nix::time::{clock_gettime, ClockId};
use tokio::fs;

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
    let mount_point = base_mount_point.join(&time_ms);

    // NOTE: Because we mount a tmpfs, and use the ms since boot in our
    // generation directory, it is highly unlikely that we will run into a
    // collision here. If we do, there's likely some kind of crafted
    // timing attack going on, and we shouldn't write any secrets here.
    // If the directory exists, but isn't mounted, then we'll write to our
    // tmpfs without writing to whatever is currently backing this
    // directory anyway.
    if device_mounted(&mount_point).await? {
        return Err(MountSecretsError::AlreadyMounted);
    }

    if !mount_point.exists() {
        let _ = fs::create_dir_all(&mount_point)
            .await
            .map_err(MountSecretsError::CreatingFilesFailure);
    }

    mount_persistent_ramfs(&mount_point)
        .await
        .map_err(MountSecretsError::RamfsCreationFailure)?;
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

    // Remove any old symlinks
    unmount(base_mount_point, None, Some(&time_ms)).await?;

    Ok(())
}

pub async fn unmount(
    base_mount_point: &Path,
    unlink_dir: Option<&Path>,
    skip: Option<&str>,
) -> Result<(), UnmountSecretsError> {
    let mut dir_entries = fs::read_dir(base_mount_point)
        .await
        .map_err(UnmountSecretsError::ListingOldSymlinks)?;

    while let Some(entry) = dir_entries
        .next_entry()
        .await
        .map_err(UnmountSecretsError::ListingOldSymlinks)?
    {
        let file_name = entry.file_name();
        let dir_name = file_name.to_str().expect("path is not UTF-8 compatible");
        if Some(dir_name) != skip {
            let p = entry.path();
            if device_mounted(&p).await? {
                unmount_persistent_ramfs(&p).await?
            }

            // TODO: better error
            fs::remove_dir(&p)
                .await
                .map_err(UnmountSecretsError::DeletingOldDir)?;
        }
    }

    if let Some(p) = unlink_dir {
        if p.is_symlink() {
            tokio::fs::remove_file(p)
                .await
                .map_err(UnmountSecretsError::RemovingSymlink)?;
        }
    }

    Ok(())
}
