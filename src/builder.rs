use std::path::PathBuf;

use nix::unistd::{Group, User};

use crate::{IntoSecretBackingImpl, RuntimeKey, Secret, SecretManager};
use crate::secret::ProcessRunningError;

#[derive(Default)]
pub struct SecretManagerBuilder {
    secret_root: Option<PathBuf>,
    owner_user: Option<User>,
    owner_group: Option<Group>,
    secrets: Option<Vec<Secret>>,
    keys: Option<Vec<RuntimeKey>>,
    private_key_paths: Option<Vec<PathBuf>>,
}

impl SecretManagerBuilder {
    pub fn set_secret_root(self, secret_root: PathBuf) -> Self {
        Self {
            secret_root: Some(secret_root),
            ..self
        }
    }

    pub fn set_owner_user(self, user: User) -> Self {
        Self {
            owner_user: Some(user),
            ..self
        }
    }

    pub fn set_owner_group(self, group: Group) -> Self {
        Self {
            owner_group: Some(group),
            ..self
        }
    }

    pub fn set_secrets(self, secrets: Vec<Secret>) -> Self {
        Self {
            secrets: Some(secrets),
            ..self
        }
    }

    pub fn set_keys(self, keys: Vec<RuntimeKey>) -> Self {
        Self {
            keys: Some(keys),
            ..self
        }
    }

    pub fn set_private_key_paths(self, paths: Vec<PathBuf>) -> Self {
        Self {
            private_key_paths: Some(paths),
            ..self
        }
    }

    pub async fn build<I>(
        self,
        imp: I,
    ) -> SecretManager<<I as IntoSecretBackingImpl>::Error, <I as IntoSecretBackingImpl>::Impl>
    where
        I: IntoSecretBackingImpl + 'static,
        <I as IntoSecretBackingImpl>::Error: 'static,
        <I as IntoSecretBackingImpl>::Impl: 'static,
        ProcessRunningError: From<<I as IntoSecretBackingImpl>::Error>
    {
        let private_key_paths = self.private_key_paths.unwrap_or_else(|| {
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
        });

        let backing = imp.build().await;
        SecretManager::new(
            self.secret_root.unwrap(),
            self.owner_user.unwrap(),
            self.owner_group.unwrap(),
            self.secrets.unwrap(),
            self.keys.unwrap(),
            private_key_paths,
            backing,
        )
    }
}
