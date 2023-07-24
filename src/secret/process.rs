use std::path::PathBuf;
use std::process::ExitStatus;

use age::Identity;
use tokio::fs::OpenOptions;
use tokio::process::Command;

use super::S3SecretBackingError;
use crate::age::{decrypt_bytes, DecryptionError};
use crate::{Secret, SecretBackingImpl};

#[derive(Clone, Debug)]
pub enum ExposureType {
    EnvironmentVariable(String),
    File(Option<PathBuf>),
}

#[derive(Clone, Debug)]
pub struct ExposedSecretConfig {
    pub name: String,
    pub exposure_type: ExposureType,
}

#[derive(thiserror::Error, Debug)]
#[error("no such secret in configuration: {0}")]
pub struct NoSuchSecret(String);

impl ExposedSecretConfig {
    pub fn into_exposed_secret(self, secrets: &[Secret]) -> Result<ExposedSecret, NoSuchSecret> {
        let secret = secrets
            .iter()
            .find(|i| i.name == self.name)
            .cloned()
            .ok_or_else(|| NoSuchSecret(self.name.clone()))?;

        Ok(ExposedSecret {
            secret,
            exposure_type: self.exposure_type,
        })
    }
}

#[derive(Clone)]
pub struct ExposedSecret {
    pub secret: Secret,
    pub exposure_type: ExposureType,
}

impl std::str::FromStr for ExposedSecretConfig {
    type Err = &'static str;

    // --expose file:tailscaleKey
    // --expose file:sshKey:/var/ssh/id_rsa
    // --expose env:tailscaleKey:TAILSCALE_API_KEY
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split(':').collect::<Vec<_>>();
        if parts.len() > 3 || parts.len() < 2 {
            return Err("wrong number of path components");
        }

        let name = parts[1].to_string();
        let exposure_type = match parts.first().copied() {
            Some("env") => {
                if parts.len() != 3 {
                    return Err("env requires exactly 3 path components");
                }

                let env_var_key = parts[2];
                Ok(ExposureType::EnvironmentVariable(env_var_key.to_string()))
            },
            Some("file") => {
                let path = parts.get(2).map(PathBuf::from);
                Ok(ExposureType::File(path))
            },
            Some(_) => Err("only supported flags are env/file"),
            None => unreachable!("we would have exited above where len < 2"),
        }?;

        Ok(ExposedSecretConfig { name, exposure_type })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ProcessRunningError {
    #[error("error decrypting secrets: {0}")]
    SecretDecryptionFailure(#[from] DecryptionError),
    #[error("command string is empty")]
    EmptyCommand,
    #[error("couldn't create tempdir: {0}")]
    CreatingTempDir(std::io::Error),
    #[error("couldn't create temp file: {0}")]
    CreatingTempFile(std::io::Error),
    #[error("couldn't create symlink to decrypted secret: {0}")]
    CreatingSymlink(std::io::Error),
    #[error("couldn't cleanup dangling symlink: {0}")]
    DeletingSymlink(std::io::Error),
    #[error("error fetching secrets from backing store: {0}")]
    BackingStoreErr(Box<dyn std::error::Error>),
    #[error("secret {0} was not valid UTF-8 for an environment var: {1}")]
    NotValidUTF8(String, std::string::FromUtf8Error),
    #[error("error running process: {0}")]
    ForkingProcess(std::io::Error),
    #[error("no such secret: {0}")]
    NoSuchSecret(String),
}

pub async fn run_process<B>(
    argv: &[String],
    exposures: &[ExposedSecret],
    identities: &[Box<dyn Identity>],
    backing: &B,
) -> Result<ExitStatus, ProcessRunningError>
where
    B: SecretBackingImpl,
    ProcessRunningError: From<<B as SecretBackingImpl>::Error>,
{
    let first = argv.first().ok_or(ProcessRunningError::EmptyCommand)?;
    let mut cmd = Command::new(first);
    for arg in argv[1..].iter() {
        cmd.arg(arg);
    }

    // TODO: permissions?
    let secret_dir = tempdir::TempDir::new("").map_err(ProcessRunningError::CreatingTempDir)?;
    cmd.env(
        "SECRETS_FILE_DIR",
        secret_dir
            .path()
            .to_str()
            .expect("we should be able to represent all paths as os strs"),
    );

    let mut cleanup_paths: Vec<PathBuf> = Vec::new();

    for exposure in exposures {
        let encrypted_bytes = backing.read(&exposure.secret.path).await?;
        match &exposure.exposure_type {
            ExposureType::EnvironmentVariable(name) => {
                let mut buf = Vec::<u8>::new();
                decrypt_bytes(&*encrypted_bytes, &mut buf, identities).await?;
                let decrypted_string = String::from_utf8(buf).map_err(|e| {
                    ProcessRunningError::NotValidUTF8(exposure.secret.name.clone(), e)
                })?;
                cmd.env(name, &decrypted_string);
            }
            ExposureType::File(maybe_path) => {
                let dest_path = secret_dir.path().join(&exposure.secret.name);

                let mut file = OpenOptions::new()
                    .mode(0o0600)
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&dest_path)
                    .await
                    .map_err(ProcessRunningError::CreatingTempFile)?;

                decrypt_bytes(&*encrypted_bytes, &mut file, identities).await?;
                if let Some(path) = maybe_path {
                    tokio::fs::symlink(&dest_path, &path)
                        .await
                        .map_err(ProcessRunningError::CreatingSymlink)?;

                    cleanup_paths.push(path.clone());
                }
            }
        }
    }

    let result = cmd
        .status()
        .await
        .map_err(ProcessRunningError::ForkingProcess)?;

    for path in cleanup_paths {
        if let Err(e) = tokio::fs::remove_file(path)
            .await
            .map_err(ProcessRunningError::DeletingSymlink)
        {
            eprintln!("{e}");
        };
    }

    Ok(result)
}

impl From<S3SecretBackingError> for ProcessRunningError {
    fn from(value: S3SecretBackingError) -> Self {
        ProcessRunningError::BackingStoreErr(Box::new(value))
    }
}
