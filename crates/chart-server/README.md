# fulgur chart-server

Chart.js v4 互換 JSON spec を受け取り、SVG / PNG / WebP / data-URI を返す HTTP レンダリングサーバー。

- **OpenAPI UI**: `/docs`
- **OpenAPI JSON**: `/openapi.json`
- **MCP エンドポイント**: `POST /mcp` (JSON-RPC 2.0, MCP 2025-03-26)

## クイックスタート

```bash
cargo run -p chart-server
```

```bash
curl -X POST http://localhost:3000/chart \
  -H 'Content-Type: application/json' \
  -d '{"chart":{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}}'
# → PNG バイナリ (Content-Type: image/png)
```

## エンドポイント

| Method | Path | 説明 |
|--------|------|------|
| `GET`  | `/chart` | クエリパラメータでレンダリング |
| `POST` | `/chart` | JSON body でレンダリング |
| `POST` | `/chart/validate` | spec を検証のみ（レンダリングなし） |
| `POST` | `/chart/create` | 短縮 URL を作成 |
| `GET`  | `/chart/s/{id}` | 短縮 URL を `/chart?c=…` にリダイレクト |
| `POST` | `/mcp` | MCP (JSON-RPC 2.0) |
| `GET`  | `/health` | ヘルスチェック |
| `GET`  | `/llms.txt` | LLM 向け API 説明 |

### GET /chart

| パラメータ | 型 | デフォルト | 説明 |
|---|---|---|---|
| `c` | JSON string | 必須 | Chart.js v4 spec（URL エンコード済み） |
| `f` | `svg` \| `png` \| `webp` \| `data-uri` | `png` | 出力フォーマット |
| `w` | integer | — | 幅 (px) |
| `h` | integer | — | 高さ (px) |
| `bkg` | string | transparent | 背景色 |

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

## 出力フォーマット

| 値 | Content-Type | 説明 |
|---|---|---|
| `png` | `image/png` | バイナリ PNG（デフォルト） |
| `svg` | `image/svg+xml` | インライン SVG |
| `webp` | `image/webp` | バイナリ WebP |
| `data-uri` | `text/plain` | `data:image/svg+xml;base64,…` |

## 環境変数

| 変数 | デフォルト | 説明 |
|---|---|---|
| `PORT` | — | Railway 等が inject するポート（`FULGUR_PORT` より優先） |
| `FULGUR_PORT` | `3000` | バインドポート |
| `FULGUR_HOST` | `0.0.0.0` | バインドアドレス |
| `FULGUR_MAX_CONCURRENT` | CPU 数 | 同時レンダリング数上限 |
| `FULGUR_MAX_BODY_SIZE` | `102400` | リクエストボディ上限（バイト） |
| `FULGUR_RENDER_TIMEOUT_MS` | `1000` | レンダリングタイムアウト（ミリ秒） |
| `FULGUR_SHORTLINK_LIMIT` | `10000` | 短縮 URL 保持数上限 |
| `FULGUR_CORS_ORIGINS` | `*` | CORS 許可オリジン（カンマ区切り） |
| `FULGUR_RATE_LIMIT` | `60` | レート制限（リクエスト/分/IP） |

## Docker

```bash
docker build -f crates/chart-server/Dockerfile -t chart-server .
docker run -p 3000:3000 chart-server
```

## エラーコード

| HTTP | code | 原因 |
|---|---|---|
| 400 | `PARSE_ERROR` | spec のパースエラー |
| 400 | `VALIDATE_ERROR` | 入力上限検証エラー |
| 400 | `MISSING_PARAM` | 必須パラメータ不足 |
| 404 | `NOT_FOUND` | 短縮 URL が存在しない |
| 429 | — | レート制限超過 |
| 503 | `BUSY` | 同時レンダリング上限超過 |
| 504 | `TIMEOUT` | レンダリングタイムアウト |
| 500 | `RENDER_ERROR` | レンダリング内部エラー |
