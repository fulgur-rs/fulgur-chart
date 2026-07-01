# Shortlink TTL/max-age カップリング Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `/chart/s/{id}` の解決レスポンスに `Cache-Control` ヘッダを付与し、前段 CDN が「保証リンク寿命 L」を尊重してキャッシュ・negative-cache するようにする(beads issue `fulgur-chart-8tr.3`)。

**Architecture:** `Config` に `shortlink_ttl_seconds`(env: `FULGUR_SHORTLINK_TTL_SECONDS`, default 86400)を追加し `AppState` まで配線する。`handlers/shortlink.rs::get_shortlink` の3分岐(307成功 / 404未検出 / 503障害)それぞれに `Cache-Control` を付与する。`ShortlinkStore` のストレージ層(byte/件数上限のaccounting・eviction)や `Layered backend` 合成には一切手を入れない(design で不採用と確定済み)。`/chart` 直接レンダリング用の既存 `cache_headers()`(86400固定)も変更しない。

**Tech Stack:** Rust, axum, tokio, clap(Config)。既存パターンを踏襲するのみで新規依存追加なし。

---

## 前提

- beads issue: `fulgur-chart-8tr.3`(design/acceptance 済み)
- worktree: `/home/ubuntu/fulgur-chart/.worktrees/8tr3-shortlink-ttl-cache-control`(ブランチ `feat/8tr3-shortlink-ttl-cache-control`)
- ベースライン確認済み: `cargo test -p chart-server` で 38 tests all green

---

### Task 1: Config に `shortlink_ttl_seconds` を追加

**Files:**
- Modify: `crates/chart-server/src/config.rs:35`(`shortlink_entry_bytes` の直後に追加)

**Step 1: フィールドを追加**

`crates/chart-server/src/config.rs` の `shortlink_entry_bytes` フィールド定義の直後(36行目、`cors_origins` の前)に以下を追加する:

```rust
    /// shortlink の保証有効期限（秒）。リンクは少なくともこの期間は解決可能で
    /// あることを約束する下限保証（この時刻ちょうどに実データが削除される
    /// わけではない）。`/chart/s/{id}` 解決成功時レスポンスの
    /// `Cache-Control: max-age` に使い、前段 CDN が保証期間を超えて古い
    /// 解決結果を配信しないようにする。
    #[arg(long, env = "FULGUR_SHORTLINK_TTL_SECONDS", default_value_t = 86_400)]
    pub shortlink_ttl_seconds: u64,
```

**Step 2: ビルドで確認(コンパイルエラーが出るはず)**

Run: `cargo build -p chart-server 2>&1 | grep -A3 "missing field"`
Expected: `Config { ... }` を直書きしている3箇所(`server.rs` 2箇所, `handlers/shortlink.rs` 1箇所)で `missing field \`shortlink_ttl_seconds\`` エラーが出る。これは Task 2 で解消する。

**Step 3: コミット(Task 2 とまとめてコミットする)**

このタスク単体ではビルドが壊れた状態になるため、コミットは Task 2 完了後にまとめて行う。

---

### Task 2: AppState への配線 + 既存テストの Config リテラル更新

**Files:**
- Modify: `crates/chart-server/src/state.rs:8-16`
- Modify: `crates/chart-server/src/server.rs:31-40`(`build_router` 内の `AppState` 構築)
- Modify: `crates/chart-server/src/server.rs:115-130`(`restricted_cors_router` テストヘルパー)
- Modify: `crates/chart-server/src/server.rs:138-153`(`router_with_compression` テストヘルパー)
- Modify: `crates/chart-server/src/handlers/shortlink.rs:158-173`(`router_with_store` テストヘルパー)

**Step 1: `AppState` にフィールドを追加**

`crates/chart-server/src/state.rs` を編集:

```rust
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn ShortlinkBackend>,
    pub semaphore: Arc<Semaphore>,
    pub render_timeout_ms: u64,
    /// サーバ全体に適用する PNG 圧縮プリセット（起動時設定）。
    pub png_compression: Compression,
    /// WebP 出力のポリシー（有効/無効・面積予算。起動時設定）。
    pub webp: WebpPolicy,
    /// shortlink 解決成功時の Cache-Control max-age に使う保証有効期限（秒）。
    pub shortlink_ttl_seconds: u64,
}
```

**Step 2: `build_router` で配線**

