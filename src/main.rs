use std::path::PathBuf;
use std::process::ExitStatus;
use std::unimplemented;

use clap::Parser;
use credible::cli::Actions;
use credible::util::partition_specs;
use credible::StorageConfig::S3;
use credible::{cli, SecretManagerConfig};
use log::SetLoggerError;
use simplelog::{ConfigBuilder, LevelFilter};
use thiserror::Error;
use tokio::fs;

use crate::cli::{CliParams, StateBuilderError};

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
    #[error("couldn't read config file at {0}: {1}")]
    ReadingConfigFile(PathBuf, std::io::Error),
    #[error("invalid config file: {0}")]
    ParsingConfigFile(#[from] serde_yaml::Error),
    #[error("bad command line arguments: {0}")]
    SettingUpState(#[from] StateBuilderError),
    #[error("couldn't configure logger: {0}")]
    SettingLogger(#[from] SetLoggerError),
    #[error("error: {0}")]
    Executing(#[from] cli::Error),
}

fn find_config_file() -> Option<PathBuf> {
    let mut directory = std::env::current_dir().ok()?;
    loop {
        let candidate = directory.join("credible.yaml");
        if candidate.exists() {
            log::debug!("using config at {}", candidate.to_string_lossy());
            return Some(candidate);
        }

        match directory.parent() {
            None => return None,
            Some(p) => directory = p.to_owned(),
        }
    }
}

fn init_logger(level: LevelFilter) -> Result<(), SetLoggerError> {
    let config = ConfigBuilder::default()
        .add_filter_allow_str("credible")
        .build();

    simplelog::TermLogger::init(
        level,
        config,
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )
}

async fn real_main() -> Result<ExitStatus, MainError> {
    let args = CliParams::try_parse()?;
    init_logger(args.log_level)?;
    let config_file = match args.config_file.is_empty() {
        false => args.config_file,
        true => find_config_file()
            .map(|f| vec![f])
            .ok_or(MainError::NoConfigFile)?,
    };
    log::trace!("config loaded");

    let mut builder = cli::StateBuilder::default();
    for file in config_file {
        let data = fs::read(&file)
            .await
            .map_err(|e| MainError::ReadingConfigFile(file.to_path_buf(), e))?;
        let config: SecretManagerConfig = serde_yaml::from_slice(&data)?;

        if let Some(c) = config.exposures {
            let (files, envs) = partition_specs(c);
            builder.add_file_exposures(files)?;
            builder.add_env_exposures(envs)?;
        }

        if let Some(secrets) = config.secrets {
            builder.add_secrets(secrets);
        }

        if let Some(storage) = config.storage {
            builder = match storage {
                S3(s) => builder.set_secret_storage(s).await?,
                _ => unimplemented!(),
            };
        }
    }

    let (files, envs) = partition_specs(args.exposure);
    builder.add_file_exposures(files)?;
    builder.add_env_exposures(envs)?;

    if let Some(paths) = args.private_key_paths {
        builder.set_identities(paths);
    }
    let state = builder.build().await?;
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
            log::error!("error: {e}");
            1
        }
    };

    std::process::exit(code);
}
