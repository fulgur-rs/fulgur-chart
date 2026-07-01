# ShortlinkBackend trait 切り出し + lib/bin 分離 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart-server を bin-only から lib+bin に分割し、`ShortlinkBackend` trait(async/fallible)を seam として切り出して、外部 crate が durable backend を inject できるようにする(beads: fulgur-chart-8tr.1)。

**Architecture:** `src/lib.rs` を crate root にして全モジュールを集約。新規 `backend.rs` に `ShortlinkBackend` trait + `BackendError` を定義。in-memory `ShortlinkStore` が trait を実装(既存の byte-accounting ロジックを async method 内に保持)。`AppState.store` を `Arc<dyn ShortlinkBackend>` に、`build_router` 第2引数を `Arc<dyn ShortlinkBackend>` に変更。`main.rs` は in-memory を wire するだけの薄い composition root。

**Tech Stack:** Rust / axum 0.8 / tokio / async-trait 0.1 / dashmap。

**スコープ外(別 issue):** resolve の render-by-id 化(8tr.4)、durable 実装(8tr.6)、Layered backend(8tr.3)。resolve は従来どおり 307 → `/chart?{query}` を維持する。

**作業ディレクトリ:** `/home/ubuntu/fulgur-chart/.worktrees/shortlink-backend-trait`(このworktree内で作業する)。

**進め方の原則:** 各 Task の終わりで必ず `cargo build -p chart-server && cargo test -p chart-server` がグリーンになるように分割してある。型駆動リファクタなので Task 3 は複数ファイルを一括変更する(中間状態ではコンパイルが通らないため)。

---

## Task 1: lib/bin 分離(挙動不変のリストラクチャ)

**狙い:** モジュールを lib crate root(`src/lib.rs`)へ集約し、`main.rs` を薄い bin に降格する。trait はまだ導入しない。これは純粋な構造変更で、35 テストがそのまま通るはず。

**Files:**
- Create: `crates/chart-server/src/lib.rs`
- Modify: `crates/chart-server/src/main.rs`(全面置換)

**Step 1: `src/lib.rs` を作成**

```rust
//! chart-server library crate.
//!
//! HTTP rendering server の本体。bin(`main.rs`)は薄い composition root として
//! このライブラリの `build_router` を呼ぶだけ。外部 crate はこのライブラリに
//! 依存し、`ShortlinkBackend` を実装した durable backend を `build_router` に
//! inject できる。

mod config;
mod handlers;
mod render;
mod response;
mod server;
mod state;
mod store;

pub use config::Config;
pub use server::build_router;
pub use store::ShortlinkStore;
```

(注: モジュールは private `mod` のままにし、外部に出す item だけ `pub use` する。公開 API 面を最小化する。`handlers`/`render`/`state` 等は `crate::` 経由の intra-crate 参照のみで使われており private で問題ない。)

**Step 2: `src/main.rs` を全面置換**

```rust
use std::net::SocketAddr;

use chart_server::{Config, ShortlinkStore, build_router};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cfg = Config::parse();
    let store = ShortlinkStore::new(
        cfg.shortlink_limit,
        cfg.shortlink_max_bytes,
        cfg.shortlink_entry_bytes,
    );
    // Railway は $PORT を inject する。FULGUR_PORT より優先して読む。
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(cfg.port);
    // タプル形式で bind すれば IPv6 アドレス（::1 等）でも正しく動作する。
    let listener = tokio::net::TcpListener::bind((cfg.host.as_str(), port))
        .await
        .unwrap();
    println!("chart-server listening on {}:{}", cfg.host, port);
    axum::serve(
        listener,
        build_router(&cfg, store).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
```

**Step 3: ビルド & テストでグリーンを確認**

Run: `cargo build -p chart-server && cargo test -p chart-server`
Expected: コンパイル成功 + `test result: ok. 35 passed`。

**Step 4: Commit**

```bash
git add crates/chart-server/src/lib.rs crates/chart-server/src/main.rs
git commit -m "refactor(chart-server): split bin into lib + thin composition root"
```

---

