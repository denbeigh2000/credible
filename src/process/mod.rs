use std::collections::HashMap;
use std::process::ExitStatus;

use age::Identity;
use nix::sys::stat::FchmodatFlags::FollowSymlink;
use nix::sys::stat::Mode;
use signal_hook_tokio::Signals;
use tokio::process::Command;
use tokio_stream::StreamExt;

use crate::process::signals::SIGNALS;
use crate::secret::{clean_files, expose_env, expose_files, S3SecretStorageError};
use crate::util::map_secrets;
use crate::{Exposures, Secret, SecretStorage};

mod error;
pub use error::*;

mod signals;
use signals::kill;

pub async fn run_process<B>(
    argv: &[String],
    secrets: &HashMap<String, Secret>,
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

    let tmpdir = tempfile::tempdir().map_err(ProcessRunningError::CreatingTempDir)?;
    cmd.env(
        "SECRETS_FILE_DIR",
        tmpdir
            .path()
            .to_str()
            .expect("we should be able to represent all paths as os strs"),
    );

    nix::sys::stat::fchmodat(
        None,
        tmpdir.path(),
        Mode::from_bits(0o0700).unwrap(),
        FollowSymlink,
    )
    .map_err(ProcessRunningError::ChmoddingTempDir)?;

    // Signal interception done before setting up secrets. This lets us avoid
    // edge cases where we may leave secrets around without cleaning up
    let mut signals = Signals::new(SIGNALS).map_err(ProcessRunningError::CreatingSignalHandlers)?;

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
            // TODO: Something about this is causing us to lose our task and
            // exit early?
            finished_process = &mut process_fut => {
                break finished_process.map_err(ProcessRunningError::JoiningProcess)?;
            },
            signal = signals.next() => {
                // NOTE: we should always be able to receive signals through the life of our process
                let signal = signal.expect("signal iterator ended prematurely");
                if let Err(e) = kill(pid, signal).await {
                    // NOTE: If this is due to the process finishing, we can
                    // just exit the next loop.
                    eprintln!("{e}");
                }
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
