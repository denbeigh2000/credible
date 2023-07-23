use std::path::Path;

use age::cli_common::read_identities;
use age::{Decryptor, Identity};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::{TokioAsyncReadCompatExt, FuturesAsyncReadCompatExt};

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
    #[error("writing secret to file")]
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

pub async fn decrypt_bytes<R, W>(
    encrypted_bytes: R,
    writer: &mut W,
    identities: &[Box<dyn Identity>],
) -> Result<(), DecryptionError>
where
    R: AsyncRead + Unpin + Sized,
    W: AsyncWrite + Unpin + Sized,
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
        .map_err(DecryptionError::DecryptingSecret)?;

    let mut comp_reader = reader.compat();
    tokio::io::copy(&mut comp_reader, writer)
        .await
        .map_err(DecryptionError::WritingSecret)?;

    Ok(())
}
