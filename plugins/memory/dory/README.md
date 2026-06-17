# Dory Memory — Hermes Agent Plugin

Backend: Rust + pgvector. Fast hybrid (semantic + full-text) search, temporal recall, auto-decay/pruning.

## Quick start

1. Start the Dory backend via Docker Compose:

   ```bash
   docker compose up -d
   ```

2. Set the environment variable in your Hermes profile:

   ```bash
   export DORY_API_URL=http://localhost:5005
   ```

3. Activate the plugin in Hermes:

   ```bash
   hermes memory setup   # selects dory provider, prompts for DORY_API_URL
   ```

4. Verify it works:

   ```bash
   hermes dory status
   ```

## Configuration

| Field | Env var | Default | Description |
|---|---|---|---|
| `api_url` | `DORY_API_URL` | `http://localhost:5005` | Dory backend HTTP address |
| `default_namespace` | — | (auto: profile name) | Override the namespace |

The namespace is auto-derived from the active Hermes profile directory name. To override, set `default_namespace` during `hermes memory setup`.

## Tools exposed to the agent

| Tool | Description |
|---|---|
| `recall` | Semantic + full-text hybrid search with optional tag filtering |
| `sweep` | Find stale `status:todo` tasks not accessed in 24h |
| `search_temporal` | Time-window based recall ("what happened yesterday") |

## How it works

- **`sync_turn()`** — Each conversation turn is persisted as a Dory memory (user input + assistant response). Non-blocking daemon thread.
- **`prefetch()`** — Before each API call, the most relevant 2000-token window of memories is injected into context.
- **`on_session_end()`** — Session summary is stored as an immortal `Reference` memory for long-term retention.
- **Immortal memories** — Protected at the DB level (trigger rejects deletes). Set automatically for `Reference` and `Environment` intents.

## API endpoints consumed

| Dory endpoint | Used by |
|---|---|
| `POST /v1/memories` | `sync_turn`, `on_session_end` |
| `POST /v1/search` | `recall` tool |
| `POST /v1/search/temporal` | `search_temporal` tool |
| `POST /v1/search/budget` | `prefetch` |
| `GET /v1/sweep/{namespace}` | `sweep` tool |