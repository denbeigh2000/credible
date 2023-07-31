use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;

#[derive(Deserialize, Hash, Eq, PartialEq, Clone, Debug)]
#[serde(tag = "type")]
pub enum ExposureSpec {
    #[serde(alias = "file")]
    File(FileExposeArgs),
    #[serde(alias = "env")]
    Env(EnvExposeArgs),
}

impl ExposureSpec {
    pub fn file_from_str(path: &str) -> Self {
        let path = path.parse().expect("infallible error");
        Self::File(FileExposeArgs { path })
    }

    pub fn env_from_str(name: &str) -> Self {
        let name = name.parse().expect("infallible error");
        Self::Env(EnvExposeArgs { name })
    }
}

#[derive(Debug, Clone)]
pub struct CliExposureSpec {
    pub secret_name: String,
    pub exposure_spec: ExposureSpec,
}

impl From<CliExposureSpec> for (String, ExposureSpec) {
    fn from(val: CliExposureSpec) -> Self {
        (val.secret_name, val.exposure_spec)
    }
}

impl FromStr for CliExposureSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split(':').collect::<Vec<_>>();
        let (secret_name, exposure_spec) = match parts[..] {
            ["file", secret_name, path] => (secret_name, ExposureSpec::file_from_str(path)),
            ["env", secret_name, name] => (secret_name, ExposureSpec::env_from_str(name)),
            // TODO
            _ => return Err(format!("invalid cli exposure spec: {s}")),
        };

        let secret_name = secret_name.to_string();
        Ok(Self {
            secret_name,
            exposure_spec,
        })
    }
}

#[derive(Deserialize, Clone, Debug, Hash, Eq, PartialEq)]
pub struct FileExposeArgs {
    pub path: PathBuf,
}

#[derive(Deserialize, Clone, Debug, Hash, Eq, PartialEq)]
pub struct EnvExposeArgs {
    pub name: String,
}

#[derive(Default)]
pub struct Exposures {
    pub files: HashMap<String, HashSet<FileExposeArgs>>,
    pub envs: HashMap<String, HashSet<EnvExposeArgs>>,
}

impl Exposures {
    pub fn add_config<I: IntoIterator<Item = (String, HashSet<ExposureSpec>)>>(
        &mut self,
        config: I,
    ) {
        config.into_iter().for_each(|(name, specs)| {
            for spec in specs {
                match spec {
                    ExposureSpec::Env(env_name) => {
                        match self.envs.get_mut(&name) {
                            Some(v) => v.insert(env_name),
                            None => self
                                .envs
                                .insert(name.clone(), HashSet::from([env_name]))
                                .is_some(),
                        };
                    }

                    ExposureSpec::File(file_path) => {
                        match self.files.get_mut(&name) {
                            Some(v) => v.insert(file_path),
                            None => self
                                .files
                                .insert(name.clone(), HashSet::from([file_path]))
                                .is_some(),
                        };
                    }
                }
            }
        });
    }
}
