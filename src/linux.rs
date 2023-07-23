use std::io;
use std::path::Path;
use std::process::Command;

use block_utils::{get_mount_device, BlockUtilsError};
use thiserror::Error;

#[derive(Error, Debug)]
#[error("failed to check if device mounted: {0}")]
pub struct CheckMountedError(#[from] BlockUtilsError);

pub fn device_mounted(dir: &Path) -> Result<bool, CheckMountedError> {
    let present = get_mount_device(dir).map(|d| d.is_some())?;

    Ok(present)
}

#[derive(Error, Debug)]
#[error("unable to mount ramfs: {0}")]
pub struct MountRamfsError(#[from] io::Error);

pub fn mount_persistent_ramfs(dir: &Path) -> Result<(), MountRamfsError> {
    // NOTE: Not using nix here because it's non-obvious how to pass the
    // default mode to MsFlags
    Command::new("mount")
        .arg("-t")
        .arg("ramfs")
        .arg("none")
        .arg(dir)
        .arg("-o")
        .arg("nodev,nosuid,mode=0751")
        .status()?;

    Ok(())
}
