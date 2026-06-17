# Dory Memory

[![Rust](https://img.shields.io/badge/rust-1.85%2B-dea584?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen)](https://github.com/anomalyco/dory-memory/pulls)
![Docker](https://img.shields.io/badge/docker-compose-2496ED?logo=docker)

**Backend memory store for [Hermes Agent](https://github.com/anomalyco/hermes-agent).**  
A semantic memory engine powered by **pgvector** (PostgreSQL vector database) with server-side embeddings via any OpenAI-compatible API.

Store, search, and maintain agent memories with hybrid vector + full-text retrieval, automatic decay, consolidation, and a sandbox for ephemeral context.

---

## Features

- **Hybrid search** — Reciprocal Rank Fusion over vector cosine distance and full-text search (`tsvector`)
- **Intent routing** — Automatically classifies incoming memories: `Task`, `Reference`, `Environment`, `Preference`, `Backlog`, `Pivot`, `Correction`
- **Immortal memories** — Protected from decay/pruning; set automatically for `Reference` and `Environment` intents
- **Embedding cache** — In-memory `DashMap` deduplication; saves API calls on repeated content
- **Ephemeral sandbox** — `Pivot` and `Task` intents stage memories in a `VecDeque` before committing on flush
- **Temporal recall** — Query memories within ISO datetime windows
- **Token-budget search** — `recall_within_token_budget` fits results into an LLM context limit
- **Maintenance** — Automatic decay/importance adjustment every 24 h, stale-memory listing, batch purge (immortal-protected)
- **Consolidation** — Merges sandbox to DB when idle
- **Workspace telemetry** — `notify`-based file watcher drives proactive horizon sweeps
- **Hermes plugin** — Drop-in `MemoryProvider` plugin with 6 agent tools (`recall`, `sweep`, `search_temporal`, `list_stale`, `purge`, `stats`)

---

## Architecture

```
Hermes Agent (Python MemoryProvider plugin)
        │  HTTP
        ▼
┌──────────────────────────────────────────┐
│          axum HTTP server (routes.rs)     │
│                                          │
│  ┌──────────┐  ┌──────────┐  ┌────────┐ │
│  │ guard.rs │  │ embed.rs │  │ cache  │ │
│  │ sanitize │→│ embedding │→│ .rs    │ │
│  │ redact   │  │ API call  │  │ DashMap│ │
│  └──────────┘  └──────────┘  └────────┘ │
│         │              │                  │
│         ▼              ▼                  │
│  ┌──────────────────────────────┐        │
│  │      DoryEngine (engine.rs)   │        │
│  │  process_and_route_memory    │        │
│  │  hybrid_recall / temporal    │        │
│  │  proactive_horizon_sweep     │        │
│  └──────────┬───────────────────┘        │
│             │                             │
└─────────────┼─────────────────────────────┘
              │  sqlx
              ▼
┌──────────────────────┐
│  PostgreSQL + pgvector │
│  - dory_memories       │
│  - dory_namespaces     │
└──────────────────────┘

Background workers (workers.rs):
  - Decay/pruning (every 24h)
  - Consolidation (idle trigger)

Telemetry daemon (telemetry.rs):
  - notify watcher on workspace
```

---

## Quick start

### With Docker Compose (recommended)

```bash
cp .env.example .env     # edit your API key
docker compose up -d
```

### Without Docker

```bash
# Start PostgreSQL with pgvector
docker run -d --name dory-pg \
  -e POSTGRES_USER=dory -e POSTGRES_PASSWORD=dory -e POSTGRES_DB=dory \
  -p 5432:5432 pgvector/pgvector:pg16

# Build and run
cargo run
```

---

## Configuration

Create `dory.toml`:

```toml
[database]
url = "postgres://dory:dory@localhost:5432/dory"

[server]
host = "0.0.0.0"
port = 5005

[embedding]
api_url = "https://api.openai.com/v1/embeddings"
api_key = "sk-..."              # or DORY_EMBEDDING_API_KEY env var
model = "text-embedding-ada-002"
dimensions = 1536
```

Set `DORY_CONFIG` to your config path (defaults to `./dory.toml`).

> **Security note:** Prefer the `DORY_EMBEDDING_API_KEY` environment variable over writing the key in `dory.toml`. The config loader checks the env var first.

---

## API Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/memories` | Insert a memory |
| `POST` | `/v1/search` | Hybrid semantic + full-text search |
| `POST` | `/v1/search/temporal` | Recall within a time window |
| `POST` | `/v1/search/budget` | Token-budgeted search (for prefetch) |
| `GET` | `/v1/sweep/{namespace}` | Proactive horizon sweep (stale tasks) |
| `POST` | `/v1/maintenance/stale` | List stale non-immortal memories |
| `POST` | `/v1/batch/delete` | Batch delete (immortal protected) |
| `GET` | `/v1/stats` | Database statistics |

---

## Hermes Plugin

The plugin lives in [`plugins/memory/dory/`](plugins/memory/dory/) and provides:

- **6 agent tools:** `recall`, `sweep`, `search_temporal`, `list_stale`, `purge`, `stats`
- **Auto-namespace:** derived from Hermes profile name
- **Background sync:** async `sync_turn` records conversation turns
- **CLI:** `hermes dory status`, `hermes dory config`, `hermes dory stats`

Install by copying `plugins/memory/dory/` into your Hermes agent's plugin directory.  
Set `DORY_API_URL` (default `http://localhost:5005`).

---

## Development

```bash
cargo check              # fast validation
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test               # unit + integration
cargo fmt                # formatting
```

Requires **Rust ≥1.85** (edition 2024). The project pins `stable` in `rust-toolchain.toml`.

### Project layout

```
src/
├── main.rs       # Entrypoint: config, pool, migrations, axum, workers
├── config.rs     # TOML config struct + env var overrides
├── error.rs      # DoryError (thiserror) + axum IntoResponse
├── models.rs     # DoryMemoryNode, DoryInsertPayload, DoryIntent, TimeWindow
├── guard.rs      # Secret redaction + prompt-injection sanitization
├── embed.rs      # OpenAI-compatible API client
├── cache.rs      # DashMap embedding cache + VecDeque sandbox
├── engine.rs     # Core engine: routing, recall, stats, maintenance
├── routes.rs     # Axum HTTP handlers
├── workers.rs    # Decay/pruning + consolidation
└── telemetry.rs  # Workspace file watcher
```