use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::cache::DoryCacheManager;
use crate::embed::EmbeddingClient;
use crate::engine::DoryEngine;
use crate::error::{DoryError, DoryResult};
use crate::guard;
use crate::models::{DoryInsertPayload, DoryIntent, DoryMemoryResponse, TimeWindow};

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<DoryEngine>,
    pub cache: Arc<DoryCacheManager>,
    pub embedder: Arc<EmbeddingClient>,
    pub default_dimensions: usize,
    pub embedding_model: String,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/memories", post(insert_memory))
        .route("/v1/memories/{id}", delete(delete_memory))
        .route("/v1/search", post(hybrid_search))
        .route("/v1/search/temporal", post(temporal_search))
        .route("/v1/search/budget", post(budget_search))
        .route("/v1/sweep/{namespace}", get(horizon_sweep))
        .route("/v1/namespaces", post(ensure_namespace))
        .route("/v1/maintenance/stale", post(list_stale))
        .route("/v1/batch/delete", post(batch_delete))
        .route("/v1/stats", get(get_stats))
        .with_state(state)
}

#[derive(Deserialize)]
pub struct InsertRequest {
    pub namespace: String,
    pub content_l0: String,
    #[serde(default)]
    pub content_l1: Option<String>,
    #[serde(default)]
    pub content_l2: Option<String>,
    pub intent: DoryIntent,
    pub is_immortal: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub deferred_until: Option<chrono::DateTime<chrono::Utc>>,
}

async fn insert_memory(
    State(state): State<AppState>,
    Json(req): Json<InsertRequest>,
) -> DoryResult<StatusCode> {
    let text = guard::sanitize_text(&req.content_l0)?;
    if let Some(ref l2) = req.content_l2 {
        guard::sanitize_text(l2)?;
    }

    let dimensions = state
        .engine
        .get_namespace_dimensions(&req.namespace)
        .await
        .map_err(DoryError::Database)?
        .unwrap_or(state.default_dimensions as i32) as usize;

    state
        .engine
        .ensure_namespace(&req.namespace, &state.embedding_model, dimensions as i32)
        .await
        .map_err(DoryError::Database)?;

    let embedding = state
        .embedder
        .generate_embedding(&text, &state.cache, dimensions)
        .await?;

    let payload = DoryInsertPayload {
        namespace: req.namespace,
        content_l0: text,
        content_l1: req.content_l1,
        content_l2: req.content_l2,
        embedding,
        is_immortal: req.is_immortal,
        tags: req.tags,
        deferred_until: req.deferred_until,
    };

    state
        .engine
        .process_and_route_memory(&state.cache, payload, req.intent)
        .await
        .map_err(DoryError::Database)?;

    Ok(StatusCode::CREATED)
}

async fn delete_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> DoryResult<StatusCode> {
    let deleted = state
        .engine
        .delete_memory(id)
        .await
        .map_err(DoryError::Database)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(DoryError::NotFound(format!("Memory {id} not found")))
    }
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub namespace: String,
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

fn default_limit() -> i32 {
    10
}

async fn hybrid_search(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> DoryResult<Json<Vec<DoryMemoryResponse>>> {
    let query_text = req.query_text;
    let query = req.query.unwrap_or_default();

    let dimensions = state
        .engine
        .get_namespace_dimensions(&req.namespace)
        .await
        .map_err(DoryError::Database)?
        .unwrap_or(state.default_dimensions as i32) as usize;

    let query_text_final = if query_text.is_empty() { query } else { query_text };
    let embedding = if query_text_final.is_empty() {
        vec![0.0; dimensions]
    } else {
        state
            .embedder
            .generate_embedding(&query_text_final, &state.cache, dimensions)
            .await?
    };

    let results: Vec<DoryMemoryResponse> = state
        .engine
        .hybrid_recall(&req.namespace, &query_text_final, embedding, req.tags, req.limit)
        .await
        .map_err(DoryError::Database)?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(results))
}

