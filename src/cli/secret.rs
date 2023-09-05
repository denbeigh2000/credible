use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;

use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::process::Command;

use super::State;
use crate::age::{decrypt_bytes, encrypt_bytes, get_identities, DecryptionError, EncryptionError};
use crate::{SecretError, SecretStorage};

pub async fn create<S, E>(
    state: &State<S, E>,
    secret_name: &str,
    source_file: Option<&Path>,
) -> Result<ExitStatus, CreateUpdateSecretError>
where
    S: SecretStorage,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
{
    let secret = state
        .secrets
        .get(secret_name)
        .ok_or_else(|| CreateUpdateSecretError::NoSuchSecret(secret_name.to_string()))?;
    // TODO: Check to see if this exists?
    let data = match source_file {
        Some(file) => File::open(file)
            .await
            .map_err(CreateUpdateSecretError::ReadSourceData)?,
        None => todo!("Secure tempdir editing"),
    };

    let encrypted_data = encrypt_bytes(data, &secret.encryption_keys)
        .await
        .map_err(CreateUpdateSecretError::EncryptingSecret)?;
    state
        .storage
        .write(&secret.path, encrypted_data.as_slice())
        .await
        .map_err(|e| CreateUpdateSecretError::WritingToStore(Box::new(e)))?;

    Ok(ExitStatus::from_raw(0))
}

pub async fn edit<S, E>(
    state: &State<S, E>,
    editor: &str,
    secret_name: &str,
) -> Result<ExitStatus, EditSecretError>
where
    S: SecretStorage,
    E: SecretError,
    <S as SecretStorage>::Error: 'static,
{
    let secret = state
        .secrets
        .get(secret_name)
        .ok_or_else(|| EditSecretError::NoSuchSecret(secret_name.to_string()))?;
    let identities = get_identities(&state.private_key_paths)?;
    // NOTE: It would be nice if this supported creating new files, too
    let reader = state
        .storage
        .read(&secret.path)
        .await
        .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;
    let temp_file = NamedTempFile::new().map_err(EditSecretError::CreatingTempFile)?;
    let temp_file_path = temp_file.path();
    // Scope ensures temp file is closed after we write decrypted data
    {
        let mut temp_file_handle = File::create(temp_file_path)
            .await
            .map_err(EditSecretError::OpeningTempFile)?;
        let mut reader = decrypt_bytes(reader, &identities).await?;
        tokio::io::copy(&mut reader, &mut temp_file_handle)
            .await
            .map_err(EditSecretError::OpeningTempFile)?;
    }
    let editor_result = Command::new(editor)
        .arg(temp_file_path)
        .status()
        .await
        .map_err(EditSecretError::InvokingEditor)?;

    if !editor_result.success() {
        return Err(EditSecretError::EditorBadExit(editor_result));
    }

    let temp_file_handle = File::open(temp_file_path)
        .await
        .map_err(EditSecretError::OpeningTempFile)?;
    let encrypted_data = encrypt_bytes(temp_file_handle, &secret.encryption_keys).await?;
    state
        .storage
        .write(&secret.path, encrypted_data.as_slice())
        .await
        .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;

    Ok(ExitStatus::from_raw(0))
}

#[derive(thiserror::Error, Debug)]
pub enum CreateUpdateSecretError {
    #[error("no such secret: {0}")]
    NoSuchSecret(String),
    #[error("error reading source data: {0}")]
    ReadSourceData(std::io::Error),
    #[error("failed to write to backing store: {0}")]
    WritingToStore(Box<dyn std::error::Error>),
    #[error("error encrypting secret: {0}")]
    EncryptingSecret(#[from] EncryptionError),
}

#[derive(thiserror::Error, Debug)]
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

#[derive(thiserror::Error, Debug)]
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
