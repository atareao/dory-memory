use crate::error::{DoryError, DoryResult};
use crate::cache::DoryCacheManager;

pub struct EmbeddingClient {
    client: reqwest::Client,
    api_url: String,
    api_key: String,
    model: String,
}

impl EmbeddingClient {
    pub fn new(api_url: String, api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_url,
            api_key,
            model,
        }
    }

    pub async fn generate_embedding(
        &self,
        text: &str,
        cache: &DoryCacheManager,
        dimensions: usize,
    ) -> DoryResult<Vec<f32>> {
        if let Some(cached) = cache.get_cached_embedding(text) {
            return Ok(cached);
        }

        let response = self
            .client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": self.model,
                "input": text,
                "dimensions": dimensions,
            }))
            .send()
            .await
            .map_err(|e| DoryError::Embedding(format!("Request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DoryError::Embedding(format!(
                "API returned {status}: {body}"
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| DoryError::Embedding(format!("Failed to parse response: {e}")))?;

        let embedding: Vec<f32> = body["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| DoryError::Embedding("No embedding in response".to_string()))?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();

        cache.insert_embedding(text, embedding.clone());
        Ok(embedding)
    }
}