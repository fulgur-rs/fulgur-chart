# FileShortlinkStore Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart-server の shortlink backend を、再起動/デプロイをまたいで永続化する filesystem 実装 `FileShortlinkStore` に置き換える（in-memory `ShortlinkStore` を撤去し唯一の backend にする）。

**Architecture:** `ShortlinkBackend` trait（既存 seam）の filesystem 実装を追加。1 エントリ = 1 ファイル（ファイル名 = id、内容 = query 文字列）で `root` 配下に保存。id 検証（26 文字・ASCII 英数字）を**パス構築より前**に行い path traversal / panic を防ぐ。書き込みは同一 dir 内 temp + rename で atomic。集約上限（件数・集約バイト）は撤去し per-entry 上限のみ保持。TTL 能動削除・sharding・in-memory 再設計は範囲外（それぞれ sdp / 将来 / 別 issue）。

**Tech Stack:** Rust, tokio (`tokio::fs`), async-trait, axum, clap, tempfile (dev)。beads issue: `fulgur-chart-8tr.6`。

作業ディレクトリ: `/home/ubuntu/fulgur-chart/.worktrees/8tr6-file-shortlink-store`（全コマンドはここで実行）。

---

## Task 1: `FileShortlinkStore` を追加（TDD・既存コードに追加）

既存の `ShortlinkStore` はこの時点では残したまま、`FileShortlinkStore` を並行して追加し、単体テストで完成させる。

**Files:**
- Create: `crates/chart-server/src/file_store.rs`
- Modify: `crates/chart-server/src/lib.rs`
- Modify: `crates/chart-server/Cargo.toml`（`tempfile` を dev-dependency に追加）

**Step 1: `tempfile` を dev-dependency に追加**

`crates/chart-server/Cargo.toml` の `[dev-dependencies]` に追記:

```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
tempfile = "3"
```

**Step 2: `file_store.rs` に実装＋失敗するテストを書く**

`crates/chart-server/src/file_store.rs` を新規作成:

