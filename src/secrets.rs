use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

use aws_config::meta::region::RegionProviderChain;
use aws_sdk_secretsmanager::{
    error::SdkError,
    operation::{create_secret::CreateSecretOutput, get_secret_value::GetSecretValueError},
    types::{Filter, FilterNameStringType},
    Client,
};
use serde::{Deserialize, Serialize};

use crate::{ConfigValue, Configuration};

type Error = Box<dyn std::error::Error>;

#[derive(Serialize, Deserialize)]
struct SecretInfo {
    id: String,
    name: String,
    version: String,
}

pub struct SecretManager {
    client: Client,
    secret_info: SecretInfo,
    secret_value: Configuration<String>,
}

impl SecretManager {
    pub async fn create(secret_name: String, dest: impl AsRef<Path>) -> Result<Self, Error> {
        let client = Self::make_client().await;
        let (info, secret_value) =
            if let Some(data) = Self::get_secret_by_name(&client, &secret_name).await? {
                if data.1.is_empty() {
                    (data.0, Configuration::new(PathBuf::from("/")))
                } else {
                    (
                        data.0,
                        Configuration::from_str(&data.1, PathBuf::from("/"))?,
                    )
                }
            } else {
                let secret = Self::create_secret(&client, &secret_name).await?;
                let arn = secret.arn().unwrap().to_string();
                let info = SecretInfo {
                    id: arn,
                    name: secret_name,
                    version: secret.version_id().unwrap_or_default().to_string(),
                };
                (info, Configuration::new(PathBuf::from("/")))
            };

        let contents = serde_json::to_string(&info)?;
        tokio::fs::create_dir_all(&dest).await?;
        tokio::fs::write(dest, contents).await?;
        let mut res = Self {
            client,
            secret_info: info,
            secret_value,
        };
        res.save().await?;
        Ok(res)
    }

    pub async fn from_secret_arn(secret_arn: &str, cwd: PathBuf) -> Result<Self, Error> {
        let client = Self::make_client().await;
        let (secret_info, secret_string) = Self::get_secret_by_arn(&client, secret_arn).await?;
        let secret_value = if secret_string.is_empty() {
            Configuration::new(cwd)
        } else {
            Configuration::from_str(&secret_string, cwd)?
        };
        Ok(Self {
            client,
            secret_info,
            secret_value,
        })
    }

    pub async fn from_config(config_path: impl AsRef<Path>, path: PathBuf) -> Result<Self, Error> {
        let client = Self::make_client().await;
        let secret_info: SecretInfo = serde_json::from_reader(File::open(&config_path)?)?;
        let (_, secret_string) = Self::get_secret_by_arn(&client, &secret_info.id).await?;
        let secret_value = if secret_string.is_empty() {
            Configuration::new(path)
        } else {
            Configuration::from_str(&secret_string, path)?
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

    pub fn get_value(&self, key: &str) -> Option<&String> {
        self.secret_value.get_value(key)
    }

    pub fn get_values_for_cwd(&self) -> HashMap<&String, &String> {
        self.secret_value.get_values_for_cwd()
    }

    pub fn print_tree(&self) -> String {
        self.secret_value.print_tree()
    }

    pub fn set_value(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Option<String> {
        self.secret_value.set_value(key, value)
    }

    pub fn remove_value(&mut self, key: &str) -> Option<String> {
        self.secret_value.remove_value(key)
    }

    pub fn resolve_value<'a>(&'a self, value: &'a ConfigValue) -> Option<&'a String> {
        match value {
            ConfigValue::Secret { path, key } => self.secret_value.get_value_at(path, key),
            ConfigValue::Value(v) => Some(v),
        }
    }

    pub fn resolve<'a>(
        &'a self,
        config: &'a Configuration<ConfigValue>,
    ) -> HashMap<&'a String, &'a String> {
        let data = config.get_values_for_cwd();
        data.into_iter()
            .map(|(key, value)| {
                (
                    key,
                    self.resolve_value(value).unwrap_or_else(|| match value {
                        ConfigValue::Secret { key, .. } => key,
                        ConfigValue::Value(v) => v,
                    }),
                )
            })
            .collect()
    }

    pub async fn save(&mut self) -> Result<(), Error> {
        let mut writer = String::new();
        self.secret_value.write(&mut writer)?;
        let response = self
            .client
            .update_secret()
            .secret_id(self.secret_id())
            .secret_string(writer)
            .send()
            .await?;
        self.secret_info.version = response.version_id().unwrap().to_string();
        Ok(())
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
    ) -> Result<CreateSecretOutput, Error> {
        let value = client.create_secret().name(secret_name).send().await?;
        Ok(value)
    }

    async fn get_secret_by_name(
        client: &Client,
        name: &str,
    ) -> Result<Option<(SecretInfo, String)>, Error> {
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
    ) -> Result<(SecretInfo, String), SdkError<GetSecretValueError>> {
        let secret = client.get_secret_value().secret_id(arn).send().await?;
        let info = SecretInfo {
            id: arn.to_string(),
            name: secret.name().unwrap().to_string(),
            version: secret.version_id().unwrap_or_default().to_string(),
        };
        let value = secret.secret_string().map(|v| v.to_string());
        Ok((info, value.unwrap_or_default()))
    }
}
