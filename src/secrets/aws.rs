use aws_config::meta::region::RegionProviderChain;
use aws_sdk_secretsmanager::{
    operation::create_secret::CreateSecretOutput,
    types::{Filter, FilterNameStringType},
    Client,
};
use serde::{Deserialize, Serialize};

use crate::Configuration;

use super::{VaultError, VaultKind, VaultTrait};

#[derive(thiserror::Error, Debug)]
pub enum AwsError {
    #[error(transparent)]
    Secret(#[from] aws_sdk_secretsmanager::Error),
    #[error(transparent)]
    Encoding(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AwsSecretInfo {
    id: String,
    name: String,
    version: String,
}

#[derive(Debug)]
pub struct AwsSecretVault {
    client: Client,
    secret_info: AwsSecretInfo,
    secret_value: Configuration<String>,
}

#[async_trait::async_trait]
impl VaultTrait for AwsSecretVault {
    fn get(&self) -> &Configuration<String> {
        &self.secret_value
    }

    fn get_mut(&mut self) -> &mut Configuration<String> {
        &mut self.secret_value
    }

    fn into_vault_kind(&self) -> VaultKind {
        VaultKind::AwsSecretManager(self.secret_info.clone())
    }
    async fn save(&mut self) -> Result<(), VaultError> {
        self.save_secret().await?;
        Ok(())
    }
}

impl AwsSecretVault {
    pub async fn create(secret_name: String) -> Result<Self, AwsError> {
        let client = Self::make_client().await;
        let (info, secret_value) =
            if let Some(data) = Self::get_secret_by_name(&client, &secret_name).await? {
                if data.1.is_empty() {
                    (data.0, Configuration::new())
                } else {
                    (data.0, serde_json::from_str(&data.1)?)
                }
            } else {
                let secret = Self::create_secret(&client, &secret_name).await?;
                let arn = secret.arn().unwrap().to_string();
                let info = AwsSecretInfo {
                    id: arn,
                    name: secret_name,
                    version: secret.version_id().unwrap_or_default().to_string(),
                };
                (info, Configuration::new())
            };

        let mut res = Self {
            client,
            secret_info: info,
            secret_value,
        };
        res.save_secret().await?;
        Ok(res)
    }

    async fn save_secret(&mut self) -> Result<(), AwsError> {
        let writer = serde_json::to_string_pretty(&self.secret_value)?;
        self.secret_info.version = self.update_secret(writer).await?;
        Ok(())
    }
    pub async fn from_info(info: &AwsSecretInfo) -> Result<Self, AwsError> {
        Self::from_secret_arn(&info.id).await
    }

    async fn from_secret_arn(secret_arn: &str) -> Result<Self, AwsError> {
        let client = Self::make_client().await;
        let (secret_info, secret_string) = Self::get_secret_by_arn(&client, secret_arn).await?;
        let secret_value = if secret_string.is_empty() {
            Configuration::new()
        } else {
            serde_json::from_str(&secret_string)?
        };
        Ok(Self {
            client,
            secret_info,
            secret_value,
        })
    }

    pub fn secret_id(&self) -> &str {
        &self.secret_info.id
    }

    pub fn secret_name(&self) -> &str {
        &self.secret_info.name
    }

    async fn make_client() -> Client {
        let region_provider = RegionProviderChain::default_provider().or_else("eu-west-1");
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region_provider)
            .load()
            .await;
        Client::new(&config)
    }

    async fn create_secret(
        client: &Client,
        secret_name: impl Into<String>,
    ) -> Result<CreateSecretOutput, aws_sdk_secretsmanager::Error> {
        let value = client.create_secret().name(secret_name).send().await?;
        Ok(value)
    }

    async fn get_secret_by_name(
        client: &Client,
        name: &str,
    ) -> Result<Option<(AwsSecretInfo, String)>, aws_sdk_secretsmanager::Error> {
        let secret = client
            .list_secrets()
            .filters(
                Filter::builder()
                    .key(FilterNameStringType::Name)
                    .values(name.to_string())
                    .build(),
            )
            .send()
            .await?;

        let secret = secret
            .secret_list()
            .into_iter()
            .find(|v| v.name.as_ref().is_some_and(|v| v == name));
        let Some(arn) = secret.and_then(|s| s.arn()) else {
            return Ok(None);
        };
        let res = Self::get_secret_by_arn(client, arn).await?;
        Ok(Some(res))
    }

    async fn get_secret_by_arn(
        client: &Client,
        arn: &str,
    ) -> Result<(AwsSecretInfo, String), aws_sdk_secretsmanager::Error> {
        let secret = client.get_secret_value().secret_id(arn).send().await?;
        let info = AwsSecretInfo {
            id: arn.to_string(),
            name: secret.name().unwrap().to_string(),
            version: secret.version_id().unwrap_or_default().to_string(),
        };
        let value = secret.secret_string().map(|v| v.to_string());
        Ok((info, value.unwrap_or_default()))
    }

    async fn update_secret(
        &mut self,
        data: String,
    ) -> Result<String, aws_sdk_secretsmanager::Error> {
        let response = self
            .client
            .update_secret()
            .secret_id(self.secret_id())
            .secret_string(data)
            .send()
            .await?;
        Ok(response.version_id().unwrap_or_default().to_string())
    }
}
