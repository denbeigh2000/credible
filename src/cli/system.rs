use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;

pub use system::UnmountSecretsError;

use super::State;
use crate::age::{get_identities, DecryptionError};
use crate::{system, SecretError, SecretStorage};

pub async fn mount<S, E>(
    state: &State<S, E>,
    mount_point: &Path,
    secret_dir: &Path,
) -> Result<ExitStatus, MountSecretsError>
where
    S: SecretStorage,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
{
    let identities = get_identities(&state.private_key_paths)?;

    let exposures = HashMap::new();

    system::mount(
        mount_point,
        secret_dir,
        &state.secrets,
        &exposures,
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
    system::unmount(mount_point, secret_dir).await?;

    Ok(ExitStatus::from_raw(0))
}

#[derive(thiserror::Error, Debug)]
pub enum MountSecretsError {
    #[error("error mounting secrets: {0}")]
    MountingSecrets(#[from] system::MountSecretsError),
    #[error("error reading identities: {0}")]
    ReadingIdentities(#[from] DecryptionError),
}
