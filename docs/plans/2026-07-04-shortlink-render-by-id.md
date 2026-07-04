# Shortlink resolve を render-by-id 化 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `GET /chart/s/{id}` を「307 で `/chart?{full-spec}` へリダイレクト」から「サーバ側で spec を引いて直接レンダリングして返す(render-by-id)」に変更し、spec を二度と URL に載せない。これで 414(request-line 上限)天井が消え、create と resolve の受理域が一致する。

**Architecture:** store 形式(`build_query` が作る percent-encode 済み query 文字列)は不変のまま維持する。`chart.rs` の GET ハンドラ本体を `render_from_query(q: ChartQuery, headers, state)` に切り出し、`get_shortlink` は store から引いた query 文字列を **GET と同一の抽出器**(`axum::extract::Query::try_from_uri`)で `ChartQuery` に parse し、その共有関数へ流す。成功(200/304)時のみ `Cache-Control` を shortlink TTL 値(8tr.3)に上書きし、それ以外は `no-store`。

**Tech Stack:** Rust / axum 0.8.9(`Query::try_from_uri` は public 確認済)/ tokio / `cargo test -p chart-server`。

**対象 issue:** fulgur-chart-8tr.4(epic fulgur-chart-8tr の最後の未完サブタスク)。design は issue の design フィールド参照。

**前提の事実(確認済):**
- `OutputFormat` は `Debug, Clone, Copy, Default, PartialEq, serde::Deserialize` を derive。`as_str()` は `"svg"/"png"/"webp"/"data-uri"`、Deserialize は `rename_all="lowercase"` + `DataUri` に `rename="data-uri"` で **完全一致**(create が書く `f=…` を resolve が同じ意味で読める)。
- create(`post_create`)は overrides を pre-apply せず、raw spec を `c=`、`w/h/bkg` を別パラメータで store に入れる。resolve を `render_from_query` に通せば overrides は GET と同じく **1 回だけ** 適用される。
- `crate::handlers::chart::{ChartQuery, render_from_query}` はクレート内(shortlink.rs)から到達可能(`handlers` は crate-private mod だが内部 `pub mod chart`)。
- ベースライン: `cargo test -p chart-server` = 51 unit + 5 integration passed, 0 failed。

---

## 実装決定ログ(実装中に plan を更新)

**decode は `serde_urlencoded::from_str::<ChartQuery>` を直接使う(当初案の `Query::try_from_uri` は不採用)。**
理由: `axum::extract::Query::try_from_uri` は内部で spec を `http::Uri` に載せ直す。`http::Uri` は
内部 offset が u16 のため `MAX_LEN = u16::MAX - 1 = 65534`(`http-1.4.2/src/uri/mod.rs:145`)の長さ上限を
持つ。これは create の受理域(body 100KiB / `shortlink_entry_bytes` 512KiB)**より低い**天井であり、
create で 200 になった大 spec が resolve で 500(`Uri::parse` の TooLong)に落ちる。render-by-id の要件は
「spec を URL に載せない=長さ天井を持ち込まない」なので、Uri を経由しない。
`serde_urlencoded::from_str` は axum の Query 抽出器が内部で使う parser そのもの
(`serde_urlencoded::Deserializer::new(form_urlencoded::parse(..))`)で、成功時は完全等価(axum は
`serde_path_to_error` でエラー文言を richにするだけ)。→ parse パリティを保ったまま長さ天井のみ除去。
`serde_urlencoded = "0.7"` を chart-server の直接依存に追加(既に axum 経由で lock 済み・単一版 0.7.1、
Cargo.lock は +1 行のみ)。

この決定に伴うテストの帰結:
- round-trip テスト(Task 1)の decode も `serde_urlencoded::from_str` に合わせる(handler が使わない
  `try_from_uri` を lock し続けると、将来 handler を try_from_uri に「単純化」して天井バグに戻す誘導になる)。
- 大 spec 受け入れテスト(Task 4)の判別条件は raw body 長ではなく **encoded query 長 > 65534**。
  これが try_from_uri 回帰への実質ガード(payload を縮めて 64KiB を下回ると判別力を失うため明示 assert)。
- create body / テストは `format:svg` を明示(`OutputFormat::default()` は `Png` で、既定だと content-type が
  image/png になり SVG assert が落ちるため)。

