use std::collections::HashMap;
use std::process::ExitStatus;

use age::Identity;
use signal_hook::consts::signal;
use signal_hook_tokio::Signals;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_stream::StreamExt;

use crate::age::decrypt_bytes;
use crate::secret::S3SecretStorageError;
use crate::{Exposures, Secret, SecretStorage};

mod error;
pub use error::*;

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
    let pid = process_handle.id().unwrap();
    let process_fut = process_handle.wait();
    tokio::pin!(process_fut);

    // TODO: Generically handle all signals
    let mut signals = Signals::new([
        signal::SIGINT,
        signal::SIGABRT,
        signal::SIGHUP,
        signal::SIGUSR1,
        signal::SIGUSR2,
    ])
    .expect("failed to set up signal listeners");

    let result = loop {
        tokio::select! {
            finished_process = &mut process_fut => {
                break finished_process;
            },
            signal = signals.next() => {
                let signal = signal.unwrap();
                Command::new("kill")
                    .arg("-s")
                    .arg(signal.to_string())
                    .arg(pid.to_string())
                    .status()
                    .await
                    .expect("failed sending signal");
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

    Ok(result.expect("TODO: IOError"))
}

impl From<S3SecretStorageError> for ProcessRunningError {
    fn from(value: S3SecretStorageError) -> Self {
        ProcessRunningError::FetchingSecretsErr(Box::new(value))
    }
}
