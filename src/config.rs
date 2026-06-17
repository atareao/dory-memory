use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct DoryConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub embedding: EmbeddingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingConfig {
    pub api_url: String,
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

fn default_max_connections() -> u32 {
    10
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    5005
}

fn default_model() -> String {
    "text-embedding-ada-002".to_string()
}

fn default_dimensions() -> usize {
    1536
}

impl DoryConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to read config at {}: {e}", path.as_ref().display()))?;
        let mut config: DoryConfig = toml::from_str(&content)?;

        if let Ok(key) = std::env::var("DORY_EMBEDDING_API_KEY") {
            config.embedding.api_key = key;
        }

        Ok(config)
    }
}