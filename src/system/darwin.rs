use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;
use tokio::process::Command;

use crate::process_utils::process_msg;

#[derive(Error, Debug)]
#[error("failed to check if device mounted: {0}")]
pub struct CheckMountedError(#[from] io::Error);

// Adapted from agenix, may want to revisit/investigate alternatives?
pub async fn device_mounted(dir: &Path) -> Result<bool, CheckMountedError> {
    Command::new("diskutil")
        .arg("info")
        .arg(dir)
        .output()
        .await
        .map(|result| result.status.code() == Some(0))
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

#[derive(Error, Debug)]
pub enum UnmountRamfsError {
    #[error("unable to get disk info: {0}")]
    InvokingDiskutil(io::Error),
    #[error("unable to run umount: {0}")]
    InvokingUmount(io::Error),
    #[error("unable to unmount ramfs: {0}")]
    UnmountingRamfs(String),
    #[error("unable to delete ramfs: {0}")]
    InvokingDeletion(io::Error),
    #[error("deleting ramfs failed: {0}")]
    DeletingRamfs(String),
}

pub async fn mount_persistent_ramfs(dir: &Path) -> Result<(), MountRamfsError> {
    // 512MB for secrets should be enough for everybody...right?
    let ram_device_name = format!("ram://{}", 2048 * 512);
    // TODO: I don't think this handles non-zero error codes?
    let device_mounted_proc = Command::new("hdiutil")
        .arg("attach")
        .arg("-nomount")
        .arg(&ram_device_name)
        .output()
        .await
        .map_err(MountRamfsError::CallingSubprocess)?;

    if !device_mounted_proc.status.success() {
        let msg = process_msg("hdiutil", device_mounted_proc.stderr);
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
        .await
        .map_err(MountRamfsError::CallingSubprocess)?;
    if !mount_device_proc.status.success() {
        let msg = process_msg("newfs_hfs", mount_device_proc.stderr);
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
        .await
        .map_err(MountRamfsError::CallingSubprocess)?;
    if !mount_proc.status.success() {
        let msg = process_msg("mount", mount_proc.stderr);
        return Err(MountRamfsError::MountingRamfs(msg));
    }
    Ok(())
}

pub async fn unmount_persistent_ramfs(p: &Path) -> Result<(), UnmountRamfsError> {
    let info_proc = Command::new("diskutil")
        .arg("info")
        .arg("-plist")
        .arg(p)
        .output()
        .await
        .map_err(UnmountRamfsError::InvokingDiskutil)?;

    if !info_proc.status.success() {
        // Not mounted
        return Ok(());
    }

    // TODO: unwrawps
    let data: plist::Value = plist::from_bytes(&info_proc.stdout).unwrap();
    let dict = data.as_dictionary().unwrap();
    let disk_path = dict.get("DeviceNode").unwrap().as_string().unwrap();

    // Unmount the tmpfs from disk
    let result = Command::new("umount")
        .arg(p)
        .output()
        .await
        .map_err(UnmountRamfsError::InvokingUmount)?;

    if !result.status.success() {
        let msg = process_msg("umount", result.stderr);
        return Err(UnmountRamfsError::UnmountingRamfs(msg));
    }

    let disk_path = PathBuf::from_str(disk_path).unwrap();
    if !disk_path.exists() {
        return Ok(());
    }

    // `mount` did not detach our underlying ramfs, manually detach it
    let result = Command::new("hdiutil")
        .arg("detach")
        .arg(&disk_path)
        .output()
        .await
        .map_err(UnmountRamfsError::InvokingDeletion)?;

    if !result.status.success() {
        let msg = process_msg("diskutil", result.stderr);
        return Err(UnmountRamfsError::UnmountingRamfs(msg));
    }

    Ok(())
}
