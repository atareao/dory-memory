# Dory Memory

Backend memory store for [Hermes Agent](https://github.com/anomalyco/hermes-agent). Implemented in Rust with pgvector (PostgreSQL vector database). Deployed via Docker Compose.

## Quick start

```bash
cargo build
cargo run
```

## Key facts

- **Rust edition 2024** — requires Rust ≥1.85. System toolchain is 1.95.0 (stable pinned in `rust-toolchain.toml`).
- **pgvector** — the primary data store. Docker Compose defines `postgres:16-pgvector`.
- **OpenAI-compatible embedding API** — Dory generates embeddings server-side by calling a configured `/v1/embeddings` endpoint (OpenAI, LiteLLM, text-embeddings-router).
- **Docker Compose** — required deploy target. `docker-compose.yml` defines PostgreSQL + pgvector + the app.
- **Config file** — `dory.toml` loaded at startup. Path set via `DORY_CONFIG` env var or defaults to `./dory.toml`.
- **Axum HTTP server** — one binary, one process, tokio async runtime.
- **No CI, no tests yet** — greenfield.

## Dependencies

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime (full features) |
| `axum` | HTTP framework |
| `sqlx` (postgres, runtime-tokio, tls-rustls, migrate, uuid, chrono) | Database driver |
| `serde` + `serde_json` | Serialization |
| `pgvector` | Vector type for sqlx |
| `tiktoken-rs` | Token counting (cl100k_base) |
| `dashmap` | Thread-safe embedding cache |
| `chrono` | Timezone-aware UTC timestamps |
| `uuid` | UUID generation |
| `notify` | File system watcher (telemetry daemon) |
| `reqwest` | HTTP client for embedding API calls |
| `tower-http` (cors) | Axum middleware |
| `tracing` + `tracing-subscriber` | Structured logging |
| `toml` | Config file parsing |
| `thiserror` | Error type derivation |
| `anyhow` | Binary-level error handling |
| `dotenvy` | `.env` loading for dev convenience |

## Development

```bash
cargo check              # fast validation
cargo clippy --all-targets --all-features --locked -- -D warnings   # strict lints
cargo test               # unit + integration
cargo fmt                # formatting (uses defaults — no rustfmt.toml)
```

### Rust conventions (Apollo handbook)

- **Error handling**: use `thiserror` for library crates, `anyhow` for binaries. This is a binary — prefer `anyhow::Result`.
- **Never `unwrap`/`expect` outside tests** — propagate with `?` or handle explicitly.
- **`#[expect(clippy::lint)]`** over `#[allow(...)]` (with justification) when suppressing lints.
- **`//` comments** explain *why* (safety, workarounds); **`///` doc comments** explain *what/how* for public APIs.
- **Small `Copy` types** (≤24 bytes) can be passed by value; prefer `&str` over `String`, `&[T]` over `Vec<T>` in params.
- **Name tests descriptively**: `fn process_should_return_error_when_input_empty()`.
- **One assertion per test** when practical.

## Module layout

```
src/
├── main.rs       # Entrypoint: config load, pool init, migration, axum server, workers
├── config.rs     # TOML config struct + loader
├── error.rs      # DoryError enum (thiserror) + axum IntoResponse
├── models.rs     # DoryMemoryNode, DoryInsertPayload, DoryIntent, TimeWindow
├── guard.rs      # Secret redaction + prompt-injection sanitization
├── embed.rs      # OpenAI-compatible embedding API client
├── cache.rs      # DashMap embedding cache + VecDeque sandbox
├── engine.rs     # DoryEngine: process_and_route_memory, hybrid_recall, temporal_recall, etc.
├── routes.rs     # Axum HTTP handlers (CRUD + search)
├── workers.rs    # Background tasks: decay/pruning + consolidation
└── telemetry.rs  # notify-based workspace watcher daemon
```

## Architecture

```
Hermes Agent (Python MemoryProvider plugin)
        │
        │ HTTP
        ▼
  axum HTTP server (routes.rs)
        │
        ▼
  DoryEngine (engine.rs)
        │
        ├──► cache.rs (DashMap + sandbox)
        ├──► guard.rs (security filter)
        ├──► embed.rs (embedding API client)
        └──► sqlx → PostgreSQL + pgvector

  Background workers (workers.rs):
    - Decay/pruning loop (every 24h)
    - Consolidation loop (idle trigger)
  
  Telemetry daemon (telemetry.rs):
    - notify watcher on workspace directory
```

## Spec errata (deviations from original)

1. `LANGUAGE phpgsql` → fixed to `plpgsql` in migration
2. `proactive_prefetch_workspace` hard-coded 1536 → reads `dory_namespaces.dimensions`
3. Embeddings are server-side (Dory calls OpenAI-compatible API), not pre-computed by client