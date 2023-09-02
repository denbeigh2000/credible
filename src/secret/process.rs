use age::Identity;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::S3SecretStorageError;
use crate::age::{decrypt_bytes, DecryptionError};
use crate::{Exposures, Secret, SecretStorage};

#[derive(thiserror::Error, Debug)]
pub enum ProcessRunningError {
    #[error("reading mount config files: {0}")]
    ReadingMountConfigFiles(std::io::Error),
    #[error("decoding mount config files: {0}")]
    DecodingMountConfigFiles(serde_yaml::Error),
    #[error("error decrypting secrets: {0}")]
    SecretDecryptionFailure(#[from] DecryptionError),
    #[error("command string is empty")]
    EmptyCommand,
    #[error("couldn't create tempdir: {0}")]
    CreatingTempDir(std::io::Error),
    #[error("couldn't create temp file: {0}")]
    CreatingTempFile(std::io::Error),
    #[error("couldn't create symlink to decrypted secret: {0}")]
    CreatingSymlink(std::io::Error),
    #[error("couldn't cleanup dangling symlink: {0}")]
    DeletingSymlink(std::io::Error),
    #[error("error fetching secrets from backing store: {0}")]
    FetchingSecretsErr(Box<dyn std::error::Error>),
    #[error("secret {0} was not valid UTF-8 for an environment var: {1}")]
    NotValidUTF8(String, std::string::FromUtf8Error),
    #[error("error running process: {0}")]
    ForkingProcess(std::io::Error),
    #[error("no such secret: {0}")]
    NoSuchSecret(String),
    #[error("creating data pipe: {0}")]
    CreatingDataPipe(std::io::Error),
    #[error("writing secret to file {0}")]
    WritingToFile(std::io::Error),
}

pub async fn run_process<B>(
    argv: &[String],
    secrets: &HashMap<String, &Secret>,
    exposures: &Exposures,
    identities: &[Box<dyn Identity>],
    backing: &B,
) -> Result<ExitStatus, ProcessRunningError>
where
    B: SecretStorage,
    ProcessRunningError: From<<B as SecretStorage>::Error>,
{
    let first = argv.first().ok_or(ProcessRunningError::EmptyCommand)?;
    let mut cmd = Command::new(first);
    for arg in argv[1..].iter() {
        cmd.arg(arg);
    }

    // TODO: permissions?
    let secret_dir = tempfile::tempdir().map_err(ProcessRunningError::CreatingTempDir)?;
    cmd.env(
        "SECRETS_FILE_DIR",
        secret_dir
            .path()
            .to_str()
            .expect("we should be able to represent all paths as os strs"),
    );

    let mut buf = Vec::new();
    for (name, exposure_set) in exposures.files.iter() {
        let secret = secrets
            .get(name)
            .ok_or_else(|| ProcessRunningError::NoSuchSecret(name.to_string()))?;

        let reader = backing.read(&secret.path).await?;
        let mut reader = decrypt_bytes(reader, identities).await?;
        reader
            .read_to_end(&mut buf)
            .await
            .map_err(|e| ProcessRunningError::FetchingSecretsErr(Box::new(e)))?;

        for file_spec in exposure_set.iter() {
            let dest_path = secret_dir.path().join(&secret.name);

            let mut file = OpenOptions::new()
                .mode(0o0600)
                .create(true)
                .truncate(true)
                .write(true)
                .open(&dest_path)
                .await
                .map_err(ProcessRunningError::CreatingTempFile)?;

            file.write_all(&buf)
                .await
                .map_err(ProcessRunningError::WritingToFile)?;

            tokio::fs::symlink(&dest_path, &file_spec.path)
                .await
                .map_err(ProcessRunningError::CreatingSymlink)?;

            buf.truncate(0);
        }
    }

    let mut buf = String::new();
    for (name, exposure_set) in exposures.envs.iter() {
        let secret = secrets
            .get(name)
            .ok_or_else(|| ProcessRunningError::NoSuchSecret(name.to_string()))?;

        let reader = backing.read(&secret.path).await?;
        let mut reader = decrypt_bytes(reader, identities).await?;
        reader
            .read_to_string(&mut buf)
            .await
            .map_err(|e| ProcessRunningError::FetchingSecretsErr(Box::new(e)))?;
        for env_spec in exposure_set.iter() {
            cmd.env(&env_spec.name, &buf);
        }

        buf.truncate(0);
    }

    let mut process_handle = cmd.spawn().map_err(ProcessRunningError::ForkingProcess)?;
    let process_fut = process_handle.wait();
    tokio::pin!(process_fut);
    // NOTE: we actually need to forward _any_ signal here, not just SIGINT
    let ctrl_c_fut = tokio::signal::ctrl_c();

    let result = loop {
        tokio::select! {
            finished_process = process_fut => {
                // TODO: Properly handle this error
                break finished_process.expect("Error joining process");
            },
            _ = ctrl_c_fut => {
                // process_handle.
                todo!()
            },
        }
    };

    drop(secret_dir);

    // Clean up dangling symlinks
    for exposure_set in exposures.files.values() {
        for spec in exposure_set.iter() {
            if let Err(e) = tokio::fs::remove_file(&spec.path)
                .await
                .map_err(ProcessRunningError::DeletingSymlink)
            {
                // Failure to delete these isn't worth returning an error, because
                // these are just vanity symlinks that were pointing to our
                // now-deleted temp dir
                eprintln!("error cleaning up symlink: {e}");
            };
        }
    }

    Ok(result)
}

impl From<S3SecretStorageError> for ProcessRunningError {
    fn from(value: S3SecretStorageError) -> Self {
        ProcessRunningError::FetchingSecretsErr(Box::new(value))
    }
}