## Task 2: async-trait 依存追加 + `backend.rs`(trait + BackendError)

**狙い:** `ShortlinkBackend` trait と `BackendError` を定義する。まだどこからも使わないが、pub 再エクスポートするのでコンパイルは通る。

**Files:**
- Modify: `crates/chart-server/Cargo.toml`
- Create: `crates/chart-server/src/backend.rs`
- Modify: `crates/chart-server/src/lib.rs`

**Step 1: `Cargo.toml` の `[dependencies]` に async-trait を追加**

`dashmap = "6"` の行の近く(アルファベット順は厳密でなくてよい)に追加:

```toml
async-trait = "0.1"
```

(Cargo.lock には既に async-trait 0.1.89 が transitive で入っているのでバージョン解決は変わらない。)

**Step 2: `src/backend.rs` を作成**

```rust
use std::fmt;

use async_trait::async_trait;

/// Shortlink backend の失敗理由。
///
/// `TooLarge` / `Full` は容量系の拒否で、それぞれ HTTP 413 / 503 にマップする。
/// `Unavailable` は durable backend の I/O 障害用(→ 5xx)。in-memory 実装は
/// 決して `Unavailable` を返さないが、durable adapter(8tr.6 等)が trait
/// シグネチャを変えずに使えるよう、最初からこの variant を含めておく。
#[derive(Debug)]
pub enum BackendError {
    /// 単一エントリが per-entry バイト上限を超過（→ 413）。
    TooLarge,
    /// ストアが満杯（件数 or 集約バイト上限。→ 503）。
    Full,
    /// durable backend の一時的な I/O 障害（→ 503）。in-memory は返さない。
    Unavailable(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::TooLarge => f.write_str("shortlink entry is too large"),
            BackendError::Full => f.write_str("shortlink store is full"),
            BackendError::Unavailable(e) => write!(f, "shortlink backend unavailable: {e}"),
        }
    }
}

impl std::error::Error for BackendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BackendError::Unavailable(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

/// Shortlink の保存・解決を抽象化する backend。
///
/// `Arc<dyn ShortlinkBackend>` として `AppState` に保持するため `Send + Sync`。
/// メソッドは durable 実装(I/O を伴う)を見越して async + fallible。in-memory
/// 実装は await を持たず `Unavailable` も返さないが、同じ seam を共有する。
#[async_trait]
pub trait ShortlinkBackend: Send + Sync {
    /// `id` に `query` を保存する。容量超過時は `TooLarge` / `Full`。
    async fn insert(&self, id: String, query: String) -> Result<(), BackendError>;

    /// `id` に対応する query を返す。未登録は `Ok(None)`、I/O 障害は `Err`。
    async fn get(&self, id: &str) -> Result<Option<String>, BackendError>;
}
```

**Step 3: `src/lib.rs` に backend を登録 + 再エクスポート**

`mod config;` の前に `mod backend;` を追加し、`pub use` を1行追加:

```rust
mod backend;
mod config;
// ... 既存の mod 群 ...

pub use backend::{BackendError, ShortlinkBackend};
pub use config::Config;
pub use server::build_router;
pub use store::ShortlinkStore;
```

**Step 4: ビルドでグリーンを確認**

Run: `cargo build -p chart-server && cargo test -p chart-server`
Expected: コンパイル成功 + `35 passed`(挙動は不変)。

**Step 5: Commit**

```bash
git add crates/chart-server/Cargo.toml crates/chart-server/src/backend.rs crates/chart-server/src/lib.rs
git commit -m "feat(chart-server): define ShortlinkBackend trait and BackendError"
```

---

## Task 3: trait を配線に通す(atomic な型駆動変更)

**狙い:** `ShortlinkStore` に trait を実装し、`InsertError` を `BackendError` に統合、`AppState.store` / `build_router` を `Arc<dyn ShortlinkBackend>` 化、handler を `.await` 化する。これらは相互依存するため一括で行う(中間状態はコンパイルが通らない)。

