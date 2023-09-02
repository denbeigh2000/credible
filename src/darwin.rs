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
    #[error("failed to call subprocess: {0}")]
    CallingSubprocess(io::Error),
    #[error("failed to create ramfs: {0}")]
    CreatingRamfs(String),
    #[error("failed to format ramfs: {0}")]
    CreatingFilesystem(String),
    #[error("failed to mount ramfs: {0}")]
    MountingRamfs(String),
    #[error("did not find a device name from hdiutil")]
    NoDeviceFromHdiutil,
}

fn process_msg(raw: Vec<u8>) -> String {
    String::from_utf8(raw).unwrap_or_else(|e| {
        eprintln!("hdiutil returned non-utf8 stderr ({e})");
        "<Unknown>".to_string()
    })
}

pub fn mount_persistent_ramfs(dir: &Path) -> Result<(), MountRamfsError> {
    // 512MB for secrets should be enough for everybody...right?
    let ram_device_name = format!("ram://{}", 2048 * 512);
    // TODO: I don't think this handles non-zero error codes?
    let device_mounted_proc = Command::new("hdiutil")
        .arg("attach")
        .arg("-nomount")
        .arg(&ram_device_name)
        .output()
        .map_err(MountRamfsError::CallingSubprocess)?;

    if !device_mounted_proc.status.success() {
        let msg = process_msg(device_mounted_proc.stderr);
        return Err(MountRamfsError::CreatingRamfs(msg));
    }

    let device_string = String::from_utf8(device_mounted_proc.stdout)
        .expect("invalid utf-8 bytes from hdiutil")
        .split_whitespace()
        .next()
        .ok_or(MountRamfsError::NoDeviceFromHdiutil)?
        .to_owned();

    let mount_device_proc = Command::new("newfs_hfs")
        .arg("-v")
        .arg("credible")
        .arg(&device_string)
        .output()
        .map_err(MountRamfsError::CallingSubprocess)?;
    if !mount_device_proc.status.success() {
        let msg = process_msg(mount_device_proc.stderr);
        return Err(MountRamfsError::CreatingFilesystem(msg));
    }

    let mount_proc = Command::new("mount")
        .arg("-t")
        .arg("hfs")
        .arg("-o")
        .arg("nobrowse,nodev,nosuid,-m=0751")
        .arg(&device_string)
        .arg(dir)
        .output()
        .map_err(MountRamfsError::CallingSubprocess)?;
    if !mount_proc.status.success() {
        let msg = process_msg(mount_proc.stderr);
        return Err(MountRamfsError::MountingRamfs(msg));
    }

    Ok(())
}
