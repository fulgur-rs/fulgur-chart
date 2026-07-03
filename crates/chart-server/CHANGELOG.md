# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- *(chart-server)* durable filesystem shortlink backend (`FileShortlinkStore`): short links are persisted to `FULGUR_SHORTLINK_DIR` (default `./fulgur-shortlinks`, one file per link) and now survive server restart/redeploy when that dir is on durable storage
- *(chart-server)* `FULGUR_SHORTLINK_DIR` config for the shortlink storage directory
- *(chart-server)* Docker image bakes a nonroot-owned `/data` and sets `FULGUR_SHORTLINK_DIR=/data`; `docker-compose.yml` mounts a `shortlinks` volume there

### Changed

- **BREAKING** *(chart-server)* the in-memory shortlink store is replaced by the filesystem backend as the sole/default backend

### Removed

- **BREAKING** *(chart-server)* `FULGUR_SHORTLINK_LIMIT` and `FULGUR_SHORTLINK_MAX_BYTES` (aggregate count/byte caps): the filesystem backend enforces only the per-entry cap (`FULGUR_SHORTLINK_ENTRY_BYTES`). Deployments still setting these env vars will fail to start

## [0.1.0](https://github.com/fulgur-rs/fulgur-chart/releases/tag/chart-server-v0.1.0) - 2026-07-01

### Added

- *(chart-server)* generate shortlink ids as ULIDs instead of content-hash
- *(chart-server)* define ShortlinkBackend trait and BackendError
- *(chart-server)* WebP を既定 disable + 面積予算を設定可能に
- *(png)* demultiply 高速化 + 圧縮プリセット (fast/balanced/high)
- *(chart-server)* change default output format from svg to png
- *(chart-server)* implement MCP Streamable HTTP endpoint (/mcp)
- *(chart-server)* add Railway, Cloudflare, and Docker Compose deployment configs
- *(chart-server)* add Dockerfile and GitHub Actions CI/CD
- *(chart-server)* add GET /openapi.json and GET /docs (Swagger UI)
- *(chart-server)* add compression, CORS, body limit, semaphore, timeout, rate limit
- *(chart-server)* implement POST /chart/create and GET /chart/s/{id}
- *(chart-server)* implement POST /chart/validate
- *(chart-server)* implement GET/POST /chart with ETag cache headers
- *(chart-server)* add render helper (parse→validate→render)
- *(chart-server)* add /health (JSON) and /llms.txt endpoints
- *(chart-server)* bootstrap axum server with /health
- *(chart-server)* add Config struct with clap + env var support

### Fixed

- *(chart-server)* address gemini-code-assist review on PR #108
- *(chart-server)* shortlink ストアに集約/per-entry バイト上限を追加 (DoS 緩和)
- *(chart-server)* WebP 予算をライブラリ上限にクランプ + 304 前に enabled 判定 (Codex review)
- AI レビュー対応 — compression を起動時設定に、ほか
- address post-merge Codex review feedback
- address AI review feedback (round 6, mcp batch/id hardening)
- address AI review feedback (round 5, mcp handler hardening)
- address AI review feedback (round 4)
- reject wrong-typed MCP arguments with -32602
- address Codex review feedback
- address AI review feedback (round 3)
- address AI review feedback (round 2)
- *(chart-server)* address AI review feedback
- *(chart-server)* fix clippy warnings (derivable_impls, collapsible_if)
- *(chart-server)* fix middleware order, POST /chart width/height, MCP width/height
- *(chart-server)* guard GovernorLayer against rate_limit edge cases (0 or >60000)
- *(chart-server)* make ShortlinkStore.insert atomic with AtomicUsize
- *(chart-server)* fix ETag comparison (RFC 7232) and remove unwrap in apply_overrides

### Other

- *(chart-server)* update stale determinism comment in ShortlinkStore::insert
- *(chart-server)* add ulid dependency
- *(chart-server)* verify external backend injection + Unavailable 503 path via public API
- *(chart-server)* restore store test diagnostics + guard against await-in-shard
- *(chart-server)* store AppState behind Arc<dyn ShortlinkBackend>
- *(chart-server)* split bin into lib + thin composition root
- *(chart-server)* 上書き失敗時の値保持とバイトロールバックを検証
- *(chart-server)* Error Codes 表に 415 UNSUPPORTED_FORMAT を追記 (coderabbit)
- Merge pull request #100 from fulgur-rs/perf/png-demultiply-fast
- *(chart-server)* ETag に compression を含めない理由を明記
- add coverage for PR #95 changed lines
- simplify background_color sentinel to unwrap_or
- mark chart-server as experimental
- rewrite chart-server README in English
- chart-server の README.md を追加
- add chart-server crate to workspace
