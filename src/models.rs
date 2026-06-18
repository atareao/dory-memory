use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct DoryMemoryNode {
    pub id: uuid::Uuid,
    pub namespace: String,
    pub content_l0: String,
    pub content_l1: Option<String>,
    pub content_l2: Option<String>,
    pub embedding: pgvector::Vector,
    pub importance: f32,
    pub is_immortal: bool,
    pub tags: serde_json::Value,
    pub deferred_until: Option<DateTime<Utc>>,
    pub last_accessed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DoryMemoryResponse {
    pub id: uuid::Uuid,
    pub namespace: String,
    pub content_l0: String,
    pub content_l1: Option<String>,
    pub content_l2: Option<String>,
    pub embedding: Vec<f32>,
    pub importance: f32,
    pub is_immortal: bool,
    pub tags: serde_json::Value,
    pub deferred_until: Option<DateTime<Utc>>,
    pub last_accessed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<DoryMemoryNode> for DoryMemoryResponse {
    fn from(n: DoryMemoryNode) -> Self {
        Self {
            id: n.id,
            namespace: n.namespace,
            content_l0: n.content_l0,
            content_l1: n.content_l1,
            content_l2: n.content_l2,
            embedding: n.embedding.to_vec(),
            importance: n.importance,
            is_immortal: n.is_immortal,
            tags: n.tags,
            deferred_until: n.deferred_until,
            last_accessed_at: n.last_accessed_at,
            created_at: n.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DoryIntent {
    Correction,
    Environment,
    Preference,
    Task,
    Backlog,
    Reference,
    Pivot,
    Standard,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DoryInsertPayload {
    pub namespace: String,
    pub content_l0: String,
    pub content_l1: Option<String>,
    pub content_l2: Option<String>,
    pub embedding: Vec<f32>,
    pub is_immortal: bool,
    pub tags: Vec<String>,
    pub deferred_until: Option<DateTime<Utc>>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DoryNamespace {
    pub name: String,
    pub embedding_model: String,
    pub dimensions: i32,
    pub created_at: DateTime<Utc>,
}
