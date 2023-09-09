use std::io;
use std::path::Path;

use block_utils::{get_mount_device, BlockUtilsError};
use thiserror::Error;
use tokio::process::Command;

use crate::process_utils::process_msg;

#[derive(Error, Debug)]
#[error("failed to check if device mounted: {0}")]
pub struct CheckMountedError(#[from] BlockUtilsError);

pub async fn device_mounted(dir: &Path) -> Result<bool, CheckMountedError> {
    let present = get_mount_device(dir).map(|d| d.is_some())?;

    Ok(present)
}

#[derive(Error, Debug)]
pub enum MountRamfsError {
    #[error("unable to run mount: {0}")]
    InvokingProcess(#[from] io::Error),
    #[error("unable to mount ramfs: {0}")]
    MountingRamfs(String),
}

#[derive(Error, Debug)]
pub enum UnmountRamfsError {
    #[error("unable to run umount: {0}")]
    InvokingProcess(#[from] io::Error),
    #[error("unable to unmount ramfs: {0}")]
    UnmountingRamfs(String),
}

pub async fn mount_persistent_ramfs(dir: &Path) -> Result<(), MountRamfsError> {
    // NOTE: Not using nix here because it's non-obvious how to pass the
    // default mode to MsFlags
    let cmd = Command::new("mount")
        .arg("-t")
        .arg("ramfs")
        .arg("none")
        .arg(dir)
        .arg("-o")
        .arg("nodev,nosuid,mode=0751")
        .output()
        .await?;

    if !cmd.status.success() {
        let msg = process_msg("mount", cmd.stderr);
        return Err(MountRamfsError::MountingRamfs(msg));
    }

    Ok(())
}

pub async fn unmount_persistent_ramfs(p: &Path) -> Result<(), UnmountRamfsError> {
    let result = Command::new("umount")
        .arg(p)
        .output()
        .await
        .map_err(UnmountRamfsError::InvokingProcess)?;

    if !result.status.success() {
        let msg = process_msg("umount", result.stderr);
        return Err(UnmountRamfsError::UnmountingRamfs(msg));
    }

    Ok(())
}