```rust
use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;

use crate::backend::{BackendError, ShortlinkBackend};

/// ファイルシステム上に id→query を永続化する durable な単一ノード backend。
///
/// 1 エントリ = 1 ファイル（ファイル名 = id、内容 = query 文字列）。filesystem の
/// パスが id→artifact 対応そのものになるため in-memory インデックスは持たない。
/// 再起動/デプロイをまたいでリンクを維持する。マルチノード/LB ハズレは解決しない
/// （ローカルディスクはノード固有）。TTL 能動削除・LRU eviction は範囲外（sdp）。
pub struct FileShortlinkStore {
    root: PathBuf,
    /// 単一エントリ（query 文字列）のバイト数上限。超過は `TooLarge`（→413）。
    entry_bytes: usize,
}

impl FileShortlinkStore {
    /// `root` ディレクトリを作成（存在すれば再利用）して store を構築する。
    /// ディレクトリを作成できない場合はエラー（呼び出し側=main で fail-fast）。
    pub async fn new(root: impl AsRef<Path>, entry_bytes: usize) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).await?;
        Ok(Self { root, entry_bytes })
    }

    /// id をファイル名として安全に使えるパスへ写像する。
    /// **検証を先に行う**こと（パス構築より前）。ULID の文字集合以外
    /// （`/`・`..` 等）は path traversal のリスクがあるため弾き、`None` を返す。
    fn path_for(&self, id: &str) -> Option<PathBuf> {
        // ULID 文字列は 26 文字。ASCII 英数字のみ許可すれば `/`・`.`・`\` を含む
        // id は構造的に弾かれ、traversal は起こり得ない（byte==char なので slice 安全）。
        if id.len() != 26 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        Some(self.root.join(id))
    }
}

#[async_trait]
impl ShortlinkBackend for FileShortlinkStore {
    async fn insert(&self, id: String, query: String) -> Result<(), BackendError> {
        // per-entry 上限: このペイロード単体が大きすぎる。再送しても無駄なので即拒否（→413）。
        if query.len() > self.entry_bytes {
            return Err(BackendError::TooLarge);
        }
        let Some(final_path) = self.path_for(&id) else {
            // server 生成 ULID は常に valid。到達し得ないが防御的に Unavailable 扱い。
            return Err(BackendError::Unavailable(
                format!("invalid shortlink id: {id}").into(),
            ));
        };
        // 同一ディレクトリ内の temp ファイルに書いてから rename で atomic に配置する
        // （並行 resolve の torn read 防止。同一 dir/同一 fs なので rename は atomic）。
        // ULID は一意なので temp 名（{id}.tmp）の衝突は起きない。fsync はしない
        // （保証は再起動/デプロイ耐性であって電源断耐性ではない）。
        let tmp_path = self.root.join(format!("{id}.tmp"));
        write_then_rename(&tmp_path, &final_path, query.as_bytes())
            .await
            .map_err(|e| BackendError::Unavailable(Box::new(e)))
    }

    async fn get(&self, id: &str) -> Result<Option<String>, BackendError> {
        let Some(path) = self.path_for(id) else {
            return Ok(None); // 不正/traversal id は未検出扱い（→404）
        };
        match fs::read(&path).await {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => Ok(Some(s)),
                Err(e) => Err(BackendError::Unavailable(Box::new(e))),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(BackendError::Unavailable(Box::new(e))),
        }
    }
}

/// temp に書いて rename する。rename 失敗時は temp を掃除して漏らさない。
async fn write_then_rename(tmp: &Path, final_path: &Path, data: &[u8]) -> io::Result<()> {
    fs::write(tmp, data).await?;
    if let Err(e) = fs::rename(tmp, final_path).await {
        let _ = fs::remove_file(tmp).await;
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::FileShortlinkStore;
    use crate::backend::{BackendError, ShortlinkBackend};
    use tempfile::TempDir;

    /// 有効な ULID 形状の id（26 文字 Crockford base32）。
    fn valid_id() -> String {
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()
    }

    async fn store(entry_bytes: usize) -> (FileShortlinkStore, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let s = FileShortlinkStore::new(dir.path(), entry_bytes).await.unwrap();
        (s, dir) // TempDir を返して test 終了まで生かす（drop で自動削除）
    }

    #[tokio::test]
    async fn insert_then_get_roundtrips() {
        let (s, _d) = store(1_000).await;
        let id = valid_id();
        s.insert(id.clone(), "c=hello&f=svg".into()).await.unwrap();
        assert_eq!(s.get(&id).await.unwrap(), Some("c=hello&f=svg".into()));
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let (s, _d) = store(1_000).await;
        assert_eq!(s.get(&valid_id()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_entry_exceeding_per_entry_byte_limit() {
        let (s, _d) = store(4).await;
        let r = s.insert(valid_id(), "12345".into()).await;
        assert!(matches!(&r, Err(BackendError::TooLarge)), "{r:?}");
        assert_eq!(s.get(&valid_id()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn invalid_id_is_treated_as_not_found() {
        let (s, _d) = store(1_000).await;
        let long = "x".repeat(27);
        for bad in ["../../etc/passwd", "..", "a/b", "short", long.as_str()] {
            assert_eq!(s.get(bad).await.unwrap(), None, "id={bad:?}");
        }
    }

    #[tokio::test]
    async fn insert_overwrites_same_id() {
        let (s, _d) = store(1_000).await;
        let id = valid_id();
        s.insert(id.clone(), "first".into()).await.unwrap();
        s.insert(id.clone(), "second".into()).await.unwrap();
        assert_eq!(s.get(&id).await.unwrap(), Some("second".into()));
    }

    /// ヘッドライン受け入れ基準: insert → drop → 同 dir で再構築 → get が値を返す。
    #[tokio::test]
    async fn persists_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let id = valid_id();
        {
            let s = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap();
            s.insert(id.clone(), "c=persist&f=png".into())
                .await
                .unwrap();
        } // ここで drop（プロセス再起動相当）
        let s2 = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap();
        assert_eq!(s2.get(&id).await.unwrap(), Some("c=persist&f=png".into()));
    }

    /// temp+rename が temp ファイルを残さない（root には最終ファイルのみ）。
    #[tokio::test]
    async fn no_temp_file_left_after_insert() {
        let (s, d) = store(1_000).await;
        let id = valid_id();
        s.insert(id.clone(), "x".into()).await.unwrap();
        let mut rd = tokio::fs::read_dir(d.path()).await.unwrap();
        let mut names = vec![];
        while let Some(e) = rd.next_entry().await.unwrap() {
            names.push(e.file_name().to_string_lossy().into_owned());
        }
        assert_eq!(names, vec![id]);
    }
}
```

