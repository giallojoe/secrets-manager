mod aws;

use serde::{Deserialize, Serialize};

use aws::AwsSecretInfo;
pub use aws::AwsSecretVault;

use crate::Configuration;

use self::aws::AwsError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "provider")]
pub enum VaultKind {
    AwsSecretManager(AwsSecretInfo),
}

impl VaultKind {
    pub async fn into_vault(self) -> Result<Box<dyn VaultTrait>, VaultError> {
        match self {
            Self::AwsSecretManager(info) => Ok(Box::new(AwsSecretVault::from_info(&info).await?)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error(transparent)]
    Aws(#[from] AwsError),
}

#[async_trait::async_trait]
pub trait VaultTrait {
    fn get(&self) -> &Configuration<String>;
    fn get_mut(&mut self) -> &mut Configuration<String>;
    async fn save(&mut self) -> Result<(), VaultError>;
    fn into_vault_kind(&self) -> VaultKind;
}