**Files:**
- Modify: `crates/chart-server/src/store.rs`
- Modify: `crates/chart-server/src/state.rs`
- Modify: `crates/chart-server/src/server.rs`
- Modify: `crates/chart-server/src/handlers/shortlink.rs`
- Modify: `crates/chart-server/src/handlers/mcp.rs`(test のみ)
- Modify: `crates/chart-server/src/main.rs`

**Step 1: `store.rs` — InsertError 削除 + trait 実装 + テスト async 化**

`pub enum InsertError {...}` を削除。`impl ShortlinkStore { pub fn insert/get }` の本体ロジックを `impl ShortlinkBackend for ShortlinkStore` の async method に移す(`new` は inherent のまま残す)。エラー variant を `InsertError::*` → `BackendError::*` に置換。

ファイル冒頭の use を更新:

```rust
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::backend::{BackendError, ShortlinkBackend};
```

`struct ShortlinkStore`(`#[derive(Clone)]` + フィールド)はそのまま。`impl ShortlinkStore` には `new` だけ残す:

```rust
impl ShortlinkStore {
    pub fn new(entry_limit: usize, max_bytes: usize, entry_bytes: usize) -> Self {
        Self {
            map: Arc::new(DashMap::new()),
            count: Arc::new(AtomicUsize::new(0)),
            bytes: Arc::new(AtomicUsize::new(0)),
            entry_limit,
            max_bytes,
            entry_bytes,
        }
    }
}
```

trait 実装(insert 本体は既存ロジックをそのまま移植、`InsertError`→`BackendError`):

```rust
#[async_trait]
impl ShortlinkBackend for ShortlinkStore {
    async fn insert(&self, id: String, query: String) -> Result<(), BackendError> {
        let query_len = query.len();
        // per-entry 上限: このペイロード単体が大きすぎる。再送しても無駄なので即拒否。
        if query_len > self.entry_bytes {
            return Err(BackendError::TooLarge);
        }

        // 集約バイト/件数は global atomic で会計する（reserve-then-rollback）。
        match self.map.entry(id) {
            dashmap::Entry::Occupied(mut entry) => {
                let old_len = entry.get().len();
                if query_len > old_len {
                    let additional = query_len - old_len;
                    let prev = self.bytes.fetch_add(additional, Ordering::AcqRel);
                    if prev.saturating_add(additional) > self.max_bytes {
                        self.bytes.fetch_sub(additional, Ordering::AcqRel);
                        return Err(BackendError::Full);
                    }
                }
                entry.insert(query);
                if old_len > query_len {
                    self.bytes.fetch_sub(old_len - query_len, Ordering::AcqRel);
                }
                Ok(())
            }
            dashmap::Entry::Vacant(entry) => {
                let prev_bytes = self.bytes.fetch_add(query_len, Ordering::AcqRel);
                if prev_bytes.saturating_add(query_len) > self.max_bytes {
                    self.bytes.fetch_sub(query_len, Ordering::AcqRel);
                    return Err(BackendError::Full);
                }
                let prev = self.count.fetch_add(1, Ordering::AcqRel);
                if prev >= self.entry_limit {
                    self.count.fetch_sub(1, Ordering::AcqRel);
                    self.bytes.fetch_sub(query_len, Ordering::AcqRel);
                    Err(BackendError::Full)
                } else {
                    entry.insert(query);
                    Ok(())
                }
            }
        }
    }

    async fn get(&self, id: &str) -> Result<Option<String>, BackendError> {
        Ok(self.map.get(id).map(|v| v.clone()))
    }
}
```

`#[cfg(test)] mod tests` を全面置換(6本を async 化、`assert_eq!`→`matches!` / `.await.unwrap()`):