以下 Task 3 の handler コードは当初 `Uri` + `try_from_uri` で書かれているが、上記理由で
`serde_urlencoded::from_str` に置換して実装済み(imports からも `Query`/`Uri` を除去)。

---

### Task 1: build_query ⇄ Query::try_from_uri ラウンドトリップの回帰ロック

**なぜ最初か:** encode→store→decode が壊れると spec が静かに破損する(advisor 指摘の最重要盲点)。実装を触る前に「原値が復元される」ことをテストで固定する。`build_query` は既存で動くので、このテストは即 green(characterization / 回帰ロック)。

**Files:**
- Test: `crates/chart-server/src/handlers/shortlink.rs`(既存 `#[cfg(test)] mod http_tests` の**外**に、新しい `#[cfg(test)] mod roundtrip_tests` を追加)

**Step 1: ラウンドトリップテストを書く**

`shortlink.rs` 末尾に追加:

```rust
/// build_query が作る query 文字列を、GET /chart と同一の抽出器
/// (`Query::try_from_uri`)で parse し直したとき、c/f/w/h/bkg が原値復元される
/// ことを固定する。resolve の render-by-id 化はこの往復に依存する(advisor 指摘の盲点)。
#[cfg(test)]
mod roundtrip_tests {
    use super::build_query;
    use crate::handlers::chart::ChartQuery;
    use crate::render::OutputFormat;
    use axum::{extract::Query, http::Uri};

    #[test]
    fn build_query_round_trips_all_fields_and_formats() {
        // 構造文字 { } " : , / 空白 / 非ASCII(日本語) / + と & を値に含む。
        // + と & は form-encoding の古典的な罠(percent-encode されていれば復元される)。
        let spec = r#"{"type":"bar","data":{"labels":["a b","日本語","x+y&z"],"datasets":[{"data":[1,2,3]}]}}"#;

        for fmt in [
            OutputFormat::Svg,
            OutputFormat::Png,
            OutputFormat::Webp,
            OutputFormat::DataUri,
        ] {
            let q = build_query(spec, Some(640), Some(360), Some("hot+pink & white"), fmt);
            let uri: Uri = format!("/chart?{q}").parse().expect("query must form a valid URI");
            let Query(parsed) =
                Query::<ChartQuery>::try_from_uri(&uri).expect("stored query must parse as ChartQuery");

            assert_eq!(parsed.c.as_deref(), Some(spec), "spec round-trip failed for {fmt:?}");
            assert_eq!(parsed.w, Some(640), "w round-trip failed for {fmt:?}");
            assert_eq!(parsed.h, Some(360), "h round-trip failed for {fmt:?}");
            assert_eq!(
                parsed.bkg.as_deref(),
                Some("hot+pink & white"),
                "bkg round-trip failed for {fmt:?}"
            );
            assert_eq!(parsed.f, fmt, "format round-trip failed");
        }
    }

    /// None の w/h/bkg は query に出ず、parse 後も None のまま(Some("") にならない)。
    #[test]
    fn build_query_omits_absent_optionals() {
        let spec = r#"{"type":"bar"}"#;
        let q = build_query(spec, None, None, None, OutputFormat::Svg);
        let uri: Uri = format!("/chart?{q}").parse().unwrap();
        let Query(parsed) = Query::<ChartQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(parsed.c.as_deref(), Some(spec));
        assert_eq!(parsed.w, None);
        assert_eq!(parsed.h, None);
        assert_eq!(parsed.bkg, None);
        assert_eq!(parsed.f, OutputFormat::Svg);
    }
}
```

**Step 2: テスト実行(即 green を確認 = 往復が現に成立)**

Run: `cargo test -p chart-server roundtrip_tests`
Expected: PASS(2 tests)。**もし fail したら**、store 形式か encode/decode の非対称が既に存在する証拠なので、実装より先にそこを直す。

**Step 3: コミット**

```bash
git add crates/chart-server/src/handlers/shortlink.rs
git commit -m "test(chart-server): lock build_query round-trip via Query::try_from_uri (8tr.4)"
```

---

### Task 2: GET ハンドラ本体を `render_from_query` に切り出す(挙動不変のリファクタ)

