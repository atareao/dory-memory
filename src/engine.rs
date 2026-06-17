use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::cache::DoryCacheManager;
use crate::models::{DoryInsertPayload, DoryIntent, DoryMemoryNode, TimeWindow};

#[derive(serde::Serialize)]
pub struct DoryStats {
    pub total_memories: i64,
    pub immortal_count: i64,
    pub namespace_count: i64,
    pub namespaces: Vec<NamespaceStat>,
    pub avg_importance: f64,
    pub low_importance_count: i64,
    pub oldest_memory: Option<DateTime<Utc>>,
    pub newest_memory: Option<DateTime<Utc>>,
}

#[derive(serde::Serialize)]
pub struct NamespaceStat {
    pub name: String,
    pub count: i64,
}

pub struct DoryEngine {
    pool: PgPool,
}

impl DoryEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn process_and_route_memory(
        &self,
        cache_manager: &DoryCacheManager,
        mut payload: DoryInsertPayload,
        intent: DoryIntent,
    ) -> Result<(), sqlx::Error> {
        match intent {
            DoryIntent::Correction => {
                let mut sandbox = cache_manager.get_sandbox_lock().await;
                if let Some(last_inserted) = sandbox.back_mut() {
                    last_inserted.content_l0 = payload.content_l0;
                    last_inserted.content_l1 = payload.content_l1;
                    last_inserted.content_l2 = payload.content_l2;
                    return Ok(());
                }
                self.commit_memory(payload).await?;
            }
            DoryIntent::Environment => {
                payload.is_immortal = true;
                payload.tags.push("type:environment".to_string());
                payload.tags.push("scope:global".to_string());
                self.commit_memory(payload).await?;
            }
            DoryIntent::Preference => {
                payload.tags.push("type:preference".to_string());
                payload.tags.push("scope:habit".to_string());
                self.commit_memory(payload).await?;
            }
            DoryIntent::Backlog => {
                payload.tags.push("status:backlog".to_string());
                payload.tags.push("type:idea".to_string());
                self.commit_memory(payload).await?;
            }
            DoryIntent::Reference => {
                payload.is_immortal = true;
                payload.tags.push("type:reference".to_string());
                payload.tags.push("scope:documentation".to_string());
                self.commit_memory(payload).await?;
            }
            DoryIntent::Pivot => {
                cache_manager.flush_sandbox_to_db(self).await?;
                cache_manager.stage_ephemeral_memory(payload).await;
            }
            DoryIntent::Task | DoryIntent::Standard => {
                cache_manager.stage_ephemeral_memory(payload).await;
            }
        }
        Ok(())
    }

    pub async fn commit_memory(&self, payload: DoryInsertPayload) -> Result<Uuid, sqlx::Error> {
        let vector_wrapper = pgvector::Vector::from(payload.embedding);
        let tags_json = serde_json::to_value(&payload.tags).unwrap_or_default();

        let row = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO dory_memories (namespace, content_l0, content_l1, content_l2, embedding, is_immortal, tags, deferred_until)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
        )
        .bind(&payload.namespace)
        .bind(&payload.content_l0)
        .bind(&payload.content_l1)
        .bind(&payload.content_l2)
        .bind(vector_wrapper)
        .bind(payload.is_immortal)
        .bind(tags_json)
        .bind(payload.deferred_until)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn hybrid_recall(
        &self,
        namespace: &str,
        query_text: &str,
        query_embedding: Vec<f32>,
        required_tags: Vec<String>,
        candidate_limit: i32,
    ) -> Result<Vec<DoryMemoryNode>, sqlx::Error> {
        let vector_wrapper = pgvector::Vector::from(query_embedding);
        let tags_filter = serde_json::to_value(&required_tags).unwrap_or_default();

        let records = sqlx::query_as::<_, DoryMemoryNode>(
            r#"
            WITH vector_matches AS (
                SELECT id, ROW_NUMBER() OVER (ORDER BY embedding <=> $1) AS rank
                FROM dory_memories
                WHERE namespace = $2 AND tags @> $3::jsonb
                LIMIT 50
            ),
            text_matches AS (
                SELECT id, ROW_NUMBER() OVER (ORDER BY ts_rank(fts_tokens, plainto_tsquery('english', $4)) DESC) AS rank
                FROM dory_memories
                WHERE namespace = $2 AND fts_tokens @@ plainto_tsquery('english', $4) AND tags @> $3::jsonb
                LIMIT 50
            )
            SELECT m.id, m.namespace, m.content_l0, m.content_l1, m.content_l2,
                   m.embedding, m.importance, m.is_immortal, m.tags, m.deferred_until, m.last_accessed_at, m.created_at
            FROM dory_memories m
            JOIN (
                SELECT COALESCE(v.id, t.id) AS id,
                       (1.0 / (60 + COALESCE(v.rank, 10000)) + 1.0 / (60 + COALESCE(t.rank, 10000))) AS rrf_score
                FROM vector_matches v
                FULL OUTER JOIN text_matches t ON v.id = t.id
            ) scores ON m.id = scores.id
            ORDER BY scores.rrf_score DESC
            LIMIT $5
            "#,
        )
        .bind(vector_wrapper)
        .bind(namespace)
        .bind(tags_filter)
        .bind(query_text)
        .bind(candidate_limit)
        .fetch_all(&self.pool)
        .await?;

        let ids: Vec<Uuid> = records.iter().map(|r| r.id).collect();
        if !ids.is_empty() {
            sqlx::query(
                "UPDATE dory_memories SET last_accessed_at = NOW(), importance = LEAST(importance + 0.20, 1.0) WHERE id = ANY($1)",
            )
            .bind(&ids)
            .execute(&self.pool)
            .await?;
        }

        Ok(records)
    }

    pub async fn temporal_recall(
        &self,
        namespace: &str,
        time_window: Option<TimeWindow>,
        required_tags: Vec<String>,
        limit: i32,
    ) -> Result<Vec<DoryMemoryNode>, sqlx::Error> {
        let tags_filter = serde_json::to_value(&required_tags).unwrap_or_default();

        match time_window {
            Some(window) => {
                sqlx::query_as::<_, DoryMemoryNode>(
                    r#"
                    SELECT id, namespace, content_l0, content_l1, content_l2, embedding, importance, is_immortal, tags, deferred_until, last_accessed_at, created_at
                    FROM dory_memories
                    WHERE namespace = $1 AND tags @> $2::jsonb AND created_at >= $3 AND created_at <= $4
                    ORDER BY created_at DESC
                    LIMIT $5
                    "#,
                )
                .bind(namespace)
                .bind(tags_filter)
                .bind(window.start)
                .bind(window.end)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, DoryMemoryNode>(
                    r#"
                    SELECT id, namespace, content_l0, content_l1, content_l2, embedding, importance, is_immortal, tags, deferred_until, last_accessed_at, created_at
                    FROM dory_memories
                    WHERE namespace = $1 AND tags @> $2::jsonb AND (deferred_until IS NULL OR deferred_until <= NOW())
                    ORDER BY importance DESC, created_at DESC
                    LIMIT $3
                    "#,
                )
                .bind(namespace)
                .bind(tags_filter)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
    }

    pub async fn recall_within_token_budget(
        &self,
        namespace: &str,
        query_text: &str,
        query_vector: Vec<f32>,
        tags: Vec<String>,
        time_window: Option<TimeWindow>,
        max_token_budget: usize,
    ) -> Result<Vec<DoryMemoryNode>, sqlx::Error> {
        let candidates = if time_window.is_some() || (query_text.is_empty() && !tags.is_empty()) {
            self.temporal_recall(namespace, time_window, tags, 20).await?
        } else {
            self.hybrid_recall(namespace, query_text, query_vector, tags, 20).await?
        };

        let mut selected_nodes = Vec::new();
        let mut current_tokens = 0;
        let bpe = tiktoken_rs::cl100k_base().expect("Failed to load BPE tokenizer");

        for node in candidates {
            let node_tokens = bpe.encode_with_special_tokens(&node.content_l0).len();
            if current_tokens + node_tokens > max_token_budget {
                break;
            }
            current_tokens += node_tokens;
            selected_nodes.push(node);
        }
        Ok(selected_nodes)
    }

    pub async fn proactive_horizon_sweep(
        &self,
        namespace: &str,
    ) -> Result<Vec<DoryMemoryNode>, sqlx::Error> {
        sqlx::query_as::<_, DoryMemoryNode>(
            r#"
            SELECT id, namespace, content_l0, content_l1, content_l2, embedding, importance, is_immortal, tags, deferred_until, last_accessed_at, created_at
            FROM dory_memories
            WHERE namespace = $1
              AND tags @> '["status:todo"]'::jsonb
              AND (deferred_until IS NULL OR deferred_until <= NOW())
              AND last_accessed_at < NOW() - INTERVAL '24 hours'
            ORDER BY importance DESC
            LIMIT 5
            "#,
        )
        .bind(namespace)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_namespace_dimensions(&self, name: &str) -> Result<Option<i32>, sqlx::Error> {
        let row = sqlx::query_scalar::<_, i32>(
            "SELECT dimensions FROM dory_namespaces WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn ensure_namespace(
        &self,
        name: &str,
        model: &str,
        dimensions: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO dory_namespaces (name, embedding_model, dimensions)
            VALUES ($1, $2, $3)
            ON CONFLICT (name) DO NOTHING
            "#,
        )
        .bind(name)
        .bind(model)
        .bind(dimensions)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_memory(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM dory_memories WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_stale(
        &self,
        namespace: Option<&str>,
        older_than_hours: i64,
        importance_below: f32,
        limit: i32,
    ) -> Result<Vec<DoryMemoryNode>, sqlx::Error> {
        match namespace {
            Some(ns) => {
                sqlx::query_as::<_, DoryMemoryNode>(
                    r#"
                    SELECT id, namespace, content_l0, content_l1, content_l2, embedding, importance, is_immortal, tags, deferred_until, last_accessed_at, created_at
                    FROM dory_memories
                    WHERE namespace = $1
                      AND is_immortal = FALSE
                      AND last_accessed_at < NOW() - ($2 || ' hours')::INTERVAL
                      AND importance < $3
                    ORDER BY last_accessed_at ASC
                    LIMIT $4
                    "#,
                )
                .bind(ns)
                .bind(older_than_hours.to_string())
                .bind(importance_below)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, DoryMemoryNode>(
                    r#"
                    SELECT id, namespace, content_l0, content_l1, content_l2, embedding, importance, is_immortal, tags, deferred_until, last_accessed_at, created_at
                    FROM dory_memories
                    WHERE is_immortal = FALSE
                      AND last_accessed_at < NOW() - ($1 || ' hours')::INTERVAL
                      AND importance < $2
                    ORDER BY last_accessed_at ASC
                    LIMIT $3
                    "#,
                )
                .bind(older_than_hours.to_string())
                .bind(importance_below)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
    }

    pub async fn batch_delete(
        &self,
        namespace: Option<&str>,
        older_than_hours: Option<i64>,
        importance_below: Option<f32>,
        tags_required: Vec<String>,
    ) -> Result<u64, sqlx::Error> {
        let tags_filter = serde_json::to_value(&tags_required).unwrap_or_default();
        let has_tags = !tags_required.is_empty();

        if let Some(ns) = namespace {
            if let Some(hours) = older_than_hours
                && let Some(imp) = importance_below
            {
                let result = sqlx::query(
                    r#"
                    DELETE FROM dory_memories
                    WHERE namespace = $1
                      AND is_immortal = FALSE
                      AND last_accessed_at < NOW() - ($2 || ' hours')::INTERVAL
                      AND importance < $3
                      AND ($4::bool = FALSE OR tags @> $5::jsonb)
                    "#,
                )
                .bind(ns)
                .bind(hours.to_string())
                .bind(imp)
                .bind(has_tags)
                .bind(tags_filter)
                .execute(&self.pool)
                .await?;
                return Ok(result.rows_affected());
            }
            let result = sqlx::query(
                r#"
                DELETE FROM dory_memories
                WHERE namespace = $1
                  AND is_immortal = FALSE
                  AND ($2::bool = FALSE OR tags @> $3::jsonb)
                "#,
            )
            .bind(ns)
            .bind(has_tags)
            .bind(tags_filter)
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        } else {
            if let Some(hours) = older_than_hours
                && let Some(imp) = importance_below
            {
                let result = sqlx::query(
                    r#"
                    DELETE FROM dory_memories
                    WHERE is_immortal = FALSE
                      AND last_accessed_at < NOW() - ($1 || ' hours')::INTERVAL
                      AND importance < $2
                      AND ($3::bool = FALSE OR tags @> $4::jsonb)
                    "#,
                )
                .bind(hours.to_string())
                .bind(imp)
                .bind(has_tags)
                .bind(tags_filter)
                .execute(&self.pool)
                .await?;
                return Ok(result.rows_affected());
            }
            let result = sqlx::query(
                r#"
                DELETE FROM dory_memories
                WHERE is_immortal = FALSE
                  AND ($1::bool = FALSE OR tags @> $2::jsonb)
                "#,
            )
            .bind(has_tags)
            .bind(tags_filter)
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        }
    }

    pub async fn get_stats(&self) -> Result<DoryStats, sqlx::Error> {
        let total: (Option<i64>,) = sqlx::query_as("SELECT COUNT(*) FROM dory_memories")
            .fetch_one(&self.pool).await?;

        let immortal: (Option<i64>,) = sqlx::query_as("SELECT COUNT(*) FROM dory_memories WHERE is_immortal = TRUE")
            .fetch_one(&self.pool).await?;

        let ns_count: (Option<i64>,) = sqlx::query_as("SELECT COUNT(*) FROM dory_namespaces")
            .fetch_one(&self.pool).await?;

        let ns_rows: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT n.name, COUNT(m.id)::bigint FROM dory_namespaces n LEFT JOIN dory_memories m ON m.namespace = n.name GROUP BY n.name ORDER BY count DESC",
        )
        .fetch_all(&self.pool).await?;

        let avg_imp: (Option<f64>,) = sqlx::query_as("SELECT AVG(importance::numeric)::float8 FROM dory_memories")
            .fetch_one(&self.pool).await?;

        let low_imp: (Option<i64>,) = sqlx::query_as("SELECT COUNT(*) FROM dory_memories WHERE importance < 0.15 AND is_immortal = FALSE")
            .fetch_one(&self.pool).await?;

        let oldest: (Option<DateTime<Utc>>,) = sqlx::query_as("SELECT MIN(created_at) FROM dory_memories")
            .fetch_one(&self.pool).await?;

        let newest: (Option<DateTime<Utc>>,) = sqlx::query_as("SELECT MAX(created_at) FROM dory_memories")
            .fetch_one(&self.pool).await?;

        Ok(DoryStats {
            total_memories: total.0.unwrap_or(0),
            immortal_count: immortal.0.unwrap_or(0),
            namespace_count: ns_count.0.unwrap_or(0),
            namespaces: ns_rows.into_iter().map(|(name, count)| NamespaceStat { name, count: count.unwrap_or(0) }).collect(),
            avg_importance: avg_imp.0.unwrap_or(0.0),
            low_importance_count: low_imp.0.unwrap_or(0),
            oldest_memory: oldest.0,
            newest_memory: newest.0,
        })
    }

    pub fn pool_ref(&self) -> &PgPool {
        &self.pool
    }
}