```rust
#[cfg(test)]
mod tests {
    use super::ShortlinkStore;
    use crate::backend::{BackendError, ShortlinkBackend};

    #[tokio::test]
    async fn accepts_entry_within_limits() {
        let store = ShortlinkStore::new(10, 1000, 100);
        let val = "x".repeat(50);
        assert!(store.insert("a".into(), val.clone()).await.is_ok());
        assert_eq!(store.get("a").await.unwrap(), Some(val));
    }

    #[tokio::test]
    async fn rejects_entry_exceeding_per_entry_byte_limit() {
        let store = ShortlinkStore::new(10, 10_000, 4);
        assert!(matches!(
            store.insert("big".into(), "12345".into()).await,
            Err(BackendError::TooLarge)
        ));
        assert!(store.get("big").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_when_aggregate_byte_budget_is_full() {
        let store = ShortlinkStore::new(10, 8, 1000);
        assert!(store.insert("a".into(), "1234".into()).await.is_ok());
        assert!(store.insert("b".into(), "5678".into()).await.is_ok());
        assert!(matches!(
            store.insert("c".into(), "9".into()).await,
            Err(BackendError::Full)
        ));
        assert!(store.get("c").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_when_entry_count_is_full() {
        let store = ShortlinkStore::new(2, 10_000, 1000);
        assert!(store.insert("a".into(), "x".into()).await.is_ok());
        assert!(store.insert("b".into(), "y".into()).await.is_ok());
        assert!(matches!(
            store.insert("c".into(), "z".into()).await,
            Err(BackendError::Full)
        ));
        assert!(store.get("c").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn overwriting_same_id_does_not_double_count_bytes() {
        let store = ShortlinkStore::new(10, 8, 1000);
        assert!(store.insert("a".into(), "1234".into()).await.is_ok());
        assert!(store.insert("a".into(), "1234".into()).await.is_ok());
        assert!(store.insert("b".into(), "5678".into()).await.is_ok());
        assert!(store.get("b").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn overwriting_with_invalid_query_retains_old_value() {
        let store = ShortlinkStore::new(10, 8, 5);
        assert!(store.insert("a".into(), "1234".into()).await.is_ok());

        // 1. per-entry 上限超過による上書き失敗 (TooLarge) → 古い値を保持。
        assert!(matches!(
            store.insert("a".into(), "123456".into()).await,
            Err(BackendError::TooLarge)
        ));
        assert_eq!(store.get("a").await.unwrap(), Some("1234".into()));

        // 2. 正常な上書き（5 バイト）。
        assert!(store.insert("a".into(), "12345".into()).await.is_ok());
        assert_eq!(store.get("a").await.unwrap(), Some("12345".into()));

        // 別エントリ "b" を 3 バイトで挿入 → 合計 8 バイトで満杯。
        assert!(store.insert("b".into(), "123".into()).await.is_ok());

        // 3. 集約上限超過による上書き失敗 (Full) → 古い値を保持。
        assert!(matches!(
            store.insert("b".into(), "1234".into()).await,
            Err(BackendError::Full)
        ));
        assert_eq!(store.get("b").await.unwrap(), Some("123".into()));

        // ロールバックが正しく行われ、バイトがリークしていないこと。
        assert!(store.insert("b".into(), "12".into()).await.is_ok());
        assert_eq!(store.get("b").await.unwrap(), Some("12".into()));
    }
}
```

**Step 2: `state.rs` — store を Arc<dyn> に**

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::backend::ShortlinkBackend;
use crate::render::{Compression, WebpPolicy};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn ShortlinkBackend>,
    pub semaphore: Arc<Semaphore>,
    pub render_timeout_ms: u64,
    /// サーバ全体に適用する PNG 圧縮プリセット（起動時設定）。
    pub png_compression: Compression,
    /// WebP 出力のポリシー（有効/無効・面積予算。起動時設定）。
    pub webp: WebpPolicy,
}
```

(`use crate::store::ShortlinkStore;` は削除。)

**Step 3: `server.rs` — build_router シグネチャ + テストの Arc 化**

冒頭の use(21-26行目あたり)を変更: `store::ShortlinkStore` を外し `backend::ShortlinkBackend` を足す。

```rust
use crate::{
    backend::ShortlinkBackend,
    config::Config,
    handlers::{chart, mcp, meta, openapi::ApiDoc, shortlink, validate},
    state::AppState,
};
```

シグネチャ変更(28行目):

```rust
pub fn build_router(cfg: &Config, store: Arc<dyn ShortlinkBackend>) -> Router {
```

(`AppState { store, ... }` の行はそのままで OK — store の型が変わっただけ。)

テストモジュール内の2箇所(`restricted_cors_router` / `router_with_compression`)の `build_router(&cfg, ShortlinkStore::new(...))` を `Arc::new(...)` で包む。`use super::*;` で `Arc` は既にスコープにある(`server.rs` 冒頭の `use std::sync::Arc;`)。`use crate::store::ShortlinkStore;`(108行目)はそのまま残す。

```rust
        build_router(
            &cfg,
            Arc::new(ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024)),
        )
