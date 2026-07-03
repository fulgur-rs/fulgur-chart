# fulgur chart-server

> **⚠️ Experimental** — Not production-ready. APIs and behavior may change without notice.

HTTP rendering server that accepts Chart.js v4 JSON specs and returns SVG, PNG, WebP, or data-URI.

- **OpenAPI UI**: `/docs`
- **OpenAPI JSON**: `/openapi.json`
- **MCP endpoint**: `POST /mcp` (JSON-RPC 2.0, MCP 2025-03-26)

## Quick Start

```bash
cargo run -p chart-server
```

```bash
curl -X POST http://localhost:3000/chart \
  -H 'Content-Type: application/json' \
  -d '{"chart":{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}}'
# → PNG binary (Content-Type: image/png)
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/chart` | Render via query parameters |
| `POST` | `/chart` | Render via JSON body |
| `POST` | `/chart/validate` | Validate spec without rendering |
| `POST` | `/chart/create` | Create a short link |
| `GET`  | `/chart/s/{id}` | Redirect short link to `/chart?c=…` |
| `POST` | `/mcp` | MCP tool endpoint (JSON-RPC 2.0) |
| `GET`  | `/health` | Health check |
| `GET`  | `/llms.txt` | LLM-readable API description |

### GET /chart

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `c` | JSON string | required | Chart.js v4 spec (URL-encoded) |
| `f` | `svg` \| `png` \| `webp` \| `data-uri` | `png` | Output format |
| `w` | integer | — | Width (px) |
| `h` | integer | — | Height (px) |
| `bkg` | string | transparent | Background color |

### POST /chart

```json
{
  "chart": { "type": "bar", "data": { ... } },
  "format": "png",
  "width": 800,
  "height": 400,
  "backgroundColor": "white",
  "dsl": "chartjs"
}
```

## Output Formats

| Value | Content-Type | Description |
|-------|-------------|-------------|
| `png` | `image/png` | Binary PNG (default) |
| `svg` | `image/svg+xml` | Inline SVG |
| `webp` | `image/webp` | Binary WebP — **disabled by default** (opt-in, see below) |
| `data-uri` | `text/plain` | `data:image/svg+xml;base64,…` |

## PNG compression

PNG encode trades speed for file size. This is a **server-wide startup setting**
(`FULGUR_PNG_COMPRESSION` / `--png-compression`), not a per-request parameter —
the operator picks one preset for the deployment. WebP is lossless and SVG is text,
so the setting only affects PNG. All presets produce pixel-identical, deterministic output.

| Preset | Filter / deflate | Speed | Size |
|--------|------------------|-------|------|
| `fast` | Sub + fdeflate | fastest | largest |
| `balanced` (default) | adaptive + fdeflate | fast | small |
| `high` | adaptive + zlib L6 | slowest | smallest |

`balanced` keeps most of `fast`'s speed with a large size reduction; `high` minimizes
size further at a higher encode cost. Pixels are identical across all presets.

## WebP output (opt-in)

