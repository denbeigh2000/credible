use std::path::Path;

use age::cli_common::read_identities;
use age::{Decryptor, Encryptor, Identity, Recipient};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::{
    FuturesAsyncReadCompatExt,
    FuturesAsyncWriteCompatExt,
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};

use crate::util::{BoxedAsyncReader, BoxedAsyncWriter};

#[derive(thiserror::Error, Debug)]
pub enum EncryptionError {
    #[error("error creating encryption stream: {0}")]
    CreatingStream(age::EncryptError),
    #[error("no valid recipients found")]
    NoRecipientsFound,
    #[error("error writing encrypted secret: {0}")]
    WritingSecret(std::io::Error),
    #[error("error writing encrypted secret to backing store: {0}")]
    WritingToBackingStore(Box<dyn std::error::Error>),
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
    R: AsyncRead + Unpin + Sized + 'static,
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

// NOTE: Due to limitations in the age library, we have to explicitly call
// shutdown() on this the returned AsyncWriter
pub async fn encrypt_bytes<W>(
    writer: W,
    public_keys: &[String],
) -> Result<BoxedAsyncWriter, EncryptionError>
where
    W: AsyncWrite + Unpin + 'static,
{
    let compat_writer = writer.compat_write();
    let recipients = public_keys
        .iter()
        .filter_map(|key| parse_recipient(key).ok())
        .collect::<Vec<Box<dyn Recipient + Send>>>();
    if recipients.is_empty() {
        return Err(EncryptionError::NoRecipientsFound);
    }
    let encrypted_writer = Encryptor::with_recipients(recipients)
        .ok_or(EncryptionError::NoRecipientsFound)?
        .wrap_async_output(compat_writer)
        .await
        .map_err(EncryptionError::CreatingStream)?
        .compat_write();

    Ok(BoxedAsyncWriter::from_async_write(encrypted_writer))
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
