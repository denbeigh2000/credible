use std::path::PathBuf;
use std::process::ExitStatus;
use std::{fs, unimplemented};

use clap::Parser;
use credible::cli::Actions;
use credible::StorageConfig::S3;
use credible::{cli, SecretManagerConfig};
use thiserror::Error;

use crate::cli::CliParams;

/*
* credible system mount
* credible system unmount
* credible secret edit ...
* credible secret create ...
* credible run-command ...
*/

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
    #[error("error: {0}")]
    Executing(#[from] cli::Error),
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

async fn real_main() -> Result<ExitStatus, MainError> {
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

    let code = match args.action {
        Actions::RunCommand(args) => cli::process(&state, args).await?,
        Actions::System(cmd) => cli::system(&state, cmd).await?,
        Actions::Secret(cmd) => cli::secret(&state, cmd).await?,
    };
    Ok(code)
}

#[tokio::main]
async fn main() {
    let code = match real_main().await {
        Ok(status) => status.code().unwrap_or_default(),
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