WebP lossless encoding holds up to **three** full-frame buffers at peak (the pixmap,
the encoder's mutable input copy, and the encoded VP8L chunk, each up to `area × 4`
bytes for poorly-compressible content). For an untrusted-input server this is an
OOM vector, so WebP is **disabled by default** and must be enabled with
`FULGUR_WEBP_ENABLED=true`. When enabled, `FULGUR_MAX_WEBP_AREA` caps the post-scale
pixel area (peak memory ≈ `area × 4 × 3`); lower it to fit a tight memory budget.
The default equals the library's hard limit, which the renderer also enforces.
With WebP disabled, `format=webp` returns `415 Unsupported Media Type`.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | — | Port injected by Railway etc. (takes precedence over `FULGUR_PORT`) |
| `FULGUR_PORT` | `3000` | Bind port |
| `FULGUR_HOST` | `0.0.0.0` | Bind address |
| `FULGUR_MAX_CONCURRENT` | CPU count | Maximum concurrent renders |
| `FULGUR_MAX_BODY_SIZE` | `102400` | Request body size limit (bytes) |
| `FULGUR_RENDER_TIMEOUT_MS` | `1000` | Render timeout (milliseconds) |
| `FULGUR_SHORTLINK_DIR` | `./fulgur-shortlinks` | Directory where short links are persisted (one file per link). Created on startup; the server fails fast if it can't be created. For persistence across redeploy this must live on durable storage — see [Short link persistence](#short-link-persistence) |
| `FULGUR_SHORTLINK_ENTRY_BYTES` | `524288` (512 KiB) | Per-entry byte cap for a stored short link. Oversized requests are rejected with `413 PAYLOAD_TOO_LARGE`. The stored value is the URL-encoded chart JSON (up to ~3× the raw body), so keep this ≳ `3 × FULGUR_MAX_BODY_SIZE`; raising `FULGUR_MAX_BODY_SIZE` without raising this will 413 legitimate large charts |
| `FULGUR_SHORTLINK_TTL_SECONDS` | `86400` (24h) | Guaranteed minimum resolvable lifetime for a short link, as a floor guarantee (the underlying data isn't necessarily deleted at exactly this time). Used as the `Cache-Control: max-age` on successful `/chart/s/{id}` resolutions so upstream CDNs don't serve stale resolutions past the guarantee window |
| `FULGUR_CORS_ORIGINS` | `*` | Allowed CORS origins (comma-separated) |
| `FULGUR_RATE_LIMIT` | `0` | Rate limit (requests/minute/IP). `0` disables rate limiting (default) |
| `FULGUR_PNG_COMPRESSION` | `balanced` | PNG compression preset: `fast` / `balanced` / `high` (PNG only) |
| `FULGUR_WEBP_ENABLED` | `false` | Allow `format=webp`. Off by default (WebP has a higher peak-memory cost; opt-in) |
| `FULGUR_MAX_WEBP_AREA` | library limit (~21.3M) | Max post-scale pixel area for WebP. Peak memory ≈ `area × 4 × 3`; lower to tighten |

## Docker

The Dockerfile expects a pre-built binary. Build from the repository root:

```bash
cargo build --release -p chart-server
cp target/release/chart-server chart-server-bin
docker build -f crates/chart-server/Dockerfile -t chart-server .
# mount a volume at /data so short links survive `docker restart` and redeploys
docker run -p 3000:3000 -v chart-shortlinks:/data chart-server
```

The image sets `FULGUR_SHORTLINK_DIR=/data` (a nonroot-owned directory baked into
the image), so it starts even without a volume — but short links are then lost on
redeploy. Mount a volume at `/data` (as above, or via the provided
`docker-compose.yml`) to persist them. See [Short link persistence](#short-link-persistence).

## Short link persistence

Short links (`POST /chart/create` → `GET /chart/s/{id}`) are stored on disk under
`FULGUR_SHORTLINK_DIR`, one file per link, and resolve across process restarts and
redeploys **as long as that directory is on durable storage**. This is single-node
durability: the directory is host/volume-local, so a multi-instance deployment behind
a load balancer only resolves links created on the same node (or on a shared network
volume). Horizontal scale-out is out of scope for this backend.

- **Docker / Compose:** mount a volume at `/data` (the image default). The provided
  `docker-compose.yml` already does this.
- **Railway:** attach a Volume and set `FULGUR_SHORTLINK_DIR` to its mount path;
  without a Volume the filesystem is ephemeral and links vanish on redeploy. If the
  process runs as nonroot (e.g. deploying the Docker image as-is) and writes to the
  Volume fail with a permission error, also set `RAILWAY_RUN_UID=0` — Railway mounts
  Volumes as root, so a nonroot process otherwise can't write to them.

There is no active TTL deletion or LRU eviction, so the directory grows unbounded;
`FULGUR_SHORTLINK_TTL_SECONDS` is only a `Cache-Control` floor guarantee, not a
storage lifetime. Capacity management (TTL sweep / eviction) is tracked separately.

## Error Codes

| HTTP | Code | Cause |
|------|------|-------|
| 400 | `PARSE_ERROR` | Failed to parse spec |
| 400 | `VALIDATE_ERROR` | Input limit validation failed |
| 400 | `MISSING_PARAM` | Required parameter missing |
| 404 | `NOT_FOUND` | Short link not found |
| 415 | `UNSUPPORTED_FORMAT` | Format disabled by server policy (e.g. WebP opt-in off) |
| 429 | — | Rate limit exceeded |
| 503 | `BUSY` | Concurrent render limit exceeded |
| 504 | `TIMEOUT` | Render timed out |
| 500 | `RENDER_ERROR` | Internal render error |
