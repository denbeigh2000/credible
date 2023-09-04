use thiserror::Error;

use crate::secret::FileExposureError;
#[cfg(target_os = "macos")]
use crate::system::darwin::*;
#[cfg(target_os = "linux")]
use crate::system::linux::*;

#[derive(Error, Debug)]
pub enum MountSecretsError {
    #[error("mount point already in use, unmount first")]
    AlreadyMounted,
    #[error("failed to check if mounted: {0}")]
    MountCheckFailure(#[from] CheckMountedError),
    #[error("failed to create ramfs: {0}")]
    RamfsCreationFailure(MountRamfsError),
    // NOTE: The type system makes it hard to return a Box<dyn ...Error> trait
    // other than std::error::Error
    #[error("failed to read from backing store: {0}")]
    ReadFromStoreFailure(Box<dyn std::error::Error>),
    #[error("failed to decrypt secret: {0}")]
    DecryptingSecretFailure(#[from] crate::age::DecryptionError),
    #[error("failed to set permissions on secret: errno {0}")]
    PermissionSettingFailure(nix::errno::Errno),
    #[error("failed to create file to write decrypted secret: {0}")]
    CreatingFilesFailure(std::io::Error),
    #[error("failed to write secret to file: {0}")]
    WritingToFileFailure(std::io::Error),
    #[error("failed to create data pipe: {0}")]
    DataPipeError(std::io::Error),
    #[error("failed to create symlink: {0}")]
    SymlinkCreationFailure(std::io::Error),

    #[error("no secret with name: {0}")]
    NoSuchSecret(String),
    #[error("error exposing secrets as files: {0}")]
    ExposingFilesFailure(#[from] FileExposureError),
}

#[derive(Error, Debug)]
pub enum UnmountSecretsError {
    #[error("error checking if device mounted: {0}")]
    CheckMountedError(#[from] CheckMountedError),
    #[error("error invoking umount: {0}")]
    InvokingCommand(std::io::Error),
    #[error("error removing symlink: {0}")]
    RemovingSymlink(std::io::Error),
}
