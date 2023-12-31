use crate::age::DecryptionError;
use crate::secret::{EnvExposureError, FileExposureError};

#[derive(thiserror::Error, Debug)]
pub enum ProcessRunningError {
    #[error("reading mount config files: {0}")]
    ReadingMountConfigFiles(std::io::Error),
    #[error("decoding mount config files: {0}")]
    DecodingMountConfigFiles(serde_yaml::Error),
    #[error("error decrypting secrets: {0}")]
    SecretDecryptionFailure(#[from] DecryptionError),
    #[error("command string is empty")]
    EmptyCommand,
    #[error("couldn't create tempdir: {0}")]
    CreatingTempDir(std::io::Error),
    #[error("setting permissions on tempdir: {0}")]
    ChmoddingTempDir(nix::errno::Errno),
    #[error("couldn't create temp file: {0}")]
    CreatingTempFile(std::io::Error),
    #[error("couldn't create symlink to decrypted secret: {0}")]
    CreatingSymlink(std::io::Error),
    #[error("couldn't cleanup dangling symlink: {0}")]
    DeletingSymlink(std::io::Error),
    #[error("error fetching secrets from backing store: {0}")]
    FetchingSecretsErr(Box<dyn std::error::Error>),
    #[error("secret {0} was not valid UTF-8 for an environment var: {1}")]
    NotValidUTF8(String, std::string::FromUtf8Error),
    #[error("error running process: {0}")]
    ForkingProcess(std::io::Error),
    #[error("error joining child process: {0}")]
    JoiningProcess(std::io::Error),
    #[error("no such secret: {0}")]
    NoSuchSecret(String),
    #[error("creating data pipe: {0}")]
    CreatingDataPipe(std::io::Error),
    #[error("writing secret to file {0}")]
    WritingToFile(std::io::Error),
    #[error("preparing signal handlers: {0}")]
    CreatingSignalHandlers(std::io::Error),
    #[error("forwarding signal to child process: {0}")]
    SignallingChildProcess(std::io::Error),
    #[error("exposing secret files: {0}")]
    ExposingSecretFiles(#[from] FileExposureError),
    #[error("exposing secret envs: {0}")]
    ExposingSecretEnvs(#[from] EnvExposureError),
}