```

(2箇所とも同様に `Arc::new(...)` で包む。)

**Step 4: `handlers/shortlink.rs` — await + BackendError arm + get の Result 化 + http_tests の Arc 化**

冒頭 use(1行目)を変更:

```rust
use crate::{backend::BackendError, render::OutputFormat, state::AppState};
```

`post_create` の match(82行目〜)を `.await` + `BackendError` arm に:

```rust
    match state.store.insert(id, query).await {
        Ok(()) => (StatusCode::OK, Json(json!({"url": url}))).into_response(),
        // 単一ペイロードが per-entry 上限超過: 再送しても無駄なので 413。
        Err(BackendError::TooLarge) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "chart payload is too large for a short link",
                "code": "PAYLOAD_TOO_LARGE"
            })),
        )
            .into_response(),
        // 件数 or 集約バイトが満杯: 一時的な拒否なので 503。
        Err(BackendError::Full) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "shortlink store is full",
                "code": "STORE_FULL"
            })),
        )
            .into_response(),
        // durable backend の一時障害: 503（in-memory では発生しない）。
        Err(BackendError::Unavailable(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "shortlink backend unavailable",
                "code": "BACKEND_UNAVAILABLE"
            })),
        )
            .into_response(),
    }
```

`get_shortlink`(105行目〜)を `.await` + `Result` match に:

```rust
pub async fn get_shortlink(Path(id): Path<String>, State(state): State<AppState>) -> Response {
    match state.store.get(&id).await {
        Ok(Some(query)) => Redirect::temporary(&format!("/chart?{query}")).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "short link not found",
                "code": "NOT_FOUND"
            })),
        )
            .into_response(),
        // durable backend の一時障害: 503（in-memory では発生しない）。
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "shortlink backend unavailable",
                "code": "BACKEND_UNAVAILABLE"
            })),
        )
            .into_response(),
    }
}
```

`http_tests` モジュール: import に `use std::sync::Arc;` を足し、`router_with_store` 内の `build_router(&cfg, store)` を `build_router(&cfg, Arc::new(store))` に:

```rust
    use std::sync::Arc;
    // ... 既存 use ...
    fn router_with_store(store: ShortlinkStore) -> Router {
        let cfg = Config { /* 既存のまま */ };
        build_router(&cfg, Arc::new(store))
    }
```

(`use crate::store::ShortlinkStore;` はそのまま。)

**Step 5: `handlers/mcp.rs` の test — store を Arc 化**

`test_app`(355行目)の `store: ShortlinkStore::new(...)` を `Arc::new(...)` で包む。`use std::sync::Arc;` は既に test モジュール冒頭(348行目)にある。

```rust
        let state = AppState {
            store: Arc::new(ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024)),
            // ... 既存のまま ...
        };
```

**Step 6: `main.rs` — store を Arc 化**

```rust
    let store = std::sync::Arc::new(ShortlinkStore::new(
        cfg.shortlink_limit,
        cfg.shortlink_max_bytes,
        cfg.shortlink_entry_bytes,
    ));