**Step 3: `lib.rs` に module と再エクスポートを追加（この時点では store と併存）**

`crates/chart-server/src/lib.rs` を編集。`mod` 群に `file_store` を追加し、`pub use` に `FileShortlinkStore` を追加（`store` / `ShortlinkStore` はまだ残す）:

```rust
mod backend;
mod config;
mod file_store;
mod handlers;
mod render;
mod response;
mod server;
mod state;
mod store;

pub use backend::{BackendError, ShortlinkBackend};
pub use config::Config;
pub use file_store::FileShortlinkStore;
pub use server::build_router;
pub use store::ShortlinkStore;
```

**Step 4: テストを実行して通ることを確認**

Run: `cargo test -p chart-server file_store 2>&1 | tail -20`
Expected: `file_store::tests` の 7 本すべて PASS。既存 41 本も壊れていないこと（`cargo test -p chart-server` で 48 passed）。

**Step 5: Commit**

```bash
git add crates/chart-server/src/file_store.rs crates/chart-server/src/lib.rs crates/chart-server/Cargo.toml
git commit -m "feat(chart-server): add FileShortlinkStore durable filesystem backend (8tr.6)"
```

---

## Task 2: 配線を FileShortlinkStore に切り替え、Config を更新

`main.rs`・`Config`・全テストヘルパーを `FileShortlinkStore` に移行する。`store.rs` はこの時点では未使用のまま残す（削除は Task 3）。この Task の変更は相互依存するため一括で行い、最後にまとめてテストする。

**Files:**
- Modify: `crates/chart-server/src/config.rs`
- Modify: `crates/chart-server/src/main.rs`
- Modify: `crates/chart-server/src/server.rs`（テストヘルパー）
- Modify: `crates/chart-server/src/handlers/shortlink.rs`（テストヘルパー・不要テスト削除）
- Modify: `crates/chart-server/tests/public_api.rs`

**Step 1: `Config` を更新**

`crates/chart-server/src/config.rs`:
- `shortlink_limit` フィールド（`#[arg(... FULGUR_SHORTLINK_LIMIT ...)]` の 2 行）を**削除**。
- `shortlink_max_bytes` フィールド（doc コメント + `#[arg(... FULGUR_SHORTLINK_MAX_BYTES ...)]`）を**削除**。
- `shortlink_entry_bytes` は**残す**。
- 新しく `shortlink_dir` を追加（`shortlink_entry_bytes` の近くに配置）:

```rust
    /// shortlink を永続化するディレクトリ。既定は cwd 相対の `./fulgur-shortlinks`。
    /// FileShortlinkStore が起動時に作成する（作成不可なら fail-fast で起動中止）。
    /// 単一ノードの durable 保存であり、マルチノード/LB ハズレは解決しない。
    #[arg(long, env = "FULGUR_SHORTLINK_DIR", default_value = "./fulgur-shortlinks")]
    pub shortlink_dir: String,
```

> 破壊的変更（明記）: `FULGUR_SHORTLINK_LIMIT` / `FULGUR_SHORTLINK_MAX_BYTES` は廃止。これらを設定していたデプロイは clap の unknown-arg エラーで起動に失敗する（pre-1.0=0.1.0 で許容）。

**Step 2: `main.rs` を FileShortlinkStore に切り替え**

`crates/chart-server/src/main.rs` を全面置換:

```rust
use std::net::SocketAddr;

use chart_server::{Config, FileShortlinkStore, build_router};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cfg = Config::parse();
    // shortlink dir を作成して durable backend を wire。作成不可なら fail-fast。
    let store = std::sync::Arc::new(
        FileShortlinkStore::new(&cfg.shortlink_dir, cfg.shortlink_entry_bytes)
            .await
            .unwrap_or_else(|e| {
                panic!("failed to open shortlink dir {:?}: {e}", cfg.shortlink_dir)
            }),
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

**Step 3: `server.rs` のテストヘルパーを移行**

`crates/chart-server/src/server.rs` の `#[cfg(test)] mod tests`:
- `use crate::store::ShortlinkStore;` を `use crate::file_store::FileShortlinkStore;` に変更。
- 各 `Config { ... }` リテラルから `shortlink_limit` / `shortlink_max_bytes` の行を削除し、`shortlink_dir: "unused".into(),` を追加（build_router は `shortlink_dir` を読まないので値は任意）。
- `restricted_cors_router()` と `router_with_compression(...)` を `async fn` にし、`ShortlinkStore::new(...)` を tempdir ベースの `FileShortlinkStore` に置換。TempDir はリークして dir を生かす（テスト専用）:

```rust
    use crate::file_store::FileShortlinkStore;

    /// テスト用: tempdir に FileShortlinkStore を作って Arc で返す。
    /// TempDir はリークしてテスト実行中 dir を保持する（テスト専用の許容トレードオフ）。
    async fn temp_file_store() -> std::sync::Arc<FileShortlinkStore> {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        std::sync::Arc::new(
            FileShortlinkStore::new(dir.path(), 512 * 1024).await.unwrap(),
        )
    }

    async fn restricted_cors_router() -> Router {
        let cfg = Config { /* ... shortlink_dir: "unused".into(), shortlink_entry_bytes: 512*1024, ... */ };
        build_router(&cfg, temp_file_store().await)
    }

    async fn router_with_compression(compression: Compression) -> Router {
        let cfg = Config { /* ... */ };
        build_router(&cfg, temp_file_store().await)
    }
```

- 呼び出し側に `.await` を付ける: `png_len_for_config` 内の `router_with_compression(compression)` → `.await`、`restricted_cors_allows_if_none_match_preflight` の `restricted_cors_router()` → `.await`、`webp_disabled_returns_415_even_with_matching_etag` の 2 箇所 → `.await`。

**Step 4: `handlers/shortlink.rs` のテストヘルパーを移行し、不要テストを削除**

`crates/chart-server/src/handlers/shortlink.rs` の `#[cfg(test)] mod http_tests`:
- `use crate::store::ShortlinkStore;` を削除し、`use crate::file_store::FileShortlinkStore;` に変更。
- `router_with_store(store)` / `router_with_store_and_ttl(store, ttl)` を、store 引数ではなく `entry_bytes` / `ttl` パラメータを取る tempdir ベースのヘルパーに置換:

```rust
    use crate::file_store::FileShortlinkStore;

    async fn router_with_entry_bytes(entry_bytes: usize) -> Router {
        router_with_entry_bytes_and_ttl(entry_bytes, 86_400).await
    }

    async fn router_with_entry_bytes_and_ttl(entry_bytes: usize, ttl: u64) -> Router {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let store = FileShortlinkStore::new(dir.path(), entry_bytes).await.unwrap();
        let cfg = Config { /* ... shortlink_dir: "unused".into(), shortlink_entry_bytes: entry_bytes, shortlink_ttl_seconds: ttl, ... */ };
        build_router(&cfg, std::sync::Arc::new(store))
    }
```

- 各テストを移行:
  - `create_rejects_oversized_entry_with_413`: `router_with_entry_bytes(10).await` を使う（`entry_bytes=10` で TooLarge/413）。
  - `create_succeeds_within_limits` / `create_generates_distinct_ids_for_identical_specs` / `create_returns_url_with_26_char_ulid_id`: `router_with_entry_bytes(512 * 1024).await`。
  - `resolve_success_sets_cache_control_from_config_ttl`: `router_with_entry_bytes_and_ttl(512 * 1024, 3600).await`。
  - `resolve_not_found_sets_no_store_cache_control`: `router_with_entry_bytes(512 * 1024).await`。
- **`create_returns_503_when_store_full` テストを削除**（FileShortlinkStore は集約件数上限を持たず `Full`/503 を返さないため。ハンドラの `Full`→503 マッピング自体は残るが、external adapter 用で OSS デフォルトでは発火しない）。

**Step 5: `tests/public_api.rs` を移行**

- import: `use chart_server::{BackendError, Config, ShortlinkBackend, ShortlinkStore, build_router};` の `ShortlinkStore` を `FileShortlinkStore` に変更。
- `external_backend_can_be_injected_into_build_router` を `#[tokio::test] async fn` に変更し、`ShortlinkStore::new(10, 1024, 256)` を `FileShortlinkStore::new(tempfile::tempdir().unwrap().path(), 256).await.unwrap()` に置換（`NoopBackend` の inject 検証はそのまま）。
- `Cargo.toml` の dev-deps に `tempfile`（Task 1 で追加済み）を integration test でも使う。

**Step 6: 全テストを実行して通ることを確認**

Run: `cargo test -p chart-server 2>&1 | tail -30`
Expected: 全テスト PASS（file_store 7 + 既存から 503-full 削除分 -1 + 移行分）。`store.rs` の unit テストはまだ存在するので `store::tests` も PASS のまま。

