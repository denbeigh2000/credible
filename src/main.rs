use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use credible::StorageConfig::S3;
use credible::{
    EditSecretError,
    ExposedSecretConfig,
    GroupWrapper,
    MountSecretsError,
    ProcessRunningError,
    SecretManagerBuilder,
    SecretManagerConfig,
    UnmountSecretsError,
    UploadSecretError,
    UserWrapper,
};
use thiserror::Error;

#[derive(Parser, Debug)]
struct CliParams {
    #[arg(short, long, env = "CREDIBLE_CONFIG_FILE")]
    /// Path to the configuration file.
    config_file: PathBuf,

    #[arg(short, long, env = "CREDIBLE_PRIVATE_KEY_PATHS", value_delimiter = ',')]
    /// Comma-separated list of local private keys to use for decryption.
    /// If not provided, $HOME/.ssh/id_rsa and $HOME/.ssh/id_ecsda are checked.
    private_key_paths: Option<Vec<PathBuf>>,

    #[command(subcommand)]
    action: Actions,
}

#[derive(Subcommand, Debug)]
enum Actions {
    /// Mount all secrets in the configuration file on the current system
    Mount(Box<MountArgs>),
    /// Unmount our currently-mounted secrets, if any
    Unmount(UnmountArgs),
    /// Edit a currently-managed secret
    Edit(EditCommandArgs),
    /// Run a command with populated secrets
    RunCommand(RunCommandArgs),
    /// Upload a new secret to the store
    Upload(UploadCommandArgs),
}

#[derive(clap::Args, Debug)]
struct MountArgs {
    #[clap(
        long,
        short,
        env = "CREDIBLE_MOUNT_POINT",
        default_value = "/run/credible.d"
    )]
    /// System-managed directory to mount secrets in.
    mount_point: PathBuf,

    #[clap(
        long,
        short,
        env = "CREDIBLE_SECRET_DIR",
        default_value = "/run/credible"
    )]
    /// Directory users should access secrets from.
    secret_dir: PathBuf,

    #[arg(short, long, env = "CREDIBLE_OWNER_USER")]
    /// Default user to own secrets (if not provided, current user will be
    /// used)
    user: Option<UserWrapper>,

    #[arg(short, long, env = "CREDIBLE_OWNER_GROUP")]
    /// Default group to own secrets (if not provided, current group will be
    /// used)
    group: Option<GroupWrapper>,
}

#[derive(clap::Args, Debug)]
struct UnmountArgs {
    #[clap(
        long,
        short,
        env = "CREDIBLE_MOUNT_POINT",
        default_value = "/run/credible.d"
    )]
    /// System-managed directory to mount secrets in.
    mount_point: PathBuf,

    #[clap(
        long,
        short,
        env = "CREDIBLE_SECRET_DIR",
        default_value = "/run/credible"
    )]
    /// Directory users should access secrets from.
    secret_dir: PathBuf,
}

#[derive(clap::Args, Debug)]
struct RunCommandArgs {
    #[clap(long, short)]
    /// Secrets to expose to the executed command, in the following formats:
    /// - env:secret-name:ENV_VAR_NAME
    /// - file:secret-name:/path/to/file
    mount: Vec<ExposedSecretConfig>,
    /// Command arguments to run
    cmd: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct UploadCommandArgs {
    secret_name: String,

    source_file: PathBuf,
}

#[derive(clap::Args, Debug)]
struct EditCommandArgs {
    #[arg(short, long, env = "EDITOR")]
    editor: String,
    #[arg(short, long, env)]
    secret_name: String,
}

#[derive(Debug, Error)]
enum MainError {
    #[error("{0}")]
    ParsingCliArgs(#[from] clap::Error),
    #[error("couldn't read config file: {0}")]
    ReadingConfigFile(std::io::Error),
    #[error("invalid config file: {0}")]
    ParsingConfigFile(#[from] serde_json::Error),
    #[error("mounting secrets: {0}")]
    MountingSecrets(#[from] MountSecretsError),
    #[error("unmounting secrets: {0}")]
    UnmountingSecrets(#[from] UnmountSecretsError),
    #[error("running subcommand: {0}")]
    RunningProcess(#[from] ProcessRunningError),
    #[error("uploading secret: {0}")]
    UploadingSecret(#[from] UploadSecretError),
    #[error("editing secret: {0}")]
    EditingSecret(#[from] EditSecretError),
}

async fn real_main() -> Result<(), MainError> {
    let args = CliParams::try_parse()?;
    let data = fs::read(args.config_file).map_err(MainError::ReadingConfigFile)?;
    let config: SecretManagerConfig = serde_json::from_slice(&data)?;

    // TODO: Have some better registry/DI-style pattern here for better
    // extension
    let cfg = match config.storage {
        S3(c) => c,
        _ => unimplemented!(),
    };

    let manager = SecretManagerBuilder::default()
        .set_secrets(config.secrets)
        .set_private_key_paths(args.private_key_paths)
        .build(cfg)
        .await;

    match args.action {
        Actions::Mount(a) => {
            manager
                .mount(&a.mount_point, &a.secret_dir, &a.user, &a.group)
                .await?
        }
        Actions::Unmount(args) => manager.unmount(&args.mount_point, &args.secret_dir).await?,
        Actions::Edit(args) => manager.edit(&args.secret_name, &args.editor).await?,
        Actions::RunCommand(args) => manager.run_command(&args.cmd, &args.mount).await?,
        Actions::Upload(args) => manager.upload(&args.secret_name, &args.source_file).await?,
    };

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = real_main().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