**なぜ:** resolve と GET が **同一のレンダリング経路**を通ることを型で保証する(受理域一致・overrides 一回適用)。純粋な抽出で挙動は変わらないので、既存テストが全て green のままであることが受け入れ条件。

**Files:**
- Modify: `crates/chart-server/src/handlers/chart.rs:67-84`(`get_chart`)

**Step 1: `render_from_query` を追加し、`get_chart` を薄い委譲にする**

`chart.rs` の現 `get_chart`(67-84 行)を以下で置き換える:

```rust
pub async fn get_chart(
    State(state): State<AppState>,
    Query(q): Query<ChartQuery>,
    headers: HeaderMap,
) -> Response {
    render_from_query(q, headers, state).await
}

/// `ChartQuery` を受けてチャートをレンダリングする共有経路。
/// `GET /chart`(抽出器で `ChartQuery` を得る)と `GET /chart/s/{id}`
/// (store の query 文字列を同じ抽出器で parse する)の両方から呼ばれ、
/// overrides 適用とレンダリングの意味論を一致させる。
pub(crate) async fn render_from_query(
    q: ChartQuery,
    headers: HeaderMap,
    state: AppState,
) -> Response {
    let Some(c) = q.c else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "missing required parameter: c",
                "code": "MISSING_PARAM"
            })),
        )
            .into_response();
    };
    let json = apply_overrides(&c, q.w, q.h, q.bkg.as_deref());
    handle_render(json, q.f, "chartjs".to_string(), headers, state).await
}
```

**Step 2: ビルド + 全テストで挙動不変を確認**

Run: `cargo test -p chart-server`
Expected: PASS(51 unit + Task 1 の 2 + 5 integration。回帰ゼロ)。

**Step 3: コミット**

```bash
git add crates/chart-server/src/handlers/chart.rs
git commit -m "refactor(chart-server): extract render_from_query shared by GET and resolve (8tr.4)"
```

---

### Task 3: `get_shortlink` を render-by-id 化する

**Files:**
- Modify: `crates/chart-server/src/handlers/shortlink.rs`(imports / `get_shortlink` / 内部エラーヘルパ追加)
- Modify: `crates/chart-server/src/handlers/shortlink.rs`(既存テスト `resolve_success_sets_cache_control_from_config_ttl` を 200 前提に反転)

**Step 1: 失敗するテストを書く(既存テストを 200 前提に反転)**

`http_tests` 内の `resolve_success_sets_cache_control_from_config_ttl`(現在 `TEMPORARY_REDIRECT` を assert)の最後の 2 つの assert を置き換える:

```rust
        // render-by-id: リダイレクトではなく 200 でレンダリング済み SVG を返す。
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "headers={:?}",
            resp.headers()
        );
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "image/svg+xml; charset=utf-8"
        );
        // Cache-Control は shortlink TTL(config 3600)に結合(8tr.3)。
        assert_eq!(
            resp.headers().get("cache-control").unwrap(),
            "public, max-age=3600"
        );
        // ボディは非空のレンダリング済み SVG。
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(!bytes.is_empty(), "resolved SVG body must be non-empty");
        assert!(
            bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml"),
            "body should be SVG, got: {:?}",
            String::from_utf8_lossy(&bytes[..bytes.len().min(64)])
        );
```

**Step 2: テスト実行して fail を確認**

Run: `cargo test -p chart-server resolve_success_sets_cache_control_from_config_ttl`
Expected: FAIL(現状は 307 TEMPORARY_REDIRECT を返すため `assert_eq!(status, OK)` で落ちる)。

**Step 3: `get_shortlink` を実装する**

`shortlink.rs` の imports を更新(`Redirect` 削除、`Query`/`Uri`/`HeaderMap` 追加):

```rust
use crate::{backend::BackendError, render::OutputFormat, state::AppState};
use crate::handlers::chart::{ChartQuery, render_from_query};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use ulid::Ulid;
```

`get_shortlink` を以下で置き換える(`Ok(Some(..))` アームのみ変更、`Ok(None)`/`Err` アームは不変):

