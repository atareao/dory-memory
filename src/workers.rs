use std::sync::Arc;

use sqlx::PgPool;
use tokio::time::{interval, Duration};

use crate::engine::DoryEngine;

pub async fn start_decay_pruning_loop(pool: PgPool) {
    let mut timer = interval(Duration::from_secs(86400));
    loop {
        timer.tick().await;
        tracing::info!("Decay/pruning loop: starting");
        if let Err(e) = run_decay_pruning(&pool).await {
            tracing::error!("Decay/pruning error: {e}");
        }
    }
}

async fn run_decay_pruning(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE dory_memories
        SET importance = importance * 0.90
        WHERE is_immortal = FALSE
          AND last_accessed_at < NOW() - INTERVAL '3 days'
        "#,
    )
    .execute(pool)
    .await?;

    let deleted = sqlx::query(
        r#"
        DELETE FROM dory_memories
        WHERE importance < 0.15 AND is_immortal = FALSE
        "#,
    )
    .execute(pool)
    .await?;

    tracing::info!("Decay/pruning complete: evicted {} records", deleted.rows_affected());
    Ok(())
}

pub async fn start_consolidation_loop(engine: Arc<DoryEngine>) {
    let mut timer = interval(Duration::from_secs(3600));
    loop {
        timer.tick().await;
        tracing::info!("Consolidation loop: idle trigger check");
        if let Err(e) = run_consolidation(&engine).await {
            tracing::error!("Consolidation error: {e}");
        }
    }
}

async fn run_consolidation(engine: &DoryEngine) -> Result<(), sqlx::Error> {
    let pool = engine.pool_ref();
    let clusters = sqlx::query_as::<_, (uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>, serde_json::Value)>(
        r#"
        WITH clustered AS (
            SELECT a.id, a.namespace, a.content_l0, a.created_at, a.tags,
                   b.id AS sibling_id
            FROM dory_memories a
            JOIN dory_memories b ON a.id < b.id
                AND a.namespace = b.namespace
                AND a.created_at BETWEEN b.created_at - INTERVAL '1 hour' AND b.created_at + INTERVAL '1 hour'
                AND a.embedding <=> b.embedding < 0.15
                AND a.tags @> '["type:fragment"]'::jsonb
            WHERE a.is_immortal = FALSE
        )
        SELECT DISTINCT id, namespace, content_l0, created_at, tags
        FROM clustered
        LIMIT 10
        "#,
    )
    .fetch_all(pool)
    .await?;

    if clusters.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    let merged_tags: serde_json::Value = serde_json::json!(["type:consolidated"]);
    let merged_content: String = clusters
        .iter()
        .map(|c| c.2.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    sqlx::query(
        r#"
        INSERT INTO dory_memories (namespace, content_l0, tags)
        SELECT $1, $2, $3
        WHERE NOT EXISTS (
            SELECT 1 FROM dory_memories WHERE content_l0 = $2 AND namespace = $1
        )
        "#,
    )
    .bind(&clusters[0].1)
    .bind(&merged_content)
    .bind(&merged_tags)
    .execute(&mut *tx)
    .await?;

    for (fid, _, _, _, _) in &clusters {
        sqlx::query("DELETE FROM dory_memories WHERE id = $1 AND is_immortal = FALSE")
            .bind(fid)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    tracing::info!("Consolidation merged {} fragment(s)", clusters.len());
    Ok(())
}