#[derive(Deserialize)]
pub struct TemporalSearchRequest {
    pub namespace: String,
    #[serde(default)]
    pub start: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub end: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

async fn temporal_search(
    State(state): State<AppState>,
    Json(req): Json<TemporalSearchRequest>,
) -> DoryResult<Json<Vec<DoryMemoryResponse>>> {
    let time_window = match (req.start, req.end) {
        (Some(start), Some(end)) => Some(TimeWindow { start, end }),
        _ => None,
    };

    let results: Vec<DoryMemoryResponse> = state
        .engine
        .temporal_recall(&req.namespace, time_window, req.tags, req.limit)
        .await
        .map_err(DoryError::Database)?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(results))
}

#[derive(Deserialize)]
pub struct BudgetSearchRequest {
    pub namespace: String,
    #[serde(default)]
    pub query_text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub start: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub end: Option<chrono::DateTime<chrono::Utc>>,
    pub max_tokens: usize,
}

async fn budget_search(
    State(state): State<AppState>,
    Json(req): Json<BudgetSearchRequest>,
) -> DoryResult<Json<Vec<DoryMemoryResponse>>> {
    let dimensions = state
        .engine
        .get_namespace_dimensions(&req.namespace)
        .await
        .map_err(DoryError::Database)?
        .unwrap_or(state.default_dimensions as i32) as usize;

    let time_window = match (req.start, req.end) {
        (Some(start), Some(end)) => Some(TimeWindow { start, end }),
        _ => None,
    };

    let query_vector = if req.query_text.is_empty() {
        vec![0.0; dimensions]
    } else {
        state
            .embedder
            .generate_embedding(&req.query_text, &state.cache, dimensions)
            .await?
    };

    let results: Vec<DoryMemoryResponse> = state
        .engine
        .recall_within_token_budget(
            &req.namespace,
            &req.query_text,
            query_vector,
            req.tags,
            time_window,
            req.max_tokens,
        )
        .await
        .map_err(DoryError::Database)?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(results))
}

async fn horizon_sweep(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
) -> DoryResult<Json<Vec<DoryMemoryResponse>>> {
    let results: Vec<DoryMemoryResponse> = state
        .engine
        .proactive_horizon_sweep(&namespace)
        .await
        .map_err(DoryError::Database)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(results))
}

#[derive(Deserialize)]
pub struct NamespaceRequest {
    pub name: String,
    pub model: String,
    pub dimensions: i32,
}

async fn ensure_namespace(
    State(state): State<AppState>,
    Json(req): Json<NamespaceRequest>,
) -> DoryResult<StatusCode> {
    state
        .engine
        .ensure_namespace(&req.name, &req.model, req.dimensions)
        .await
        .map_err(DoryError::Database)?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct StaleRequest {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default = "default_stale_hours")]
    pub older_than_hours: i64,
    #[serde(default = "default_importance_below")]
    pub importance_below: f32,
    #[serde(default = "default_stale_limit")]
    pub limit: i32,
}

fn default_stale_hours() -> i64 {
    72
}

fn default_importance_below() -> f32 {
    0.5
}

fn default_stale_limit() -> i32 {
    50
}

async fn list_stale(
    State(state): State<AppState>,
    Json(req): Json<StaleRequest>,
) -> DoryResult<Json<Vec<DoryMemoryResponse>>> {
    let results: Vec<DoryMemoryResponse> = state
        .engine
        .list_stale(req.namespace.as_deref(), req.older_than_hours, req.importance_below, req.limit)
        .await
        .map_err(DoryError::Database)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(results))
}

#[derive(Deserialize)]
pub struct BatchDeleteRequest {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub older_than_hours: Option<i64>,
    #[serde(default)]
    pub importance_below: Option<f32>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct BatchDeleteResponse {
    pub deleted: u64,
}

async fn batch_delete(
    State(state): State<AppState>,
    Json(req): Json<BatchDeleteRequest>,
) -> DoryResult<Json<BatchDeleteResponse>> {
    let deleted = state
        .engine
        .batch_delete(
            req.namespace.as_deref(),
            req.older_than_hours,
            req.importance_below,
            req.tags,
        )
        .await
        .map_err(DoryError::Database)?;
    Ok(Json(BatchDeleteResponse { deleted }))
}

async fn get_stats(
    State(state): State<AppState>,
) -> DoryResult<Json<serde_json::Value>> {
    let stats = state
        .engine
        .get_stats()
        .await
        .map_err(DoryError::Database)?;
    Ok(Json(serde_json::to_value(&stats).unwrap_or_default()))
}