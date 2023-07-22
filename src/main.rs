use std::fs;
use std::path::PathBuf;

use age_flake_tool::{
    GroupWrapper,
    MountSecretError,
    SecretManager,
    SecretManagerConfig,
    UserWrapper,
};
use clap::{Parser, Subcommand};
use thiserror::Error;

#[derive(Parser, Debug)]
struct CliParams {
    #[arg(short, long, env)]
    secret_root: PathBuf,

    #[arg(short, long, env, default_value = "0")]
    user: UserWrapper,

    #[arg(short, long, env, default_value = "0")]
    group: GroupWrapper,

    #[arg(short, long, env)]
    config_file: PathBuf,

    #[command(subcommand)]
    action: Actions,
}

#[derive(Subcommand, Debug)]
enum Actions {
    Mount {},
    Edit { secret_name: String },
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
    MountingSecrets(#[from] MountSecretError),
}

fn real_main() -> Result<(), MainError> {
    let args = CliParams::try_parse()?;
    let data = fs::read(args.config_file).map_err(MainError::ReadingConfigFile)?;
    let config: SecretManagerConfig = serde_json::from_slice(&data)?;

    let manager: SecretManager = config.into();

    match args.action {
        Actions::Mount {} => manager.mount_secrets()?,
        Actions::Edit { secret_name: _ } => todo!(),
    };

    Ok(())
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
