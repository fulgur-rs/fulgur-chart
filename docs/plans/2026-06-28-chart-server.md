# chart-server 設計ドキュメント

## 概要

fulgur-chart の HTTP ラッパーサーバ。
Chart.js v4 互換 JSON spec を受け取り SVG/PNG を返す self-hosted サービス。
API エンドポイント設計は独自（QuickChart 互換ではなく Chart.js v4 spec 互換）。

## アーキテクチャ

```
クライアント
    │
    │  GET /chart?c={JSON}&w=500&h=300&f=svg
    │  POST /chart  (JSON body)
    ▼
[CDN / リバースプロキシ]  ← Cache-Control + ETag でキャッシュ
    │
    ▼
chart-server (axum)
    │
    ▼
fulgur_chart::render_chart()
    │
    ▼
SVG string / PNG bytes
```

**実装場所:** `crates/chart-server/` (新 workspace crate)

## API エンドポイント

### GET /chart

URL パラメータで spec を渡す。`<img src="...">` への直接埋め込み用。

| パラメータ | 型 | デフォルト | 説明 |
|---|---|---|---|
| `c` | JSON string | 必須 | Chart.js v4 spec |
| `w` | integer | 500 | 幅 (px) |
| `h` | integer | 300 | 高さ (px) |
| `bkg` | string | transparent | 背景色 |
| `f` | `svg` \| `png` \| `webp` \| `data-uri` | `svg` | 出力フォーマット |

### POST /chart

```json
{
  "chart": { "type": "bar", "data": { ... } },
  "width": 800,
  "height": 600,
  "backgroundColor": "#ffffff",
  "format": "png"
}
```

### POST /chart/validate

レンダリングせずに spec のパース・バリデーションのみ実行する。AI エージェントが spec を生成した後、render 前に確認するのに使う。

**リクエスト:** `chart` フィールドのみ必須（`width`/`height`/`format` は不要）

```json
{ "chart": { "type": "bar", "data": { ... } } }
```

**レスポンス 200:**
```json
{ "valid": true }
```

**レスポンス 400:**
```json
{ "valid": false, "error": "unknown chart type 'baz'", "code": "PARSE_ERROR" }
```

render をスキップするため軽量。レート制限・タイムアウトは `/chart` より緩くてよい。

### POST /chart/create

大きな spec を GET 共有可能な短縮 URL に変換する。QuickChart の `/chart/create` 準拠。

**リクエスト:** POST /chart と同じ body

**レスポンス:**
```json
{ "url": "/chart/s/a3f9c2b1" }
```

短縮 ID は `sha256(spec)` の先頭 8 文字。

### GET /chart/s/{id}

```
307 Redirect → /chart?c=...&w=...&h=...&f=...
```

spec はインメモリ `HashMap` に保持（プロセス再起動で消える。永続化は v1 対象外）。

### GET /openapi.json

OpenAPI 3.0 スキーマを JSON で返す。`utoipa` で自動生成。

### GET /docs

Swagger UI (`utoipa-swagger-ui`)。

### GET /health

```json
{ "status": "ok", "version": "0.1.0" }
```

## レスポンスヘッダ

### Content-Type

| フォーマット | Content-Type |
|---|---|
| SVG | `image/svg+xml; charset=utf-8` |
| PNG | `image/png` |

### 圧縮

`tower-http::compression::CompressionLayer` を全体に適用し、クライアントの `Accept-Encoding` に応じて gzip / brotli / deflate で圧縮する。PNG は既圧縮のため `image/png` を除外リストに指定してスキップ。SVG は XML テキストのため圧縮効果が大きい（通常 60〜80% 削減）。

### キャッシュヘッダ

```
Cache-Control: public, max-age=86400, immutable
ETag: "<sha256(spec)>-<server_version>"
X-Fulgur-Version: 0.1.0
Vary: Accept
```

**バージョン込み ETag の役割:**
サーバのバージョンが上がると ETag が全件変わるため、バグ修正等でレンダリング結果が変わった場合も CDN が自動的にキャッシュを破棄して再フェッチする。手動 purge 不要。

クライアントが `If-None-Match` を付けた場合、hash が一致すれば `304 Not Modified` を返す。

## エラーレスポンス

```json
{
  "error": "Invalid chart spec: unknown chart type 'foo'",
  "code": "PARSE_ERROR"
}
```

