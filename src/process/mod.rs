use std::collections::HashMap;
use std::process::ExitStatus;

use age::Identity;
use signal_hook_tokio::Signals;
use tokio::process::Command;
use tokio_stream::StreamExt;

use crate::secret::{clean_files, expose_env, expose_files, EnvExposeArgs, S3SecretStorageError};
use crate::util::map_secrets;
use crate::{Exposures, Secret, SecretStorage};

mod error;
pub use error::*;

mod signals;
use signals::kill;

pub async fn run_process<B>(
    argv: &[String],
    secrets: &HashMap<String, &Secret>,
    exposures: &Exposures,
    identities: &[Box<dyn Identity>],
    store: &B,
) -> Result<ExitStatus, ProcessRunningError>
where
    B: SecretStorage,
    <B as SecretStorage>::Error: 'static,
    ProcessRunningError: From<<B as SecretStorage>::Error>,
{
    let first = argv.first().ok_or(ProcessRunningError::EmptyCommand)?;
    let mut cmd = Command::new(first);
    for arg in argv[1..].iter() {
        cmd.arg(arg);
    }

    // TODO: permissions?
    let tmpdir = tempfile::tempdir().map_err(ProcessRunningError::CreatingTempDir)?;
    cmd.env(
        "SECRETS_FILE_DIR",
        tmpdir
            .path()
            .to_str()
            .expect("we should be able to represent all paths as os strs"),
    );

    // Signal interception done before setting up secrets. This lets us avoid
    // edge cases where we may leave secrets around without cleaning up
    let mut signals = Signals::new(1..32).map_err(ProcessRunningError::CreatingSignalHandlers)?;

    // Create files to expose to the process
    let env_pairs =
        map_secrets(secrets, exposures.envs.iter()).map_err(ProcessRunningError::NoSuchSecret)?;
    let file_pairs =
        map_secrets(secrets, exposures.files.iter()).map_err(ProcessRunningError::NoSuchSecret)?;

    // Write env vars first, to decrease the likelihood of leaving unencrypted
    // files on-disk in case of crash
    expose_env(&mut cmd, store, &env_pairs, identities).await?;
    expose_files(tmpdir.as_ref(), store, &file_pairs, identities).await?;

    // Spawn the process, and wait for it to finish
    let mut process_handle = cmd.spawn().map_err(ProcessRunningError::ForkingProcess)?;
    let pid = process_handle.id().expect("spawned process has no PID");
    let process_fut = process_handle.wait();
    tokio::pin!(process_fut);

    let result = loop {
        tokio::select! {
            finished_process = &mut process_fut => {
                break finished_process.map_err(ProcessRunningError::JoiningProcess)?;
            },
            signal = signals.next() => {
                // NOTE: we should always be able to receive signals through the life of our process
                let signal = signal.expect("signal iterator ended prematurely");
                kill(pid, signal).await.map_err(ProcessRunningError::SignallingChildProcess)?;
            },
        }
    };

    drop(tmpdir);

    // Clean up dangling symlinks
    let paths = exposures
        .files
        .values()
        .flat_map(|e| e.iter().map(|p| p.vanity_path.as_ref()))
        .filter_map(|e| e.map(|p| p.as_path()));
    for e in clean_files(paths).await {
        // Failure to delete these isn't worth returning an error, because
        // these are just vanity symlinks that were pointing to our
        // now-deleted temp dir
        eprintln!("{e}");
    }

    Ok(result)
}

impl From<S3SecretStorageError> for ProcessRunningError {
    fn from(value: S3SecretStorageError) -> Self {
        ProcessRunningError::FetchingSecretsErr(Box::new(value))
    }
}
