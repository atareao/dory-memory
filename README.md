# Dory Memory

[![Crates.io](https://img.shields.io/crates/v/dory-memory?logo=rust)](https://crates.io/crates/dory-memory)
[![GitHub Release](https://img.shields.io/github/v/release/atareao/dory-memory?logo=github)](https://github.com/atareao/dory-memory/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/atareao/dory-memory/ci.yml?branch=development&label=CI&logo=github)](https://github.com/atareao/dory-memory/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/actions/workflow/status/atareao/dory-memory/release.yml?label=Release&logo=github)](https://github.com/atareao/dory-memory/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Docker](https://img.shields.io/badge/docker-ready-2496ED?logo=docker)](https://github.com/atareao/dory-memory/pkgs/container/dory-memory)

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
- **Maintenance** — Automatic decay/importance adjustment every 24 h, stale-memory listing, batch purge (immortal-protected)
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

All configuration is via **environment variables** (no config file). Settings match the `DORY_` prefix in `.env.example`:

| Variable | Default | Description |
|---|---|---|
| `DORY_DATABASE_URL` | `postgres://dory:dory@localhost:5432/dory` | PostgreSQL connection string |
| `DORY_DATABASE_MAX_CONNECTIONS` | `10` | Connection pool size |
| `DORY_SERVER_HOST` | `0.0.0.0` | HTTP bind address |
| `DORY_SERVER_PORT` | `5005` | HTTP port |
| `DORY_EMBEDDING_API_URL` | `https://openrouter.ai/api/v1/embeddings` | OpenAI-compatible embeddings endpoint |
| `DORY_EMBEDDING_API_KEY` | — | API key (required) |
| `DORY_EMBEDDING_MODEL` | `BAAI/bge-m3` | Embedding model name |
| `DORY_EMBEDDING_DIMENSIONS` | `1024` | Vector dimension count |
| `DORY_WORKSPACE_ROOT` | — | Workspace path for telemetry (optional) |

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
├── config.rs     # Env-var config loader (DoryConfig::from_env)
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

### CI/CD

This project follows **Git Flow** with automated releases:

- PRs to `main` or `development` trigger CI (lint, build, test)
- Merging to `main` bumps version + generates changelog + creates tag
- Tagged releases build multi-arch binaries, publish to crates.io, and push Docker images to `ghcr.io/atareao/dory-memory`

See [GIT_FLOW.md](./GIT_FLOW.md) for the full workflow.