use std::process::ExitStatus;

use super::{ExposureLoadingError, State};
use crate::age::{get_identities, DecryptionError};
use crate::{process, SecretError, SecretStorage};

pub async fn run<S, E>(
    state: &State<S, E>,
    argv: &[String],
) -> Result<ExitStatus, ProcessRunningError>
where
    S: SecretStorage<Error = E>,
    E: SecretError,
    <S as SecretStorage>::Error: 'static + Sized,
    process::ProcessRunningError: From<E>,
{
    log::debug!("{} env exposures", state.exposures.envs.len());
    log::debug!("{} file exposures", state.exposures.files.len());
    let identities = get_identities(&state.private_key_paths)?;
    log::debug!("found {} identities", identities.len());
    let result = process::run_process(
        argv,
        &state.secrets,
        &state.exposures,
        &identities,
        &state.storage,
    )
    .await?;
    log::debug!(
        "process exited with status {}",
        result
            .code()
            .map(|s| s.to_string())
            .unwrap_or_else(|| String::from("<unknown>"))
    );
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
