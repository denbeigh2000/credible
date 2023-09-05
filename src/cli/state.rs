use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;

use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::secret::ExposureSpec;
use crate::{
    Exposures,
    IntoSecretStorage,
    ProcessRunningError,
    Secret,
    SecretError,
    SecretStorage,
};

#[derive(thiserror::Error, Debug)]
pub enum ExposureLoadingError {
    #[error("error reading mount config file: {0}")]
    ReadingMountConfigFiles(std::io::Error),
    #[error("error decoding mount config file: {0}")]
    DecodingMountConfigFiles(serde_yaml::Error),
}

pub struct State<S, E>
where
    S: SecretStorage,
    E: SecretError,
{
    pub secrets: HashMap<String, Secret>,
    pub exposures: Option<()>,
    pub private_key_paths: Vec<PathBuf>,

    pub storage: S,

    _data1: PhantomData<E>,
}

impl<S, E> State<S, E>
where
    S: SecretStorage<Error = E>,
    E: SecretError + 'static + Sized,
{
    pub fn new(secrets: Vec<Secret>, private_key_paths: Vec<PathBuf>, storage: S) -> Self {
        let secrets = secrets.into_iter().map(|s| (s.name.clone(), s)).collect();
        Self {
            secrets,
            exposures: None,
            private_key_paths,
            storage,

            _data1: Default::default(),
        }
    }

    pub async fn get_exposures(
        &self,
        config_files: &[PathBuf],
    ) -> Result<Exposures, ExposureLoadingError> {
        let mut exposures = Exposures::default();
        for path in config_files {
            let mut f = File::open(&path)
                .await
                .map_err(ExposureLoadingError::ReadingMountConfigFiles)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .await
                .map_err(ExposureLoadingError::ReadingMountConfigFiles)?;
            let data: HashMap<String, Vec<ExposureSpec>> = serde_yaml::from_slice(&buf)
                .map_err(ExposureLoadingError::DecodingMountConfigFiles)?;
            exposures.add_config(data);
        }

        Ok(exposures)
    }
}

#[derive(Default)]
pub struct StateBuilder {
    secrets: Option<Vec<Secret>>,
    private_key_paths: Option<Vec<PathBuf>>,
}

impl StateBuilder {
    pub fn set_secrets(self, secrets: Vec<Secret>) -> Self {
        Self {
            secrets: Some(secrets),
            ..self
        }
    }

    pub fn set_private_key_paths(self, paths: Option<Vec<PathBuf>>) -> Self {
        Self {
            private_key_paths: paths,
            ..self
        }
    }

    pub async fn build<I>(
        self,
        imp: I,
    ) -> State<<I as IntoSecretStorage>::Impl, <I as IntoSecretStorage>::Error>
    where
        I: IntoSecretStorage + 'static,
        <I as IntoSecretStorage>::Error: 'static,
        <I as IntoSecretStorage>::Impl: 'static,
        ProcessRunningError: From<<I as IntoSecretStorage>::Error>,
    {
        let private_key_paths = self
            .private_key_paths
            .unwrap_or_else(|| {
                let home = match std::env::var("HOME") {
                    Ok(homedir) => homedir,
                    Err(_) => return Vec::new(),
                };

                let mut ssh_dir = PathBuf::new();
                ssh_dir.push(home);
                ssh_dir.push(".ssh");

                let rsa_path = ssh_dir.join("id_rsa");
                let ed25519_path = ssh_dir.join("id_ed25519");
                vec![rsa_path, ed25519_path]
            })
            .into_iter()
            .filter(|p| p.exists())
            .collect();

        let backing = imp.build().await;
        State::new(self.secrets.unwrap(), private_key_paths, backing)
    }
}
