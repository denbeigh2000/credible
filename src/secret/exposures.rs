use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;

#[derive(Deserialize, Eq, PartialEq, Clone, Debug)]
#[serde(tag = "type")]
pub enum ExposureSpec {
    #[serde(alias = "file")]
    File(Box<FileExposeArgs>),
    #[serde(alias = "env")]
    Env(EnvExposeArgs),
}

impl ExposureSpec {
    pub fn file_from_str(path: &str) -> Self {
        let vanity_path = Some(path.parse().expect("infallible error"));
        let mode = None;
        let group = None;
        let owner = None;
        Self::File(Box::new(FileExposeArgs {
            vanity_path,
            mode,
            owner,
            group,
        }))
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

#[derive(Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct FileExposeArgs {
    pub vanity_path: Option<PathBuf>,
    pub mode: Option<u32>,
    pub owner: Option<crate::UserWrapper>,
    pub group: Option<crate::GroupWrapper>,
}

#[derive(Deserialize, Clone, Debug, Hash, Eq, PartialEq)]
pub struct EnvExposeArgs {
    pub name: String,
}

#[derive(Default)]
pub struct Exposures {
    pub files: HashMap<String, Vec<FileExposeArgs>>,
    pub envs: HashMap<String, Vec<EnvExposeArgs>>,
}

impl Exposures {
    pub fn add_config<I: IntoIterator<Item = (String, Vec<ExposureSpec>)>>(&mut self, config: I) {
        config.into_iter().for_each(|(name, specs)| {
            for spec in specs {
                match spec {
                    ExposureSpec::Env(env_name) => {
                        match self.envs.get_mut(&name) {
                            Some(v) => v.push(env_name),
                            None => {
                                self.envs.insert(name.clone(), vec![env_name]);
                            }
                        };
                    }

                    ExposureSpec::File(file_path) => {
                        match self.files.get_mut(&name) {
                            Some(v) => v.push(*file_path),
                            None => {
                                self.files.insert(name.clone(), vec![*file_path]);
                            }
                        };
                    }
                }
            }
        });
    }
}