```

(`build_router(&cfg, store)` はそのまま。`Arc<ShortlinkStore>` → `Arc<dyn ShortlinkBackend>` は呼び出し時に自動 unsizing される。)

**Step 7: ビルド & テスト & clippy でグリーンを確認**

Run:
```bash
cargo build -p chart-server && \
cargo test -p chart-server && \
cargo clippy -p chart-server --all-targets -- -D warnings
```
Expected: コンパイル成功 + `35 passed`(本数不変) + clippy warning ゼロ。

**Step 8: Commit**

```bash
git add crates/chart-server/src
git commit -m "refactor(chart-server): store AppState behind Arc<dyn ShortlinkBackend>"
```

---

## Task 4: 公開 API の外部 crate 利用を検証する統合テスト

**狙い:** 受け入れ基準「外部 crate が `Arc<dyn ShortlinkBackend>` を `build_router` に inject できる」を、別 crate としてコンパイルされる統合テスト(`tests/`)で実証する。

**Files:**
- Create: `crates/chart-server/tests/public_api.rs`

**Step 1: 統合テストを作成**

```rust
//! 公開 API が外部 crate と同じ経路(`chart_server::...` の再エクスポートのみ)で
//! 使えること、特に `ShortlinkBackend` を実装した任意の backend を
//! `build_router` に inject できることを検証する。

use std::sync::Arc;

use async_trait::async_trait;
use chart_server::{BackendError, Config, ShortlinkBackend, ShortlinkStore, build_router};
use clap::Parser;

/// 外部 crate 側で定義しうる最小の backend 実装。
struct NoopBackend;

#[async_trait]
impl ShortlinkBackend for NoopBackend {
    async fn insert(&self, _id: String, _query: String) -> Result<(), BackendError> {
        Ok(())
    }
    async fn get(&self, _id: &str) -> Result<Option<String>, BackendError> {
        Ok(None)
    }
}

#[test]
fn external_backend_can_be_injected_into_build_router() {
    // clap のデフォルト値で Config を構築(引数なし起動相当)。
    let cfg = Config::parse_from(["chart-server"]);

    // 1. 外部実装した backend を inject できる。
    let _router = build_router(&cfg, Arc::new(NoopBackend));

    // 2. OSS デフォルトの in-memory backend も同じ seam で渡せる。
    let _router2 = build_router(&cfg, Arc::new(ShortlinkStore::new(10, 1024, 256)));
}
```

**Step 2: テスト実行**

Run: `cargo test -p chart-server --test public_api`
Expected: `test result: ok. 1 passed`。

**Step 3: Commit**

```bash
git add crates/chart-server/tests/public_api.rs
git commit -m "test(chart-server): verify external backend injection via public API"
```

---

## Task 5: 最終検証(受け入れ基準の確認)

**Files:** なし(検証のみ)

**Step 1: フル検証**

Run:
```bash
cargo build -p chart-server && \
cargo test -p chart-server && \
cargo clippy -p chart-server --all-targets -- -D warnings
```
Expected:
- lib + bin がコンパイルできる。
- 全テスト通過(既存 35 + 新規統合テスト 1 = 36、内訳は実行ログで確認)。
- clippy warning ゼロ。

**Step 2: 受け入れ基準の対応確認(目視)**

- [ ] `AppState.store` の型が `Arc<dyn ShortlinkBackend>` (`state.rs`)。
- [ ] `build_router` のシグネチャが `(&Config, Arc<dyn ShortlinkBackend>)` (`server.rs`)。
- [ ] `main.rs` は in-memory を wire するだけ。
- [ ] `chart_server::{ShortlinkBackend, BackendError, Config, build_router, ShortlinkStore}` が外部から使える(Task 4 で実証)。
- [ ] HTTP マッピング: 413(TooLarge)/503(Full)/503(Unavailable)。
- [ ] `InsertError` が削除され `BackendError` に統合済み。
- [ ] resolve は 307 → `/chart?{query}` のまま(render-by-id 化していない)。

**Step 3: (Task 5 自体に commit は不要 — 全コミットは Task 1〜4 で完了済み)**

---

## 完了後

- `superpowers:verification-before-completion` でフル検証を実施。
- `superpowers:finishing-a-development-branch` でブランチ完了処理(PR 作成等)。
- 8tr.1 を close。8tr.2(既存 in-memory store を ShortlinkBackend に適合)は本実装が実質包含するため redundant として close 判断する。
