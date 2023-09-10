use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;

use crate::{Exposures, Secret, SecretError, SecretStorage};

mod builder;
pub use builder::{StateBuilder, StateBuilderError};

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
    pub exposures: Exposures,
    pub private_key_paths: Vec<PathBuf>,

    pub storage: S,

    _data1: PhantomData<E>,
}

impl<S, E> State<S, E>
where
    S: SecretStorage<Error = E>,
    E: SecretError + 'static + Sized,
{
    pub fn new(
        secrets: Vec<Secret>,
        exposures: Exposures,
        private_key_paths: Vec<PathBuf>,
        storage: S,
    ) -> Self {
        let secrets = secrets.into_iter().map(|s| (s.name.clone(), s)).collect();
        Self {
            secrets,
            exposures,
            private_key_paths,
            storage,

            _data1: Default::default(),
        }
    }
}