| HTTP | code | 原因 |
|---|---|---|
| 400 | `PARSE_ERROR` | spec のパースエラー |
| 400 | `VALIDATE_ERROR` | strict モードの検証エラー |
| 400 | `MISSING_PARAM` | 必須パラメータ不足 |
| 404 | `NOT_FOUND` | 短縮 URL の ID が存在しない |
| 500 | `RENDER_ERROR` | レンダリング内部エラー |

## 実装構成

```
crates/chart-server/
├── Cargo.toml
└── src/
    ├── main.rs          # CLI args (--host, --port, --cache-size)
    ├── server.rs        # axum router 組み立て
    ├── handlers/
    │   ├── chart.rs     # GET/POST /chart
    │   ├── create.rs    # POST /chart/create
    │   └── shortlink.rs # GET /chart/s/{id}
    ├── request.rs       # ChartRequest デシリアライズ + バリデーション
    ├── response.rs      # キャッシュヘッダ付与ロジック
    └── store.rs         # 短縮URL用インメモリ HashMap (DashMap)
```

## 依存クレート

| クレート | 用途 |
|---|---|
| `axum` | HTTP フレームワーク |
| `tokio` | 非同期ランタイム |
| `tower-http` | CORS・リクエストロギング・圧縮 |
| `tower-governor` | IP レート制限 (leaky bucket) |
| `utoipa` + `utoipa-axum` | OpenAPI スキーマ生成 |
| `utoipa-swagger-ui` | Swagger UI |
| `dashmap` | 短縮URL用スレッドセーフ HashMap |
| `sha2` | ETag 用 SHA-256 |
| `serde` + `serde_json` | JSON デシリアライズ |
| `clap` | CLI 引数 |

## AI エージェント対応

### 防御: 濫用・暴走対策

AI エージェントはループバグや大量並列呼び出しで人間より高頻度にリクエストを生成する。

| 対策 | 仕組み | デフォルト |
|---|---|---|
| IP レート制限 | `tower-governor`（leaky bucket） | 60 req/分/IP |
| キュー深さ上限 | Semaphore の待機数上限 | `max-concurrent × 4` |
| キュー満杯時 | 即座に 503（待たせない） | — |
| レート超過時 | 429 + `Retry-After` + `X-RateLimit-*` ヘッダ | — |

CLI/環境変数で設定可能:

| CLI フラグ | 環境変数 | デフォルト |
|---|---|---|
| `--rate-limit` | `FULGUR_RATE_LIMIT` | `60` (req/分/IP) |
| `--queue-depth` | `FULGUR_QUEUE_DEPTH` | `max-concurrent × 4` |

### 活用: AI エージェントから使いやすくする

**MCP サーバーエンドポイント (`/mcp`)**

Streamable HTTP (MCP 2025-03-26) で `generate_chart` ツールを公開。Claude や他の MCP 対応エージェントが直接ツールとして呼び出せる。

```json
// generate_chart ツール
{
  "name": "generate_chart",
  "description": "Render a Chart.js spec to SVG or PNG",
  "inputSchema": { "$ref": "/openapi.json#/components/schemas/ChartRequest" }
}
```

レスポンスは SVG テキストまたは Base64 PNG を MCP `content` として返す。

**`GET /llms.txt`**

AI エージェントがサービスの使い方を発見・理解するための Markdown ファイル（llms.txt 標準準拠）。
`/openapi.json` への参照、主要エンドポイントの説明、チャート生成の使用例を含む。
静的ファイルとしてバイナリに埋め込み（`include_str!`）、バージョンアップ時に更新する。

```markdown
# fulgur chart-server

> Render Chart.js v4 specs to SVG or PNG over HTTP.

## Docs

- [OpenAPI Schema](/openapi.json)
- [Interactive Docs](/docs)

## Quick start

POST /chart
Content-Type: application/json

{"chart":{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}},"format":"svg"}

## Formats

- `svg` — inline SVG string (default)
- `png` — binary PNG
- `data-uri` — base64 data URI, paste directly into <img src> or Markdown ![]()
```

**`data-uri` フォーマット (`f=data-uri`)**

HTML や Markdown に直接埋め込める Base64 データ URI を返す。AI エージェントがチャートを会話の中で提示しやすい。

```
GET /chart?c={...}&f=data-uri
→ data:image/svg+xml;base64,PHN2ZyB4bWxucz0...
```

Content-Type は `text/plain` で返し、そのまま `<img src="...">` や Markdown `![]()` に使える。

## 同時実行制御・リクエスト制限

render_chart は CPU バウンドなため、並列数を制限して過負荷を防ぐ。

