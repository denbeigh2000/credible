use std::io;
use std::path::Path;
use std::process::Command;

use thiserror::Error;

#[derive(Error, Debug)]
#[error("failed to check if device mounted: {0}")]
pub struct CheckMountedError(#[from] io::Error);

// Adapted from agenix, may want to revisit/investigate alternatives?
pub fn device_mounted(dir: &Path) -> Result<bool, CheckMountedError> {
    Command::new("diskutil")
        .arg("info")
        .arg(dir)
        .status()
        .map(|result| result.code() == Some(0))
        .map_err(CheckMountedError)
}

#[derive(Error, Debug)]
pub enum MountRamfsError {
    #[error("failed to create ramfs")]
    CreatingRamfs(io::Error),
    #[error("failed to format ramfs")]
    CreatingFilesystem(io::Error),
    #[error("failed to mount ramfs")]
    MountingRamfs(io::Error),
    #[error("did not find a device name from hdiutil")]
    NoDeviceFromHdiutil,
}

pub fn mount_ramfs(dir: &Path) -> Result<(), MountRamfsError> {
    // 512MB for secrets should be enough for everybody...right?
    let ram_device_name = format!("ram://{}", 2048 * 512);
    // TODO: I don't think this handles non-zero error codes?
    let device_bytes = Command::new("hdiutil")
        .arg("attach")
        .arg("-nomount")
        .arg(&ram_device_name)
        .output()
        .map_err(MountRamfsError::CreatingRamfs)?
        .stdout;

    let device_string = String::from_utf8(device_bytes)
        .expect("invalid utf-8 bytes from hdiutil")
        .split_whitespace()
        .next()
        .ok_or(MountRamfsError::NoDeviceFromHdiutil)?
        .to_owned();

    Command::new("newfs_hfs")
        .arg("-v")
        .arg("age-stor")
        .arg(&device_string)
        .status()
        .map_err(MountRamfsError::CreatingFilesystem)?;
    Command::new("mount")
        .arg("-t")
        .arg("hfs")
        .arg("-o")
        .arg("nobrowse,nodev,nosuid,-m=0751")
        .arg(&device_string)
        .arg(dir)
        .status()
        .map_err(MountRamfsError::MountingRamfs)?;

    Ok(())
}
