use std::fs;
use std::path::PathBuf;

use age_flake_tool::BackingConfig::S3;
use age_flake_tool::{
    ExposedSecretConfig,
    GroupWrapper,
    MountSecretError,
    SecretManagerBuilder,
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
    RunCommand(RunCommandArgs),
}

#[derive(clap::Args, Debug)]
struct RunCommandArgs {
    cmd: Vec<String>,
    #[clap(long, short, num_args = 0..)]
    mount: Vec<ExposedSecretConfig>,
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

async fn real_main() -> Result<(), MainError> {
    let args = CliParams::try_parse()?;
    let data = fs::read(args.config_file).map_err(MainError::ReadingConfigFile)?;
    let config: SecretManagerConfig = serde_json::from_slice(&data)?;

    // TODO: Have some better registry/DI-style pattern here for better
    // extension
    let cfg = match config.backing_config {
        S3(c) => c,
        _ => unimplemented!(),
    };

    let manager = SecretManagerBuilder::default()
        .set_secret_root(config.secret_root)
        .set_owner_user(config.owner_user.into())
        .set_owner_group(config.owner_group.into())
        .set_secrets(config.secrets)
        .set_keys(config.keys)
        .build(cfg)
        .await;

    match args.action {
        Actions::Mount {} => manager.mount_secrets().await?,
        Actions::Edit { secret_name: _ } => todo!(),
        Actions::RunCommand {..} => todo!(),
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
