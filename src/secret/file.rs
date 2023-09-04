use std::collections::HashSet;

use age::Identity;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::age::{decrypt_bytes, DecryptionError};
use crate::secret::exposures::*;
use crate::secret::{Secret, SecretStorage, *};

const FILE_PERMISSIONS: u32 = 0o0400;

pub async fn expose_files<S>(
    secret_dir: &Path,
    storage: &S,
    exposures: &[(&&Secret, &HashSet<FileExposeArgs>)],
    identities: &[Box<dyn Identity>],
) -> Result<(), FileExposureError>
where
    S: SecretStorage,
    <S as SecretStorage>::Error: 'static,
{
    let mut buf = vec![];
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
            let dest_path = secret_dir.join(&secret.name);

            let mut file = OpenOptions::new()
                .mode(FILE_PERMISSIONS)
                .create(true)
                .truncate(true)
                .write(true)
                .open(&dest_path)
                .await
                .map_err(FileExposureError::CreatingTempFile)?;

            file.write_all(&buf)
                .await
                .map_err(FileExposureError::WritingToFile)?;

            tokio::fs::symlink(&dest_path, &file_spec.path)
                .await
                .map_err(FileExposureError::CreatingSymlink)?;

            buf.truncate(0);
        }
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
}

#[derive(thiserror::Error, Debug)]
#[error("not able to clean up symlink at {0}: {1}")]
pub struct FileCleanupError(PathBuf, std::io::Error);
