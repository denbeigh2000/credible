use std::path::PathBuf;
use std::{fs, unimplemented};

use clap::{Parser, Subcommand};
use credible::StorageConfig::S3;
use credible::{cli, CliExposureSpec, GroupWrapper, SecretManagerConfig, UserWrapper};
use thiserror::Error;

/*
* How should this actually work?
*
* credible system mount
* credible system unmount
* credible secret edit ...
* credible secret create ...
* credible run-command ...
*/

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
enum SystemAction {
    /// Mount all secrets in the configuration file on the current system
    Mount(Box<MountArgs>),
    /// Unmount our currently-mounted secrets, if any
    Unmount(UnmountArgs),
}

#[derive(Subcommand, Debug)]
enum SecretAction {
    /// Upload a new secret to the store
    Upload(UploadCommandArgs),
    /// Edit a currently-managed secret
    Edit(EditCommandArgs),
}

#[derive(Subcommand, Debug)]
enum Actions {
    /// Perform system-level functionality (persistent mounting)
    #[command(subcommand)]
    System(SystemAction),
    /// Perform secret management (create/edit)
    #[command(subcommand)]
    Secret(SecretAction),
    /// Run a command with populated secrets
    RunCommand(RunCommandArgs),
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

    #[clap(long, short, default_value = "/run/credible")]
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
    /// Name of the secret (as defined in conf file) to upload
    secret_name: String,

    /// Plaintext file to read content from
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
    MountingSecrets(#[from] cli::system::MountSecretsError),
    #[error("unmounting secrets: {0}")]
    UnmountingSecrets(#[from] cli::system::UnmountSecretsError),
    #[error("running subcommand: {0}")]
    RunningProcess(#[from] cli::process::ProcessRunningError),
    #[error("uploading secret: {0}")]
    UploadingSecret(#[from] cli::secret::CreateUpdateSecretError),
    #[error("editing secret: {0}")]
    EditingSecret(#[from] cli::secret::EditSecretError),
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

    let state = cli::StateBuilder::default()
        .set_secrets(config.secrets)
        .set_private_key_paths(args.private_key_paths)
        .build(cfg)
        .await;

    match args.action {
        Actions::RunCommand(args) => {
            cli::process::run(&state, &args.cmd, args.mount, &args.mount_config).await?
        }
        Actions::System(cmd) => match cmd {
            SystemAction::Mount(a) => {
                cli::system::mount(&state, &a.mount_point, &a.secret_dir).await?
            }
            SystemAction::Unmount(a) => {
                cli::system::unmount(&a.mount_point, &a.secret_dir).await?
            }
        },
        Actions::Secret(cmd) => match cmd {
            SecretAction::Edit(args) => {
                cli::secret::edit(&state, &args.secret_name, &args.editor).await?
            }
            SecretAction::Upload(args) => {
                cli::secret::create(&state, &args.secret_name, Some(&args.source_file)).await?
            }
        },
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
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };

    std::process::exit(code);
}