```rust
pub async fn get_shortlink(
    Path(id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    match state.store.get(&id).await {
        Ok(Some(query)) => {
            // store の query 文字列を GET /chart と同一の抽出器で parse する。
            // build_query が percent-encode 済みなので URI-safe。これで resolve の
            // parse が GET と厳密に同一になり、spec を URL に載せ直さずに直接レンダする。
            let uri = match format!("/chart?{query}").parse::<Uri>() {
                Ok(u) => u,
                Err(_) => return internal_error_no_store(),
            };
            let q = match Query::<ChartQuery>::try_from_uri(&uri) {
                Ok(Query(q)) => q,
                Err(_) => return internal_error_no_store(),
            };
            let mut resp = render_from_query(q, headers, state.clone()).await;
            // 成功(200)と 304(条件付き再検証)だけを CDN キャッシュ可能にし、
            // max-age を shortlink TTL に結合する(8tr.3)。エラー(400/415/500/503/504)は
            // no-store で、前段 CDN が誤って永続化しないようにする。
            let cc = if resp.status().is_success() || resp.status() == StatusCode::NOT_MODIFIED {
                state.shortlink_cache_control.clone()
            } else {
                no_store_cache_control()
            };
            resp.headers_mut().insert(header::CACHE_CONTROL, cc);
            resp
        }
        Ok(None) => {
            let mut resp = (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "short link not found",
                    "code": "NOT_FOUND"
                })),
            )
                .into_response();
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, no_store_cache_control());
            resp
        }
        // durable backend の一時障害: 503(FileShortlinkStore は I/O 失敗時に返す)。
        Err(err) => {
            eprintln!("Shortlink backend unavailable (resolve): {err}");
            let mut resp = (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "shortlink backend unavailable",
                    "code": "BACKEND_UNAVAILABLE"
                })),
            )
                .into_response();
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, no_store_cache_control());
            resp
        }
    }
}

/// store に入っている query 文字列が壊れていて parse 不能な場合の 500。
/// 自前 write なので通常到達しないが、防御的に no-store で返す。
fn internal_error_no_store() -> Response {
    let mut resp = (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": "corrupt short link entry",
            "code": "INTERNAL"
        })),
    )
        .into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, no_store_cache_control());
    resp
}
```

**Step 4: テスト実行して pass を確認**

Run: `cargo test -p chart-server resolve_success_sets_cache_control_from_config_ttl`
Expected: PASS。

**Step 5: 未使用 import/警告がないか確認**

Run: `cargo clippy -p chart-server --all-targets -- -D warnings`
Expected: 警告ゼロ(`Redirect` を消し忘れると unused import で落ちる)。

**Step 6: コミット**

```bash
git add crates/chart-server/src/handlers/shortlink.rs
git commit -m "feat(chart-server): render shortlink by id instead of 307 redirect (8tr.4)"
```

---

### Task 4: 受け入れテスト — 旧 307 で 414 になった大 spec が 200 になる

**なぜ:** これが 8tr.4 の存在理由。request-line 上限(概ね 8〜16KiB)を超える spec を create→resolve し、414 ではなく 200 でレンダリングされることを end-to-end で示す。

**Files:**
- Test: `crates/chart-server/src/handlers/shortlink.rs`(`http_tests` 内に追加)

**Step 1: テストを書く**

`http_tests` 内に追加:

```rust
    /// 旧 307 リダイレクト方式では request-line 上限(~8-16KiB)を超える大 spec は
    /// resolve 時に 414 になっていた。render-by-id では spec を URL に載せないので
    /// create の受理域(body 100KiB / entry_bytes)まで resolve も 200 で通る。
    #[tokio::test]
    async fn resolve_large_spec_renders_200_not_414() {
        let router = router_with_entry_bytes(512 * 1024).await;

        // ~40KiB のラベルを持つ棒グラフ。旧方式なら resolve の URL 長が
        // request-line 上限を大きく超えて 414 になっていたサイズ。
        let labels: Vec<String> = (0..2000).map(|i| format!("category-label-{i:05}")).collect();
        let data: Vec<u32> = (0..2000).collect();
        let chart = serde_json::json!({
            "type": "bar",
            "data": { "labels": labels, "datasets": [{ "data": data }] }
        });
        let create_body = serde_json::json!({ "chart": chart }).to_string();
        assert!(
            create_body.len() > 16_384,
            "test spec must exceed the old request-line ceiling, got {}B",
            create_body.len()
        );

        let (status, body) =
            status_and_body(router.clone().oneshot(create_request(&create_body)).await.unwrap()).await;
        assert_eq!(status, StatusCode::OK, "create should accept large spec: {body}");
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let url = json["url"].as_str().unwrap().to_string();

        let resp = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&url)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "resolve of large spec must render (200), not 414; headers={:?}",
            resp.headers()
        );
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "image/svg+xml; charset=utf-8"
        );
    }
```

