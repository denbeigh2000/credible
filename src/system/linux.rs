use std::io;
use std::path::Path;
use std::process::Command;

use block_utils::{get_mount_device, BlockUtilsError};
use thiserror::Error;

use crate::process_utils::process_msg;

#[derive(Error, Debug)]
#[error("failed to check if device mounted: {0}")]
pub struct CheckMountedError(#[from] BlockUtilsError);

pub fn device_mounted(dir: &Path) -> Result<bool, CheckMountedError> {
    let present = get_mount_device(dir).map(|d| d.is_some())?;

    Ok(present)
}

// #[derive(Error, Debug)]
// pub struct MountRamfsError(#[from] io::Error);

pub enum MountRamfsError {
    #[error("unable to run mount: {0}")]
    InvokingProcess(#[from] io::Error),
    #[error("unable to mount ramfs: {0}")]
    MountingRamfs(String),
}

pub fn mount_persistent_ramfs(dir: &Path) -> Result<(), MountRamfsError> {
    // NOTE: Not using nix here because it's non-obvious how to pass the
    // default mode to MsFlags
    let cmd = Command::new("mount")
        .arg("-t")
        .arg("ramfs")
        .arg("none")
        .arg(dir)
        .arg("-o")
        .arg("nodev,nosuid,mode=0751")
        .output()?;

    if !cmd.status.success() {
        let msg = process_msg("mount", cmd.stderr);
        return Err(MountRamfsError::CreatingRamfs(msg));
    }

    Ok(())
}