`crates/chart-server/src/server.rs` の `AppState { ... }` 構築(28-40行目付近)に1行追加:

```rust
    let state = AppState {
        store,
        semaphore,
        render_timeout_ms: cfg.render_timeout_ms,
        png_compression: cfg.png_compression,
        webp: crate::render::WebpPolicy {
            enabled: cfg.webp_enabled,
            max_area: cfg.max_webp_area,
        },
        shortlink_ttl_seconds: cfg.shortlink_ttl_seconds,
    };
```

**Step 3: 既存テストの `Config { ... }` リテラルを更新**

`server.rs` の `restricted_cors_router()` と `router_with_compression()` 内の `Config { ... }` それぞれに `shortlink_entry_bytes` の直後、`cors_origins` の前に1行追加:

```rust
            shortlink_ttl_seconds: 86_400,
```

`handlers/shortlink.rs` の `router_with_store()` 内の `Config { ... }` にも同様に追加。

**Step 4: ビルド・テストで確認**

Run: `cargo build -p chart-server && cargo test -p chart-server`
Expected: コンパイルが通り、既存38 testsが全てpassする(新規テストはまだ追加していない)。

**Step 5: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/8tr3-shortlink-ttl-cache-control
git add crates/chart-server/src/config.rs crates/chart-server/src/state.rs crates/chart-server/src/server.rs crates/chart-server/src/handlers/shortlink.rs
git commit -m "feat(chart-server): add shortlink_ttl_seconds config"
```

---

### Task 3: resolve 成功時に `Cache-Control: public, max-age=<L>` を付与(TDD)

**Files:**
- Modify: `crates/chart-server/src/handlers/shortlink.rs`(`get_shortlink` 関数、import、テストヘルパー、テスト)

**Step 1: 失敗するテストを書く**

`crates/chart-server/src/handlers/shortlink.rs` の `http_tests` モジュールに、`router_with_store` の直後に新しいヘルパーを追加:

```rust
    /// shortlink_ttl_seconds を明示的に指定できる router ヘルパー
    /// （config駆動であることをdefault値(86400)と異なる値で検証するため）。
    fn router_with_store_and_ttl(store: ShortlinkStore, ttl: u64) -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 4,
            max_body_size: 102_400,
            render_timeout_ms: 1000,
            shortlink_limit: 100,
            shortlink_max_bytes: 128 * 1024 * 1024,
            shortlink_entry_bytes: 512 * 1024,
            shortlink_ttl_seconds: ttl,
            cors_origins: "*".into(),
            rate_limit: 0,
            log_level: "info".into(),
            png_compression: Compression::default(),
            webp_enabled: false,
            max_webp_area: fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS,
        };
        build_router(&cfg, Arc::new(store))
    }
```

(`Compression` は `router_with_store` 側で既に import 済みか確認し、未 import なら `use crate::render::Compression;` を http_tests モジュール先頭に追加する。)

続けて、ファイル末尾のテスト群に以下を追加:

```rust
    /// resolve成功時は Cache-Control: public, max-age=<config値> が付く。
    /// default(86400)と異なる値(3600)を使い、handlerがハードコードでなく
    /// config駆動であることを検証する。
    #[tokio::test]
    async fn resolve_success_sets_cache_control_from_config_ttl() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store_and_ttl(store, 3600);

        let create_body =
            r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let (status, body) = status_and_body(
            router
                .clone()
                .oneshot(create_request(create_body))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={body}");
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
            StatusCode::TEMPORARY_REDIRECT,
            "headers={:?}",
            resp.headers()
        );
        assert_eq!(
            resp.headers().get("cache-control").unwrap(),
            "public, max-age=3600"
        );
    }
