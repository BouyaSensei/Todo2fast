# Todo2fast

A fast, collaborative todo manager with **document comprehension**.

Drop a PDF (or a document) in. Todo2fast reads it, pulls out the dates and
action items, and turns them into todos — then you and your team collaborate
on top: comments, reactions, and shared boards.

## Features

- **Boards & Todos** — organize work into boards; create, edit, complete, delete todos.
- **Comments & Reactions** — threaded discussion on every todo + emoji reactions.
- **Collaboration** — invite people to a board with share tokens; roles (owner / member).
- **PDF upload & date extraction** — upload a PDF, get its text and detected dates back.
- **Document comprehension** — turn a document into concrete todos. Uses a local
  LLM (Ollama) or a cloud model when available, with a deterministic fallback so it
  always works.

## Stack

| Layer     | Tech                                   |
|-----------|----------------------------------------|
| Backend   | Rust — `axum`, SQLite (`rusqlite`)      |
| Frontend  | React + TypeScript (Vite)               |
| E2E tests | Cypress                                |
| CI        | GitHub Actions: build, clippy, tests, `cargo audit` |

## Local development

### Backend
```bash
cd backend
cargo run          # serves on http://localhost:8080
cargo test         # unit + integration tests
```

### Frontend
```bash
cd frontend
npm install
npm run dev        # Vite dev server, proxies /api to :8080
npm run cy:e2e     # Cypress end-to-end tests
```

## Configuration

| Variable      | Default              | Description                          |
|---------------|----------------------|--------------------------------------|
| `T2F_ADDR`    | `0.0.0.0:8080`       | Bind address for the API server      |
| `T2F_DB_PATH` | `./todo2fast.sqlite` | SQLite database file                 |
| `T2F_LLM_URL` | `http://localhost:11434` | Ollama endpoint for comprehension |
| `T2F_LLM_MODEL` | `llama3.1`         | Model name for the LLM layer         |

## Security

CI enforces a security gate on every push: `cargo clippy -D warnings`, the full
test suite, and `cargo audit` (dependency vulnerability scan). The API binds
locally by default; enable auth tokens before exposing it publicly.
