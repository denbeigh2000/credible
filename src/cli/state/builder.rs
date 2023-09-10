use std::collections::HashSet;
use std::default;
use std::marker::PhantomData;
use std::path::PathBuf;

use super::State;
use crate::secret::{EnvExposeArgs, FileExposeArgs};
use crate::{Exposures, IntoSecretStorage, Secret, SecretError, SecretStorage};

#[derive(thiserror::Error, Debug)]
pub enum StateBuilderError {
    #[error("error opening config file at {0}: {1}")]
    ReadingConfigFile(PathBuf, std::io::Error),
    #[error("invalid config file: {0}")]
    ParsingConfigFile(#[from] serde_yaml::Error),

    #[error("duplicate secret path specified: {0}")]
    DuplicatePath(PathBuf),
    #[error("dupliecate environment variable name specified: {0}")]
    DuplicateEnvName(String),

    #[error("build() called without a storage configuration provided")]
    StorageUnset,

    #[error("multiple storage configurations provided")]
    DuplicateStorageConfig,

    #[error("error configuring storage: {0}")]
    SettingUpStorage(Box<dyn std::error::Error>),
}

enum SetState<E> {
    Unset,
    Set(E),
}

impl<E> Default for SetState<E> {
    fn default() -> Self {
        Self::Unset
    }
}

// #[derive(Default)]
pub struct StateBuilder<E, I> {
    exposures: Exposures,
    secrets: Vec<Secret>,
    storage: SetState<I>,
    private_key_paths: Option<Vec<PathBuf>>,

    seen_env_vars: HashSet<String>,
    seen_file_paths: HashSet<PathBuf>,
    seen_secret_names: HashSet<String>,

    _data1: PhantomData<E>,
}

impl<E, I> Default for StateBuilder<E, I> {
    fn default() -> Self {
        Self {
            exposures: Default::default(),
            secrets: Default::default(),
            storage: SetState::Unset,
            private_key_paths: Default::default(),

            seen_env_vars: Default::default(),
            seen_file_paths: Default::default(),
            seen_secret_names: Default::default(),

            _data1: Default::default(),
        }
    }
}

impl<E, J> StateBuilder<E, J> {
    pub fn set_identities<I: IntoIterator<Item = PathBuf>>(&mut self, items: I) {
        match &mut self.private_key_paths {
            Some(paths) => paths.extend(items),
            None => {
                let keys = items.into_iter().collect();
                self.private_key_paths = Some(keys);
            }
        }
    }

    pub async fn set_secret_storage<En, Jn, S>(
        self,
        into_storage: S,
    ) -> Result<StateBuilder<S::Error, S::Impl>, StateBuilderError>
    where
        S: IntoSecretStorage<Error = En, Impl = Jn> + 'static,
        <S as IntoSecretStorage>::Error: 'static,
        <S as IntoSecretStorage>::Impl: 'static,
        // ProcessRunningError: From<<S as IntoSecretStorage>::Error>,
    {
        let storage = into_storage.build().await;

        Ok(StateBuilder {
            exposures: self.exposures,
            secrets: self.secrets,
            storage: SetState::Set(storage),
            private_key_paths: self.private_key_paths,

            seen_env_vars: self.seen_env_vars,
            seen_file_paths: self.seen_file_paths,
            seen_secret_names: self.seen_secret_names,

            _data1: default::Default::default(),
        })
    }

    pub fn add_secrets<I: IntoIterator<Item = Secret>>(&mut self, items: I) {
        self.secrets.extend(items);
    }

    // pub async fn add_config_file(self, p: &Path) -> Result<(), StateBuilderError> {
    //     let data = fs::read(p)
    //         .await
    //         .map_err(|e| StateBuilderError::ReadingConfigFile(p.to_path_buf(), e))?;
    //     let config: SecretManagerConfig = serde_yaml::from_slice(&data)?;

    //     let (files, envs): (Vec<_>, Vec<_>) =
    //         config
    //             .exposures
    //             .into_iter()
    //             .fold((vec![], vec![]), |(mut fs, mut es), item| {
    //                 match item {
    //                     ExposureSpec::Env(s) => es.push(s),
    //                     ExposureSpec::File(s) => fs.push(*s),
    //                 };

    //                 (fs, es)
    //             });

    //     self.add_file_exposures(files)?;
    //     self.add_env_exposures(envs)?;

    //     match config.storage {
    //         StorageConfig::S3(s) => self.set_secret_storage(s).await?,
    //     };
    //     Ok(())
    // }

    pub fn add_file_exposures<I>(&mut self, args: I) -> Result<(), StateBuilderError>
    where
        I: IntoIterator<Item = FileExposeArgs>,
    {
        let mut items = Vec::new();
        for mut exposure in args.into_iter() {
            if let Some(p) = &exposure.vanity_path {
                let is_new = self.seen_file_paths.insert(p.to_owned());
                if !is_new {
                    return Err(StateBuilderError::DuplicatePath(
                        exposure.vanity_path.take().unwrap(),
                    ));
                }
            }

            items.push(exposure);
        }

        self.exposures.add_files(items);
        Ok(())
    }

    pub fn add_env_exposures<I>(&mut self, args: I) -> Result<(), StateBuilderError>
    where
        I: IntoIterator<Item = EnvExposeArgs>,
    {
        let mut items = Vec::new();
        for exposure in args.into_iter() {
            let is_new = self.seen_env_vars.insert(exposure.name.clone());
            if !is_new {
                return Err(StateBuilderError::DuplicateEnvName(exposure.name));
            }

            items.push(exposure);
        }
        self.exposures.add_envs(items);
        Ok(())
    }
}

impl<E, J> StateBuilder<E, J>
where
    E: SecretError + 'static + Sized,
    J: SecretStorage<Error = E>,
{
    pub async fn build(self) -> Result<State<J, E>, StateBuilderError> {
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

        let backing = match self.storage {
            SetState::Set(b) => b,
            SetState::Unset => return Err(StateBuilderError::StorageUnset),
        };

        Ok(State::new(
            self.secrets,
            self.exposures,
            private_key_paths,
            backing,
        ))
    }
}