```

**Step 2: テストが失敗することを確認**

Run: `cargo test -p chart-server resolve_success_sets_cache_control_from_config_ttl -- --nocapture`
Expected: FAIL(`cache-control` ヘッダが存在せず `.unwrap()` で panic、または assert_eq 不一致)

**Step 3: 最小実装**

`crates/chart-server/src/handlers/shortlink.rs` 冒頭の import に `header` を追加:

```rust
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
```

`get_shortlink` の `Ok(Some(query))` 分岐を書き換え:

```rust
        Ok(Some(query)) => {
            let mut resp = Redirect::temporary(&format!("/chart?{query}")).into_response();
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                format!("public, max-age={}", state.shortlink_ttl_seconds)
                    .parse()
                    .unwrap(),
            );
            resp
        }
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p chart-server resolve_success_sets_cache_control_from_config_ttl`
Expected: PASS

**Step 5: コミット**

```bash
git add crates/chart-server/src/handlers/shortlink.rs
git commit -m "feat(chart-server): set Cache-Control max-age on shortlink resolve success"
```

---

### Task 4: resolve 404(未検出)時に `Cache-Control: no-store` を付与(TDD)

**Files:**
- Modify: `crates/chart-server/src/handlers/shortlink.rs`

**Step 1: 失敗するテストを書く**

`http_tests` モジュール末尾に追加:

```rust
    /// 未検出(404)は Cache-Control: no-store。前段CDNのnegative-cacheが
    /// LBハズレ由来の一時的な404を永続化させないようにするため。
    #[tokio::test]
    async fn resolve_not_found_sets_no_store_cache_control() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store(store);

        let resp = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/chart/s/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
    }
```

**Step 2: テストが失敗することを確認**

Run: `cargo test -p chart-server resolve_not_found_sets_no_store_cache_control -- --nocapture`
Expected: FAIL(ヘッダなし)

**Step 3: 最小実装**

`get_shortlink` の `Ok(None)` 分岐を書き換え:

```rust
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
                .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            resp
        }
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p chart-server resolve_not_found_sets_no_store_cache_control`
Expected: PASS

**Step 5: コミット**

```bash
git add crates/chart-server/src/handlers/shortlink.rs
git commit -m "feat(chart-server): set Cache-Control no-store on shortlink 404"
```

---

### Task 5: resolve 503(backend障害)時に `Cache-Control: no-store` を付与(TDD)

**Files:**
- Modify: `crates/chart-server/src/handlers/shortlink.rs`(実装)
- Modify: `crates/chart-server/tests/public_api.rs`(テスト。`UnavailableBackend` スタブは既存のものを流用)

**Step 1: 失敗するテストを書く**

`crates/chart-server/tests/public_api.rs` の `resolve_returns_503_backend_unavailable_when_backend_errors` の直後に追加:

```rust
/// backend が `Unavailable` を返すときの 503 応答は Cache-Control: no-store
/// （一時障害を前段CDNに誤ってキャッシュさせないため）。
#[tokio::test]
async fn resolve_returns_no_store_cache_control_when_backend_unavailable() {
    let cfg = default_config();
    let router = build_router(&cfg, Arc::new(UnavailableBackend));
    let req = Request::builder()
        .method("GET")
        .uri("/chart/s/deadbeef")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
}
```

**Step 2: テストが失敗することを確認**

Run: `cargo test -p chart-server --test public_api resolve_returns_no_store_cache_control_when_backend_unavailable -- --nocapture`
Expected: FAIL(ヘッダなし)

**Step 3: 最小実装**

`get_shortlink` の `Err(err)` 分岐を書き換え:

```rust
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
                .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            resp
        }
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p chart-server --test public_api resolve_returns_no_store_cache_control_when_backend_unavailable`
Expected: PASS

**Step 5: コミット**

```bash
git add crates/chart-server/src/handlers/shortlink.rs crates/chart-server/tests/public_api.rs
git commit -m "feat(chart-server): set Cache-Control no-store on shortlink resolve 503"
```

---

### Task 6: 最終検証

**Step 1: フルテストスイート**

Run: `cargo test -p chart-server`
Expected: 全テスト(既存38 + 新規3 = 41 tests)が green。

**Step 2: clippy**

Run: `cargo clippy -p chart-server -- -D warnings`
Expected: warning/error なし。

**Step 3: フォーマット確認**

Run: `cargo fmt -p chart-server -- --check`
Expected: 差分なし(あれば `cargo fmt -p chart-server` を実行してから再コミット)。

**Step 4: acceptance criteria との突合**

`bd show fulgur-chart-8tr.3` の ACCEPTANCE CRITERIA を1件ずつ確認し、全て満たしていることを確認する。

---

## Execution Handoff

このプランの実行方式を選んでください:

**1. Subagent-Driven(この session)** — タスクごとにfresh subagentを起動、タスク間でレビュー

**2. Parallel Session(別session)** — worktree内で新しいsessionを開き、executing-plansでバッチ実行
