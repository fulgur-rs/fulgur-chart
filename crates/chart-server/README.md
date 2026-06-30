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
| `FULGUR_SHORTLINK_LIMIT` | `10000` | Maximum number of stored short links |
| `FULGUR_SHORTLINK_MAX_BYTES` | `134217728` (128 MiB) | Aggregate byte budget across all stored short links (bounds memory; oldest is **not** evicted — see below) |
| `FULGUR_SHORTLINK_ENTRY_BYTES` | `524288` (512 KiB) | Per-entry byte cap for a stored short link. Oversized requests are rejected with `413 PAYLOAD_TOO_LARGE`. The stored value is the URL-encoded chart JSON (up to ~3× the raw body), so keep this ≳ `3 × FULGUR_MAX_BODY_SIZE`; raising `FULGUR_MAX_BODY_SIZE` without raising this will 413 legitimate large charts |
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
docker run -p 3000:3000 chart-server
```

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
