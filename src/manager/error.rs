use std::process::ExitStatus;

use thiserror::Error;

use crate::age::{DecryptionError, EncryptionError};

#[derive(Error, Debug)]
pub enum CreateUpdateSecretError {
    #[error("error reading source data: {0}")]
    ReadSourceData(std::io::Error),
    #[error("failed to write to backing store: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
    #[error("error encrypting secret: {0}")]
    EncryptingSecret(#[from] EncryptionError),
}

#[derive(Error, Debug)]
pub enum UploadSecretError {
    #[error("no configured secret with name {0}")]
    NoSuchSecret(String),
    #[error("error creating pipe: {0}")]
    CreatingPipe(std::io::Error),
    #[error("error reading source file: {0}")]
    ReadingSourceFile(std::io::Error),
    #[error("error encrypting secret: {0}")]
    EncryptingData(#[from] EncryptionError),
    #[error("error writing encrpyted data to store: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
}

#[derive(Error, Debug)]
pub enum EditSecretError {
    #[error("no secret named {0}")]
    NoSuchSecret(String),
    #[error("error creating tempfile: {0}")]
    CreatingTempFile(std::io::Error),
    #[error("error opening tempfile: {0}")]
    OpeningTempFile(std::io::Error),
    #[error("error creating pipe: {0}")]
    CreatingPipe(std::io::Error),
    #[error("error fetching existing secret from store: {0}")]
    FetchingFromStore(Box<dyn std::error::Error>),
    #[error("error decrypting existing secret: {0}")]
    DecryptingSecret(#[from] DecryptionError),
    #[error("error encrypting updated secret: {0}")]
    EncryptingSecret(#[from] EncryptionError),
    #[error("error uploading updated secret: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
    #[error("error invoking editor: {0}")]
    InvokingEditor(std::io::Error),
    #[error("editor exited with non-success status: {0}")]
    EditorBadExit(ExitStatus),
}
