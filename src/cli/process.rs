use std::path::PathBuf;
use std::process::ExitStatus;

use super::{ExposureLoadingError, State};
use crate::age::{get_identities, DecryptionError};
use crate::{process, CliExposureSpec, SecretError, SecretStorage};

pub async fn run<S, E>(
    state: &State<S, E>,
    argv: &[String],
    exposure_flags: Vec<CliExposureSpec>,
    config_files: &[PathBuf],
) -> Result<ExitStatus, ProcessRunningError>
where
    S: SecretStorage<Error = E>,
    E: SecretError,
    <S as SecretStorage>::Error: 'static + Sized,
    process::ProcessRunningError: From<E>,
{
    let mut exposures = state.get_exposures(config_files).await?;
    exposures.add_cli_config(exposure_flags);
    let identities = get_identities(&state.private_key_paths)?;
    let result = process::run_process(
        argv,
        &state.secrets,
        &exposures,
        &identities,
        &state.storage,
    )
    .await?;
    Ok(result)
}

#[derive(thiserror::Error, Debug)]
pub enum ProcessRunningError {
    #[error("loading exposures: {0}")]
    LoadingExposures(#[from] ExposureLoadingError),
    #[error("loading identities: {0}")]
    LoadingIdentities(#[from] DecryptionError),
    #[error("running process: {0}")]
    RunningProcess(#[from] process::ProcessRunningError),
}
