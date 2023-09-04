use age::Identity;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::EnvExposeArgs;
use crate::age::{decrypt_bytes, DecryptionError};
use crate::{Secret, SecretStorage};

pub async fn expose_env<S>(
    cmd: &mut Command,
    storage: &S,
    exposures: &[(&Secret, &Vec<EnvExposeArgs>)],
    identities: &[Box<dyn Identity>],
) -> Result<(), EnvExposureError>
where
    S: SecretStorage,
    <S as SecretStorage>::Error: 'static,
{
    // Expose environment variables to the process
    let mut buf = String::new();
    for (secret, exposure_set) in exposures {
        let reader = storage
            .read(&secret.path)
            .await
            .map_err(|e| EnvExposureError::FetchingSecret(Box::new(e)))?;
        let mut reader = decrypt_bytes(reader, identities).await?;
        reader
            .read_to_string(&mut buf)
            .await
            .map_err(|e| EnvExposureError::FetchingSecret(Box::new(e)))?;
        for env_spec in exposure_set.iter() {
            cmd.env(&env_spec.name, &buf);
        }

        buf.truncate(0);
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum EnvExposureError {
    #[error("error fetching secret: {0}")]
    FetchingSecret(Box<dyn std::error::Error + 'static>),
    #[error("error decrypting secrets: {0}")]
    DecryptingSecret(#[from] DecryptionError),
}
