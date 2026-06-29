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
| `compression` | `fast` \| `balanced` \| `high` | `balanced` | PNG size/speed tradeoff (PNG only) |

### POST /chart

```json
{
  "chart": { "type": "bar", "data": { ... } },
  "format": "png",
  "width": 800,
  "height": 400,
  "backgroundColor": "white",
  "compression": "balanced",
  "dsl": "chartjs"
}
```

## Output Formats

| Value | Content-Type | Description |
|-------|-------------|-------------|
| `png` | `image/png` | Binary PNG (default) |
| `svg` | `image/svg+xml` | Inline SVG |
| `webp` | `image/webp` | Binary WebP |
| `data-uri` | `text/plain` | `data:image/svg+xml;base64,…` |

## PNG compression

The `compression` parameter trades encode speed for file size (PNG only; WebP is
lossless and SVG is text). All presets produce pixel-identical, deterministic output.

| Preset | Filter / deflate | Speed | Size |
|--------|------------------|-------|------|
| `fast` | Sub + fdeflate | fastest | largest |
| `balanced` (default) | adaptive + fdeflate | fast | small |
| `high` | adaptive + zlib L6 | slowest | smallest |

`balanced` keeps most of `fast`'s speed with a large size reduction; `high` minimizes
size further at a higher encode cost. Pixels are identical across all presets.

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
| `FULGUR_CORS_ORIGINS` | `*` | Allowed CORS origins (comma-separated) |
| `FULGUR_RATE_LIMIT` | `0` | Rate limit (requests/minute/IP). `0` disables rate limiting (default) |

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
| 429 | — | Rate limit exceeded |
| 503 | `BUSY` | Concurrent render limit exceeded |
| 504 | `TIMEOUT` | Render timed out |
| 500 | `RENDER_ERROR` | Internal render error |
