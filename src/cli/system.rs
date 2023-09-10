use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;

pub use system::UnmountSecretsError;

use super::{ExposureLoadingError, State};
use crate::age::{get_identities, DecryptionError};
use crate::{system, SecretError, SecretStorage};

pub async fn mount<S, E>(
    state: &State<S, E>,
    mount_point: &Path,
    secret_dir: &Path,
) -> Result<ExitStatus, MountSecretsError>
where
    S: SecretStorage<Error = E>,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
{
    let identities = get_identities(&state.private_key_paths)?;

    if !state.exposures.envs.is_empty() {
        panic!("env exposures on system mount");
    }

    system::mount(
        mount_point,
        secret_dir,
        &state.secrets,
        &state.exposures.files,
        &identities,
        &state.storage,
    )
    .await?;

    Ok(ExitStatus::from_raw(0))
}

pub async fn unmount(
    mount_point: &Path,
    secret_dir: &Path,
) -> Result<ExitStatus, UnmountSecretsError> {
    system::unmount(mount_point, Some(secret_dir), None).await?;

    Ok(ExitStatus::from_raw(0))
}

#[derive(thiserror::Error, Debug)]
pub enum MountSecretsError {
    #[error("error mounting secrets: {0}")]
    MountingSecrets(#[from] system::MountSecretsError),
    #[error("error reading identities: {0}")]
    ReadingIdentities(#[from] DecryptionError),
    #[error("error loading exposures: {0}")]
    LoadingExposures(#[from] ExposureLoadingError),
}
