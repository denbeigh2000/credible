use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use credible::StorageConfig::S3;
use credible::{
    CliExposureSpec,
    EditSecretError,
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
    /// Path to the configuration file. If not provided, will search upward for
    /// files named credible.yaml.
    config_file: Option<PathBuf>,

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

    /// Secrets to expose to the executed command, in the following format:
    /// - file:secret-name:/path/to/file
    #[arg(long, env = "CREDIBLE_MOUNT_CONFIGS", value_delimiter = ',')]
    mount: Vec<CliExposureSpec>,
    /// Specify YAML files to load exposure specs from
    #[arg(long, env = "CREDIBLE_MOUNT_CONFIG_PATHS", value_delimiter = ',')]
    mount_config: Vec<PathBuf>,
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
    #[arg(long, env = "CREDIBLE_MOUNT_CONFIGS", value_delimiter = ',')]
    mount: Vec<CliExposureSpec>,
    /// Specify YAML files to load exposure specs from
    #[arg(long, env = "CREDIBLE_MOUNT_CONFIG_PATHS", value_delimiter = ',')]
    mount_config: Vec<PathBuf>,
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
    #[error("no config file given, and no credible.yaml found")]
    NoConfigFile,
    #[error("couldn't read config file: {0}")]
    ReadingConfigFile(std::io::Error),
    #[error("invalid config file: {0}")]
    ParsingConfigFile(#[from] serde_yaml::Error),
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

fn find_config_file() -> Option<PathBuf> {
    let mut directory = std::env::current_dir().ok()?;
    loop {
        let candidate = directory.join("credible.yaml");
        if candidate.exists() {
            return Some(candidate);
        }

        match directory.parent() {
            None => return None,
            Some(p) => directory = p.to_owned(),
        }
    }
}

async fn real_main() -> Result<(), MainError> {
    let args = CliParams::try_parse()?;
    let config_file = args
        .config_file
        .or_else(find_config_file)
        .ok_or(MainError::NoConfigFile)?;
    let data = fs::read(config_file).map_err(MainError::ReadingConfigFile)?;
    let config: SecretManagerConfig = serde_yaml::from_slice(&data)?;

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
        Actions::RunCommand(args) => {
            manager
                .run_command(&args.cmd, args.mount, &args.mount_config)
                .await?
        }
        Actions::Upload(args) => manager.upload(&args.secret_name, &args.source_file).await?,
    };

    Ok(())
}

#[tokio::main]
async fn main() {
    let code = match real_main().await {
        Ok(_) => 0,
        Err(MainError::ParsingCliArgs(e)) => {
            eprintln!("{e}");
            1
        },
        Err(e) => {
            eprintln!("error: {e}");
            1
        },
    };

    std::process::exit(code);
}
