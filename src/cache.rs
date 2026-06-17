use std::collections::VecDeque;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{Mutex, MutexGuard};

use crate::models::DoryInsertPayload;

pub struct DoryCacheManager {
    embedding_cache: DashMap<String, Vec<f32>>,
    session_sandbox: Arc<Mutex<VecDeque<DoryInsertPayload>>>,
}

impl DoryCacheManager {
    pub fn new() -> Self {
        Self {
            embedding_cache: DashMap::new(),
            session_sandbox: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn get_cached_embedding(&self, text: &str) -> Option<Vec<f32>> {
        self.embedding_cache.get(text).map(|v| v.clone())
    }

    pub fn insert_embedding(&self, text: &str, embedding: Vec<f32>) {
        self.embedding_cache.insert(text.to_string(), embedding);
    }

    pub async fn get_sandbox_lock(&self) -> MutexGuard<'_, VecDeque<DoryInsertPayload>> {
        self.session_sandbox.lock().await
    }

    pub async fn stage_ephemeral_memory(&self, payload: DoryInsertPayload) {
        let mut sandbox = self.session_sandbox.lock().await;
        sandbox.push_back(payload);
        if sandbox.len() > 50 {
            sandbox.pop_front();
        }
    }

    #[allow(dead_code)]
    pub async fn proactive_prefetch_workspace(
        &self,
        engine: &crate::engine::DoryEngine,
        namespace: &str,
        dimensions: usize,
        tags: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        let dummy_vector = vec![0.0; dimensions];
        if let Ok(nodes) = engine
            .hybrid_recall(namespace, "", dummy_vector, tags, 5)
            .await
        {
            for node in nodes {
                self.insert_embedding(&node.content_l0, vec![0.0; dimensions]);
            }
        }
        Ok(())
    }

    pub async fn flush_sandbox_to_db(&self, engine: &crate::engine::DoryEngine) -> Result<(), sqlx::Error> {
        let mut sandbox = self.session_sandbox.lock().await;
        while let Some(payload) = sandbox.pop_front() {
            engine.commit_memory(payload).await?;
        }
        Ok(())
    }
}