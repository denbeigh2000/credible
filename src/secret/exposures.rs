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
    pub fn file_from_str(secret_name: String, path: &str) -> Self {
        let vanity_path = Some(path.parse().expect("infallible error"));
        let mode = None;
        let group = None;
        let owner = None;
        Self::File(Box::new(FileExposeArgs {
            secret_name,
            vanity_path,
            mode,
            owner,
            group,
        }))
    }

    pub fn env_from_str(secret_name: String, name: &str) -> Self {
        let name = name.parse().expect("infallible error");
        Self::Env(EnvExposeArgs { secret_name, name })
    }
}

impl FromStr for ExposureSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split(':').collect::<Vec<_>>();
        Ok(match parts[..] {
            ["file", name, path] => ExposureSpec::file_from_str(name.to_string(), path),
            ["env", name, env] => ExposureSpec::env_from_str(name.to_string(), env),
            // TODO
            _ => return Err(format!("invalid cli exposure spec: {s}")),
        })
    }
}

#[derive(Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct FileExposeArgs {
    pub secret_name: String,
    #[serde(alias = "path")]
    pub vanity_path: Option<PathBuf>,
    pub mode: Option<u32>,
    pub owner: Option<crate::UserWrapper>,
    pub group: Option<crate::GroupWrapper>,
}

#[derive(Deserialize, Clone, Debug, Hash, Eq, PartialEq)]
pub struct EnvExposeArgs {
    pub secret_name: String,
    pub name: String,
}

#[derive(Default)]
pub struct Exposures {
    pub files: HashMap<String, Vec<FileExposeArgs>>,
    pub envs: HashMap<String, Vec<EnvExposeArgs>>,
}

impl Exposures {
    pub fn add_files<I: IntoIterator<Item = FileExposeArgs>>(&mut self, specs: I) {
        for spec in specs {
            match self.files.get_mut(&spec.secret_name) {
                Some(v) => v.push(spec),
                None => {
                    self.files.insert(spec.secret_name.clone(), vec![spec]);
                }
            };
        }
    }

    pub fn add_envs<I: IntoIterator<Item = EnvExposeArgs>>(&mut self, specs: I) {
        for spec in specs {
            match self.envs.get_mut(&spec.secret_name) {
                Some(v) => v.push(spec),
                None => {
                    self.envs.insert(spec.secret_name.clone(), vec![spec]);
                }
            };
        }
    }
}