| 項目 | 仕組み | デフォルト |
|---|---|---|
| 同時レンダリング上限 | `tokio::sync::Semaphore` | CPU コア数 |
| 上限超過時 | 503 + `Retry-After: 1` | — |
| リクエスト body サイズ | `axum::extract::DefaultBodyLimit` | 100 KB |
| レンダリングタイムアウト | `tokio::time::timeout` | 1000 ms |
| タイムアウト時 | 504 + `{"code":"TIMEOUT"}` | — |

## CORS

`tower-http::cors::CorsLayer` で設定。デフォルトはすべてのオリジンを許可（self-hosted 前提）。
制限したい場合は `--cors-origins` または環境変数で上書きする。

## 設定注入

CLI フラグと環境変数（`FULGUR_` プレフィックス）を両対応。
環境変数が CLI フラグより優先される（Railway / Cloudflare Workers Containers での運用を考慮）。

| CLI フラグ | 環境変数 | デフォルト |
|---|---|---|
| `--host` | `FULGUR_HOST` | `0.0.0.0` |
| `--port` | `FULGUR_PORT` | `3000` |
| `--max-concurrent` | `FULGUR_MAX_CONCURRENT` | CPU コア数 |
| `--max-body-size` | `FULGUR_MAX_BODY_SIZE` | `102400` (100KB) |
| `--render-timeout` | `FULGUR_RENDER_TIMEOUT` | `1000` (ms) |
| `--shortlink-limit` | `FULGUR_SHORTLINK_LIMIT` | `10000` |
| `--cors-origins` | `FULGUR_CORS_ORIGINS` | `*` |
| `--log-level` | `FULGUR_LOG_LEVEL` | `info` |

## CLI インターフェース

```
chart-server [OPTIONS]

OPTIONS:
  --host <HOST>                バインドアドレス [default: 0.0.0.0] [env: FULGUR_HOST]
  --port <PORT>                ポート番号 [default: 3000] [env: FULGUR_PORT]
  --max-concurrent <N>         同時レンダリング上限 [default: CPU コア数] [env: FULGUR_MAX_CONCURRENT]
  --max-body-size <BYTES>      最大リクエスト body サイズ [default: 1048576] [env: FULGUR_MAX_BODY_SIZE]
  --render-timeout <MS>        レンダリングタイムアウト (ms) [default: 1000] [env: FULGUR_RENDER_TIMEOUT]
  --shortlink-limit <N>        短縮URL の最大保持件数 [default: 10000] [env: FULGUR_SHORTLINK_LIMIT]
  --cors-origins <ORIGINS>     許可オリジン（カンマ区切り、* で全許可）[default: *] [env: FULGUR_CORS_ORIGINS]
  --log-level <LEVEL>          ログレベル [default: info] [env: FULGUR_LOG_LEVEL]
```

## Docker

```dockerfile
FROM gcr.io/distroless/cc-debian12
COPY chart-server /usr/local/bin/chart-server
EXPOSE 3000
ENTRYPOINT ["chart-server"]
```

クロスコンパイルは `cargo zigbuild` / GitHub Actions の `upload-release-assets` ワークフローで既存 CLI と同様に対応。

## 配布・デプロイ

### コンテナイメージ

GitHub Actions で自動ビルド・プッシュ。

| レジストリ | イメージ名 |
|---|---|
| GitHub Container Registry（主） | `ghcr.io/fulgur-rs/chart-server:latest` |
| Docker Hub（ミラー） | `docker.io/fulgurrs/chart-server:latest` |

タグ: `latest`（main ブランチ）、`x.y.z`（リリースタグ）

### Railway

`railway.toml` を配置して Docker イメージを直接デプロイ。環境変数で設定注入。

```toml
[deploy]
startCommand = "chart-server"
healthcheckPath = "/health"
```

### Cloudflare Workers Containers

Docker コンテナをエッジで実行。`wrangler.toml` でコンテナイメージを指定。
CDN キャッシュは Cloudflare が自動処理するため、`Cache-Control` + `ETag` ヘッダがそのまま有効になる。

```toml
name = "chart-server"
main = "chart-server"

[containers]
image = "ghcr.io/fulgur-rs/chart-server:latest"
```

## v1 対象外

- 認証・API キー
- 短縮 URL の永続化（ファイル/DB）
- レート制限（前段の CDN/nginx に委ねる）
- `Surrogate-Key` ヘッダ（CDN が決まったタイミングで追加）
- WebFont アップロード API

## ライセンス

未決定。fulgur_chart コア・各バインディングは MIT / Apache-2.0 のまま。
chart-server のライセンスは Fulgur の OSS 哲学（`bd memories fulgur-philosophy` 参照）に基づき別途決定する。
