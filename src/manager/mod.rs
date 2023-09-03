use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::age::{decrypt_bytes, encrypt_bytes, get_identities, EncryptionError};
use crate::process::{run_process, ProcessRunningError};
use crate::secret::ExposureSpec;
use crate::{CliExposureSpec, Exposures, Secret, SecretError, SecretStorage};

mod error;
pub use error::*;

// TODO: Best way to share these without turning into a kludge?
pub struct SecretManager<E, I>
where
    E: SecretError,
    I: SecretStorage,
{
    pub secrets: Vec<Secret>,
    pub private_key_paths: Vec<PathBuf>,

    pub storage: I,

    _data1: PhantomData<E>,
}

impl<E, I> SecretManager<E, I>
where
    E: SecretError + 'static + Sized,
    I: SecretStorage<Error = E>,
    ProcessRunningError: From<<I as SecretStorage>::Error>,
{
    pub fn new(secrets: Vec<Secret>, private_key_paths: Vec<PathBuf>, storage: I) -> Self {
        Self {
            secrets,
            private_key_paths,
            storage,

            _data1: Default::default(),
        }
    }

    pub async fn create(
        &self,
        secret: &Secret,
        source_file: Option<&Path>,
    ) -> Result<(), CreateUpdateSecretError> {
        // TODO: Check to see if this exists?
        let data = match source_file {
            Some(file) => File::open(file)
                .await
                .map_err(CreateUpdateSecretError::ReadSourceData)?,
            None => todo!("Secure tempdir editing"),
        };

        let (reader, fut) = encrypt_bytes(data, &secret.encryption_keys)
            .await
            .map_err(CreateUpdateSecretError::EncryptingSecret)?;
        self.storage
            .write(&secret.path, reader)
            .await
            .map_err(|e| CreateUpdateSecretError::WritingToStore(Box::new(e)))?;

        fut.await.map_err(|e| {
            CreateUpdateSecretError::EncryptingSecret(EncryptionError::SpawningThread(e))
        })??;

        Ok(())
    }

    pub async fn edit(
        &self,
        secret_name: &str,
        editor: &str,
    ) -> Result<ExitStatus, EditSecretError> {
        let secret = self
            .secrets
            .iter()
            .find(|s| s.name == secret_name)
            .ok_or_else(|| EditSecretError::NoSuchSecret(secret_name.to_string()))?;
        let identities = get_identities(&self.private_key_paths)?;
        // NOTE: It would be nice if this supported creating new files, too
        let reader = self
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
        let (reader, fut) = encrypt_bytes(temp_file_handle, &secret.encryption_keys).await?;
        self.storage
            .write(&secret.path, reader)
            .await
            .map_err(|e| EditSecretError::WritingToStore(Box::new(e)))?;

        fut.await
            .map_err(|e| EditSecretError::EncryptingSecret(EncryptionError::SpawningThread(e)))?
            .map_err(EditSecretError::EncryptingSecret)?;

        Ok(ExitStatus::from_raw(0))
    }

    pub async fn run_command(
        &self,
        argv: &[String],
        exposure_flags: Vec<CliExposureSpec>,
        config_files: &[PathBuf],
    ) -> Result<ExitStatus, ProcessRunningError> {
        let secrets_map = self.secrets.iter().fold(HashMap::new(), |mut acc, x| {
            acc.insert(x.name.clone(), x);
            acc
        });
        let mut exposures = Exposures::default();
        for path in config_files {
            let mut f = File::open(&path)
                .await
                .map_err(ProcessRunningError::ReadingMountConfigFiles)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .await
                .map_err(ProcessRunningError::ReadingMountConfigFiles)?;
            let data: HashMap<String, HashSet<ExposureSpec>> = serde_yaml::from_slice(&buf)
                .map_err(ProcessRunningError::DecodingMountConfigFiles)?;
            exposures.add_config(data);
        }

        let mut cli_exposure_map: HashMap<String, HashSet<ExposureSpec>> = HashMap::new();
        for exposure in exposure_flags {
            let (name, exp) = exposure.into();
            match cli_exposure_map.get_mut(&name) {
                Some(v) => v.insert(exp),
                None => cli_exposure_map
                    .insert(name, HashSet::from([exp]))
                    .is_some(),
            };
        }
        exposures.add_config(cli_exposure_map);

        let identities = get_identities(&self.private_key_paths)?;
        let status =
            run_process(argv, &secrets_map, &exposures, &identities, &self.storage).await?;

        Ok(status)
    }

    pub async fn upload(
        &self,
        secret_name: &str,
        source_file: &Path,
    ) -> Result<ExitStatus, UploadSecretError> {
        let secret = self
            .secrets
            .iter()
            .find(|s| s.name == secret_name)
            .ok_or_else(|| UploadSecretError::NoSuchSecret(secret_name.to_string()))?;

        let file = File::open(source_file)
            .await
            .map_err(UploadSecretError::ReadingSourceFile)?;

        let (reader, handle) = encrypt_bytes(file, &secret.encryption_keys).await?;
        self.storage
            .write(&secret.path, reader)
            .await
            .map_err(|e| UploadSecretError::WritingToStore(Box::new(e)))?;

        handle
            .await
            .map_err(|e| UploadSecretError::EncryptingData(EncryptionError::SpawningThread(e)))?
            .map_err(UploadSecretError::EncryptingData)?;

        Ok(ExitStatus::from_raw(0))
    }
}
