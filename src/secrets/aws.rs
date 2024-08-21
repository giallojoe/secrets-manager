use aws_config::meta::region::RegionProviderChain;
use aws_sdk_secretsmanager::{
    operation::{create_secret::CreateSecretOutput, get_secret_value::GetSecretValueOutput},
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

fn default_profile() -> String {
    String::from("default")
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AwsSecretInfo {
    id: String,
    name: String,
    version: String,
    #[serde(default = "default_profile")]
    profile_name: String,
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
    pub async fn create(secret_name: String, profile_name: String) -> Result<Self, AwsError> {
        let client = Self::make_client(&profile_name).await;
        let (info, secret_value) =
            if let Some(arn) = Self::get_secret_by_name(&client, &secret_name).await? {
                let secret = Self::get_secret_by_arn(&client, &arn).await?;
                let info = AwsSecretInfo {
                    id: arn,
                    name: secret_name,
                    profile_name,
                    version: secret.version_id().unwrap_or_default().to_string(),
                };
                if let Some(value_raw) = secret.secret_string() {
                    (info, serde_json::from_str(value_raw)?)
                } else {
                    (info, Configuration::new())
                }
            } else {
                let secret = Self::create_secret(&client, &secret_name).await?;
                let arn = secret.arn().unwrap().to_string();
                let info = AwsSecretInfo {
                    id: arn,
                    name: secret_name,
                    profile_name,
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
        let client = Self::make_client(&info.profile_name).await;
        let value = Self::from_secret_arn(&client, &info.id).await?;
        Ok(Self {
            client,
            secret_info: info.clone(),
            secret_value: value,
        })
    }

    async fn from_secret_arn(
        client: &Client,
        secret_arn: &str,
    ) -> Result<Configuration<String>, AwsError> {
        let secret = Self::get_secret_by_arn(client, secret_arn).await?;
        let secret_value = if let Some(secret_str) = secret.secret_string() {
            serde_json::from_str(secret_str)?
        } else {
            Configuration::new()
        };
        Ok(secret_value)
    }

    pub fn secret_id(&self) -> &str {
        &self.secret_info.id
    }

    pub fn secret_name(&self) -> &str {
        &self.secret_info.name
    }

    async fn make_client(profile_name: &str) -> Client {
        let region_provider = RegionProviderChain::default_provider().or_else("eu-west-1");
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .profile_name(profile_name)
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
    ) -> Result<Option<String>, aws_sdk_secretsmanager::Error> {
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
            .secret_list
            .unwrap_or_default()
            .into_iter()
            .find(|v| v.name.as_ref().is_some_and(|v| v == name));
        Ok(secret.and_then(|v| v.arn().map(|v| v.to_string())))
    }

    async fn get_secret_by_arn(
        client: &Client,
        arn: &str,
    ) -> Result<GetSecretValueOutput, aws_sdk_secretsmanager::Error> {
        let secret = client.get_secret_value().secret_id(arn).send().await?;
        Ok(secret)
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
