use std::path::PathBuf;

use clap::{Parser, Subcommand};
use simplelog::LevelFilter;

use crate::secret::ExposureSpec;
use crate::{GroupWrapper, UserWrapper};

#[derive(Parser, Debug)]
pub struct CliParams {
    #[arg(short, long, env = "CREDIBLE_CONFIG_FILES", value_delimiter = ',')]
    /// Path to a configuration file. Can be repeated to compose multiple
    /// config files. If not provided, will search upward for
    /// files named credible.yaml.
    ///
    /// Specify multiple in an environment variable by separating with commas
    pub config_file: Vec<PathBuf>,

    /// Secrets to expose, in the following formats:
    ///
    /// - env:secret-name:ENV_VAR_NAME
    ///
    /// - file:secret-name:/path/to/file
    ///
    #[arg(long, env = "CREDIBLE_EXPOSURE_CONFIGS", value_delimiter = ',')]
    pub exposure: Vec<ExposureSpec>,

    #[arg(short, long, env = "CREDIBLE_PRIVATE_KEY_PATHS", value_delimiter = ',')]
    /// Comma-separated list of local private keys to use for decryption.
    ///
    /// If not provided, $HOME/.ssh/id_rsa and $HOME/.ssh/id_ecsda are checked.
    pub private_key_paths: Option<Vec<PathBuf>>,

    #[arg(short, long, env = "CREDIBLE_LOG_LEVEL", default_value = "warn")]
    /// Level to display logs at (off, error, warn, info, debug, trace)
    pub log_level: LevelFilter,

    #[arg(short = 'z', long, env = "CREDIBLE_CREDENTIALS_FILE")]
    /// Path to a key=value file that will set environment variables for the
    /// process (useful for providing credentials to secret storage providers).
    ///
    /// If not provided and $HOME/.config/credible/credentials exists, it will be
    /// loaded.
    pub credentials_file: Option<PathBuf>,

    #[command(subcommand)]
    pub action: Actions,
}

#[derive(Subcommand, Debug)]
pub enum SystemAction {
    /// Mount all secrets in the configuration file on the current system
    Mount(Box<MountArgs>),
    /// Unmount our currently-mounted secrets, if any
    Unmount(UnmountArgs),
}

#[derive(Subcommand, Debug)]
pub enum SecretAction {
    /// Upload a new secret to the store
    Upload(UploadCommandArgs),
    /// Edit a currently-managed secret
    Edit(EditCommandArgs),
}

#[derive(Subcommand, Debug)]
pub enum Actions {
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
pub struct MountArgs {
    #[clap(
        long,
        short,
        env = "CREDIBLE_MOUNT_POINT",
        default_value = "/run/credible.d"
    )]
    /// System-managed directory to mount secrets in.
    pub mount_point: PathBuf,

    #[clap(
        long,
        short,
        env = "CREDIBLE_SECRET_DIR",
        default_value = "/run/credible"
    )]
    /// Directory users should access secrets from.
    pub secret_dir: PathBuf,

    #[arg(short, long, env = "CREDIBLE_OWNER_USER")]
    /// Default user to own secrets (if not provided, current user will be
    /// used)
    pub user: Option<UserWrapper>,

    #[arg(short, long, env = "CREDIBLE_OWNER_GROUP")]
    /// Default group to own secrets (if not provided, current group will be
    /// used)
    pub group: Option<GroupWrapper>,
}

#[derive(clap::Args, Debug)]
pub struct UnmountArgs {
    #[clap(
        long,
        short,
        env = "CREDIBLE_MOUNT_POINT",
        default_value = "/run/credible.d"
    )]
    /// System-managed directory to mount secrets in.
    pub mount_point: PathBuf,

    #[clap(long, short, default_value = "/run/credible")]
    /// Directory users should access secrets from.
    pub secret_dir: PathBuf,
}

#[derive(clap::Args, Debug)]
pub struct RunCommandArgs {
    /// Command arguments to run
    pub cmd: Vec<String>,
}

#[derive(clap::Args, Debug)]
pub struct UploadCommandArgs {
    /// Name of the secret (as defined in conf file) to upload
    pub secret_name: String,

    /// Plaintext file to read content from
    #[clap(default_value = "/dev/stdin")]
    pub source_file: PathBuf,
}

#[derive(clap::Args, Debug)]
pub struct EditCommandArgs {
    #[arg(short, long, env = "EDITOR")]
    /// Editor to open for editing the secret
    pub editor: String,
    /// Name of the secret to edit
    pub secret_name: String,
}