**Step 7: Commit**

```bash
git add crates/chart-server/src/config.rs crates/chart-server/src/main.rs \
        crates/chart-server/src/server.rs crates/chart-server/src/handlers/shortlink.rs \
        crates/chart-server/tests/public_api.rs
git commit -m "feat(chart-server): wire FileShortlinkStore as sole backend, drop aggregate caps (8tr.6)"
```

---

## Task 3: in-memory `ShortlinkStore` と `dashmap` を撤去

未使用になった in-memory backend を撤去し、依存を掃除する。

**Files:**
- Delete: `crates/chart-server/src/store.rs`
- Modify: `crates/chart-server/src/lib.rs`
- Modify: `crates/chart-server/Cargo.toml`

**Step 1: `store.rs` を削除**

```bash
git rm crates/chart-server/src/store.rs
```

**Step 2: `lib.rs` から `store` を除去**

`crates/chart-server/src/lib.rs` から `mod store;` と `pub use store::ShortlinkStore;` の 2 行を削除。最終形:

```rust
mod backend;
mod config;
mod file_store;
mod handlers;
mod render;
mod response;
mod server;
mod state;

pub use backend::{BackendError, ShortlinkBackend};
pub use config::Config;
pub use file_store::FileShortlinkStore;
pub use server::build_router;
```

**Step 3: `Cargo.toml` から `dashmap` を削除**

`crates/chart-server/Cargo.toml` の `[dependencies]` から `dashmap = "6"` の行を削除（`store.rs` 専用だった。`sha2`/`hex` は `response.rs` の ETag で使用継続のため残す）。

**Step 4: ビルド・テスト・clippy を確認**

```bash
cargo build -p chart-server 2>&1 | tail -5
cargo test -p chart-server 2>&1 | tail -15
cargo clippy -p chart-server --all-targets 2>&1 | tail -15
```
Expected: build OK、全テスト PASS、clippy 警告なし。`ShortlinkStore` への参照が残っていないこと（`grep -rn ShortlinkStore crates/chart-server/src crates/chart-server/tests` が空）。

**Step 5: Commit**

```bash
git add -A crates/chart-server/
git commit -m "refactor(chart-server): remove in-memory ShortlinkStore and dashmap dep (8tr.6)"
```

---

## Task 4: 統合検証（手動スモーク）

永続化が実バイナリで機能することを end-to-end で確認する（REQUIRED SUB-SKILL: `verify` を後段で使う場合の材料）。

**Step 1: 一時 dir でサーバを起動 → create → resolve → 再起動 → resolve**

```bash
SLDIR=$(mktemp -d)
# 1) 起動（バックグラウンド）
FULGUR_SHORTLINK_DIR="$SLDIR" FULGUR_PORT=3999 cargo run -p chart-server >/tmp/sl.log 2>&1 &
SRV=$!
sleep 2
# 2) create
URL=$(curl -s -XPOST localhost:3999/chart/create -H 'content-type: application/json' \
  -d '{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["url"])')
echo "created: $URL"
# 3) resolve（307 を確認）
curl -s -o /dev/null -w '%{http_code}\n' "localhost:3999$URL"   # → 307
# 4) 再起動
kill $SRV; sleep 1
FULGUR_SHORTLINK_DIR="$SLDIR" FULGUR_PORT=3999 cargo run -p chart-server >/tmp/sl2.log 2>&1 &
SRV=$!
sleep 2
# 5) 再起動後も resolve できる（永続化の証明）→ 307
curl -s -o /dev/null -w '%{http_code}\n' "localhost:3999$URL"
kill $SRV; rm -rf "$SLDIR"
```

Expected: create 後 307、**再起動後も 307**（in-memory なら 404 になっていた）。`$SLDIR` に id 名のファイルが 1 つできていること。

**Step 2: 受け入れ基準の最終確認**

beads issue `fulgur-chart-8tr.6` の acceptance criteria を 1 つずつ照合し、すべて満たすことを確認する。

---

## 完了後

- REQUIRED SUB-SKILL: `superpowers:verification-before-completion` でテスト/検証。
- REQUIRED SUB-SKILL: `superpowers:finishing-a-development-branch` でブランチ完了処理（PR 等）。
- `bd close fulgur-chart-8tr.6`（ユーザー確認後）。