**Step 2: テスト実行して pass を確認**

Run: `cargo test -p chart-server resolve_large_spec_renders_200_not_414`
Expected: PASS。

**Step 3: コミット**

```bash
git add crates/chart-server/src/handlers/shortlink.rs
git commit -m "test(chart-server): resolve renders large spec as 200 (no 414 ceiling) (8tr.4)"
```

---

### Task 5: ドキュメントの公開契約を更新する

**なぜ:** resolve の挙動が「307 redirect」から「直接レンダリング」に変わる。README と llms.txt がまだ「redirect」と明記しているので、公開契約の記述を実態に合わせる。

**Files:**
- Modify: `crates/chart-server/README.md:32`
- Modify: `crates/chart-server/llms.txt:32`

**Step 1: README.md の endpoint 表を更新**

`crates/chart-server/README.md:32`:

```text
| `GET`  | `/chart/s/{id}` | Redirect short link to `/chart?c=…` |
```
を
```text
| `GET`  | `/chart/s/{id}` | Render the chart for a short link server-side (no redirect; spec never re-enters the URL) |
```
に変更。

**Step 2: llms.txt を更新**

`crates/chart-server/llms.txt:32`:

```text
GET  /chart/s/<id> — redirects to /chart?c=…
```
を
```text
GET  /chart/s/<id> — renders the chart directly (server-side render-by-id; no redirect)
```
に変更。

**Step 3: 他に「redirect / 307」を残していないか確認**

Run: `grep -rn "redirect\|307\|TEMPORARY_REDIRECT" crates/chart-server/README.md crates/chart-server/llms.txt crates/chart-server/src/handlers/`
Expected: shortlink resolve を指す redirect 記述が残っていない(build_query のコメント等、無関係な箇所のみ)。

**Step 4: コミット**

```bash
git add crates/chart-server/README.md crates/chart-server/llms.txt
git commit -m "docs(chart-server): resolve renders by id, not 307 redirect (8tr.4)"
```

---

### Task 6: 品質ゲート(fmt / clippy / 全テスト)

**Files:** なし(検証のみ)

**Step 1: フォーマット**

Run: `cargo fmt -p chart-server`
（差分が出たら `git add -A && git commit -m "style(chart-server): cargo fmt (8tr.4)"`)

**Step 2: clippy(警告をエラー扱い)**

Run: `cargo clippy -p chart-server --all-targets -- -D warnings`
Expected: 警告ゼロ。

**Step 3: 全テスト**

Run: `cargo test -p chart-server`
Expected: 全 PASS(既存 51 + 新規 round-trip 2 + large-spec 1、integration 5、反転済み cache-control テスト含む)。

**Step 4: ワークスペース全体の回帰確認**

Run: `cargo test 2>&1 | tail -20`
Expected: ワークスペース全体で回帰ゼロ(バインディング等 fulgur-chart core に影響しないこと)。

**Step 5: 変更されていないことの最終確認**

Run: `git status`
Expected: clean(全コミット済み)。

---

## 完了条件(受け入れ)

- [ ] `GET /chart/s/{id}` が 307 ではなく 200 でレンダリング済みアーティファクトを返す。
- [ ] 大 spec(> 16KiB)が create→resolve で 414 にならず 200 になる。
- [ ] resolve 成功時の `Cache-Control` が `public, max-age=<shortlink_ttl>`、404/エラーは `no-store`。
- [ ] build_query ⇄ Query::try_from_uri のラウンドトリップが全 4 フォーマット・非ASCII・`+`/`&` で緑。
- [ ] README / llms.txt の「redirect」記述が実態に更新済み。
- [ ] `cargo clippy -D warnings` と `cargo test -p chart-server` が緑。
