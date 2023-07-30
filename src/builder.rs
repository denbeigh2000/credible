use std::path::PathBuf;

use crate::secret::ProcessRunningError;
use crate::{IntoSecretStorage, Secret, SecretManager};

#[derive(Default)]
pub struct SecretManagerBuilder {
    secrets: Option<Vec<Secret>>,
    private_key_paths: Option<Vec<PathBuf>>,
}

impl SecretManagerBuilder {
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
    ) -> SecretManager<<I as IntoSecretStorage>::Error, <I as IntoSecretStorage>::Impl>
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
        SecretManager::new(self.secrets.unwrap(), private_key_paths, backing)
    }
}
