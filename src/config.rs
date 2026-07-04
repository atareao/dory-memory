#[derive(Debug, Clone)]
pub struct DoryConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub embedding: EmbeddingConfig,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub dimensions: usize,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_required(name: &str) -> anyhow::Result<String> {
    std::env::var(name)
        .map_err(|_| anyhow::anyhow!("Required environment variable {name} is not set"))
}

impl DoryConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database: DatabaseConfig {
                url: env_required("DORY_DATABASE_URL")?,
                max_connections: env_or("DORY_DATABASE_MAX_CONNECTIONS", "10")
                    .parse()
                    .unwrap_or(10),
            },
            server: ServerConfig {
                host: env_or("DORY_SERVER_HOST", "0.0.0.0"),
                port: env_or("DORY_SERVER_PORT", "5005")
                    .parse()
                    .unwrap_or(5005),
            },
            embedding: EmbeddingConfig {
                api_url: env_required("DORY_EMBEDDING_API_URL")?,
                api_key: env_required("DORY_EMBEDDING_API_KEY")?,
                model: env_or("DORY_EMBEDDING_MODEL", "BAAI/bge-m3"),
                dimensions: env_or("DORY_EMBEDDING_DIMENSIONS", "1024")
                    .parse()
                    .unwrap_or(1024),
            },
        })
    }
}