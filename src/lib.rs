use std::path::PathBuf;

use nix::unistd::{Group, User};
use serde::Deserialize;

mod wrappers;
pub use wrappers::{GroupWrapper, UserWrapper};

#[derive(Deserialize, Debug, Clone)]
pub struct Secret {
    pub name: String,
    pub path: PathBuf,
    pub encryption_keys: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RuntimeKey {
    pub private_key_path: PathBuf,
    pub secret: Secret,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SecretManagerConfig {
    pub secret_root: PathBuf,
    pub owner_user: UserWrapper,
    pub owner_group: GroupWrapper,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,
}

#[derive(Debug, Clone)]
pub struct SecretManager {
    pub secret_root: PathBuf,
    pub owner_user: User,
    pub owner_group: Group,
    pub secrets: Vec<Secret>,
    pub keys: Vec<RuntimeKey>,
}

impl From<SecretManagerConfig> for SecretManager {
    fn from(value: SecretManagerConfig) -> Self {
        Self {
            secret_root: value.secret_root,
            owner_user: value.owner_user.into(),
            owner_group: value.owner_group.into(),
            secrets: value.secrets,
            keys: value.keys,
        }
    }
}
