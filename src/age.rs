
use std::path::Path;

use age::cli_common::read_identities;
use age::{Decryptor, Encryptor, Identity, Recipient};
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio_util::compat::{
    FuturesAsyncReadCompatExt,
    FuturesAsyncWriteCompatExt,
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};

use crate::util::BoxedAsyncReader;

#[derive(thiserror::Error, Debug)]
pub enum EncryptionError {
    #[error("error creating data pipe: {0}")]
    CreatingPipe(std::io::Error),
    #[error("error spawning read thread: {0}")]
    SpawningThread(#[from] tokio::task::JoinError),
    #[error("error reading input data: {0}")]
    ReadingInput(std::io::Error),
    #[error("error writing output data: {0}")]
    WritingOutput(std::io::Error),
    #[error("error closing output stream: {0}")]
    ClosingOutput(std::io::Error),
    #[error("error creating encryption stream: {0}")]
    CreatingStream(age::EncryptError),
    #[error("no valid recipients found")]
    NoRecipientsFound,
    #[error("error writing encrypted secret: {0}")]
    WritingSecret(std::io::Error),
    #[error("error writing encrypted secret to backing store: {0}")]
    WritingToBackingStore(Box<dyn std::error::Error + Send>),
    #[error("the given public keys weren't valid")]
    InvalidRecipients,
}

#[derive(thiserror::Error, Debug)]
pub enum DecryptionError {
    #[error("error reading armored secret data: {0}")]
    ReadingArmoredSecret(age::DecryptError),
    #[error("error reading secret key: {0}")]
    ReadingSecretKey(#[from] age::cli_common::ReadError),
    #[error("error opening output file: {0}")]
    OpeningOutputFile(std::io::Error),
    #[error("error decrypting secret: {0}")]
    DecryptingSecret(age::DecryptError),
    #[error("given secret is passphrase-encrypted, which isn't supported by this tool")]
    PassphraseEncryptedFile,
    #[error("writing secret to file: {0}")]
    WritingSecret(std::io::Error),
}

fn path_to_string<P: AsRef<Path>>(path: P) -> String {
    path.as_ref().to_str().unwrap().to_string()
}

pub fn get_identities<P: AsRef<Path>>(
    paths: &[P],
) -> Result<Vec<Box<dyn Identity>>, DecryptionError> {
    let path_strings = paths.iter().map(path_to_string).collect::<Vec<_>>();
    read_identities(path_strings, None).map_err(DecryptionError::ReadingSecretKey)
}

pub async fn decrypt_bytes<R>(
    encrypted_bytes: R,
    identities: &[Box<dyn Identity>],
) -> Result<BoxedAsyncReader, DecryptionError>
where
    R: AsyncRead + Unpin + Sized + Send + 'static,
{
    let decryptor = match Decryptor::new_async(encrypted_bytes.compat())
        .await
        .map_err(DecryptionError::ReadingArmoredSecret)?
    {
        Decryptor::Passphrase(_) => return Err(DecryptionError::PassphraseEncryptedFile),
        Decryptor::Recipients(d) => d,
    };

    let key_iter = identities.iter().map(|i| i.as_ref() as &dyn Identity);
    let reader = decryptor
        .decrypt_async(key_iter)
        .map_err(DecryptionError::DecryptingSecret)?
        .compat();

    Ok(BoxedAsyncReader::from_async_read(reader))
}

pub async fn encrypt_bytes<R>(
    mut reader: R,
    public_keys: &[String],
) -> Result<
    (
        BoxedAsyncReader,
        JoinHandle<Result<(), EncryptionError>>,
    ),
    EncryptionError,
>
where
    R: AsyncRead + Send + Unpin + Send + 'static,
{
    let (r, w) = tokio_pipe::pipe().map_err(EncryptionError::CreatingPipe)?;
    let compat_writer = w.compat_write();
    let recipients = public_keys
        .iter()
        .filter_map(|key| parse_recipient(key).ok())
        .collect::<Vec<Box<dyn Recipient + Send>>>();
    if recipients.is_empty() {
        return Err(EncryptionError::NoRecipientsFound);
    }
    let mut encrypted_writer = Encryptor::with_recipients(recipients)
        .ok_or(EncryptionError::NoRecipientsFound)?
        .wrap_async_output(compat_writer)
        .await
        .map_err(EncryptionError::CreatingStream)?
        .compat_write();

    // NOTE: We spawn a thread here because AWS' SDK only exposes a
    // reader-based API, and age only exposes a writer-based API for
    // encryption, which means otherwise the user has to do this themselves to
    // avoid blocking.
    let f = tokio::spawn(async move {
        tokio::io::copy(&mut reader, &mut encrypted_writer)
            .await
            .map_err(EncryptionError::ReadingInput)?;
        encrypted_writer
            .shutdown()
            .await
            .map_err(EncryptionError::ClosingOutput)?;

        Ok(())
    });

    Ok((BoxedAsyncReader::from_async_read(r), f))
}

// [Adapted from str4d/rage (ASL-2.0)](
// https://github.com/str4d/rage/blob/85c0788dc511f1410b4c1811be6b8904d91f85db/rage/src/bin/rage/main.rs)
fn parse_recipient(s: &str) -> Result<Box<dyn Recipient + Send>, EncryptionError> {
    if let Ok(pk) = s.parse::<age::x25519::Recipient>() {
        Ok(Box::new(pk))
    } else if let Ok(pk) = s.parse::<age::ssh::Recipient>() {
        Ok(Box::new(pk))
    } else {
        Err(EncryptionError::InvalidRecipients)
    }
}
