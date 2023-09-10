use age::Identity;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::age::{decrypt_bytes, DecryptionError};
use crate::secret::exposures::*;
use crate::secret::{Secret, SecretStorage, *};

const FILE_PERMISSIONS: u32 = 0o0400;

// TODO:
// - metadata file (what points here, time set, etc)
// - state locking
pub async fn expose_files<S>(
    secret_dir: &Path,
    storage: &S,
    exposures: &[(&Secret, &Vec<FileExposeArgs>)],
    identities: &[Box<dyn Identity>],
) -> Result<(), FileExposureError>
where
    S: SecretStorage,
    <S as SecretStorage>::Error: 'static,
{
    let mut buf = vec![];
    log::debug!("mounting {} exposures", exposures.len());
    for (secret, exposure_set) in exposures {
        let reader = storage
            .read(&secret.path)
            .await
            .map_err(|e| FileExposureError::FetchingSecret(Box::new(e)))?;

        let mut reader = decrypt_bytes(reader, identities).await?;
        reader
            .read_to_end(&mut buf)
            .await
            .map_err(|e| FileExposureError::FetchingSecret(Box::new(e)))?;

        for file_spec in exposure_set.iter() {
            let owner = file_spec.owner.as_ref().map(|o| o.as_ref().uid);
            let group = file_spec.group.as_ref().map(|g| g.as_ref().gid);
            let mode = file_spec.mode.unwrap_or(FILE_PERMISSIONS);

            let dest_path = secret_dir.join(&secret.name);
            {
                let mut file = OpenOptions::new()
                    .mode(mode)
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&dest_path)
                    .await
                    .map_err(FileExposureError::CreatingTempFile)?;

                file.write_all(&buf)
                    .await
                    .map_err(FileExposureError::WritingToFile)?;

                log::debug!(
                    "wrote {} to {} with permissions {:#o}",
                    secret.name,
                    dest_path.as_path().to_string_lossy(),
                    mode,
                );
            }

            nix::unistd::chown(dest_path.as_path(), owner, group)
                .map_err(FileExposureError::SettingPermissions)?;

            if let Some(p) = &file_spec.vanity_path {
                if p.is_symlink() {
                    log::debug!("removing {}", p.to_string_lossy());
                    tokio::fs::remove_file(p)
                        .await
                        .map_err(FileExposureError::CreatingSymlink)?;
                }
                tokio::fs::symlink(&dest_path, p)
                    .await
                    .map_err(FileExposureError::CreatingSymlink)?;

                log::debug!(
                    "symlinked {} to {}",
                    p.to_string_lossy(),
                    dest_path.to_string_lossy()
                );
            }
        }

        buf.truncate(0);
    }

    Ok(())
}

pub async fn clean_files<'a, I>(paths: I) -> Vec<FileCleanupError>
where
    I: Iterator<Item = &'a Path>,
{
    let mut errs = vec![];

    for p in paths {
        if let Err(e) = tokio::fs::remove_file(p)
            .await
            .map_err(|e| FileCleanupError(p.to_owned(), e))
        {
            errs.push(e);
        };
    }

    errs
}

#[derive(thiserror::Error, Debug)]
pub enum FileExposureError {
    #[error("error fetching secret: {0}")]
    FetchingSecret(Box<dyn std::error::Error + 'static>),
    #[error("error decrypting secrets: {0}")]
    DecryptingSecret(#[from] DecryptionError),
    #[error("error creating temp file: {0}")]
    CreatingTempFile(std::io::Error),
    #[error("error writing secret to file: {0}")]
    WritingToFile(std::io::Error),
    #[error("error creating symlink to decrypted secret: {0}")]
    CreatingSymlink(std::io::Error),
    #[error("error setting permissions on created file: {0}")]
    SettingPermissions(nix::errno::Errno),
}

#[derive(thiserror::Error, Debug)]
#[error("not able to clean up symlink at {0}: {1}")]
pub struct FileCleanupError(PathBuf, std::io::Error);
