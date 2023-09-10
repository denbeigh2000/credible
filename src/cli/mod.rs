use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

pub mod args;
pub use args::*;
pub mod process;
pub mod secret;
pub mod state;
pub mod system;
pub use state::*;

use crate::{ProcessRunningError, SecretError, SecretStorage};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("mounting secrets: {0}")]
    MountingSecrets(#[from] system::MountSecretsError),
    #[error("unmounting secrets: {0}")]
    UnmountingSecrets(#[from] system::UnmountSecretsError),
    #[error("running subcommand: {0}")]
    RunningProcess(#[from] process::ProcessRunningError),
    #[error("uploading secret: {0}")]
    UploadingSecret(#[from] secret::CreateUpdateSecretError),
    #[error("editing secret: {0}")]
    EditingSecret(#[from] secret::EditSecretError),
}

pub async fn process<S, E>(state: &State<S, E>, args: RunCommandArgs) -> Result<ExitStatus, Error>
where
    S: SecretStorage<Error = E>,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
    ProcessRunningError: From<E>,
{
    let res = process::run(state, &args.cmd, args.mount, &args.mount_config).await?;
    Ok(res)
}

pub async fn system<S, E>(state: &State<S, E>, action: SystemAction) -> Result<ExitStatus, Error>
where
    S: SecretStorage<Error = E>,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
{
    match action {
        SystemAction::Mount(a) => {
            system::mount(
                state,
                &a.mount_point,
                &a.secret_dir,
                &a.mount_config,
                a.mount,
            )
            .await?
        }
        SystemAction::Unmount(a) => system::unmount(&a.mount_point, &a.secret_dir).await?,
    };

    Ok(ExitStatus::from_raw(0))
}

pub async fn secret<S, E>(s: &State<S, E>, action: SecretAction) -> Result<ExitStatus, Error>
where
    S: SecretStorage<Error = E>,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
{
    match action {
        SecretAction::Edit(a) => secret::edit(s, &a.editor, &a.secret_name).await?,
        SecretAction::Upload(a) => secret::create(s, &a.secret_name, Some(&a.source_file)).await?,
    };

    Ok(ExitStatus::from_raw(0))
}
