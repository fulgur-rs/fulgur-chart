# FileShortlinkStore TTL sweep + 容量 backstop 実装計画

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `FileShortlinkStore` に time-bucket レイアウト・TTL sweep・容量 backstop を実装し、shortlink ストアがディスク無制限増加せず、満杯でも自己回復するようにする（beads: fulgur-chart-sdp）。

**Architecture:** エントリを `root/{bucket}/{id}`（`bucket = ulid_ms(id)/1h`）に配置。背景 task と insert の inline 呼び出しが共有する `sweep_expired(now_ms)` が「`(b+1)·W + TTL ≤ now` の bucket のみ rmdir」し、age<TTL を構造的に不削除（`Cache-Control: max-age` 保証厳守）。件数/バイトのアトミックカウンタで O(1) の満杯判定を行い、超過時は inline sweep→なお超過なら `BackendError::Full`(→503)。

**Tech Stack:** Rust / tokio / async-trait / ulid 1.2 / tempfile（テスト）。対象 crate: `crates/chart-server`。

**参照:** design/acceptance は beads issue `fulgur-chart-sdp`（`bd show fulgur-chart-sdp`）。現状コードは `src/file_store.rs`, `src/backend.rs`, `src/config.rs`, `src/main.rs`, `src/server.rs`, `src/handlers/shortlink.rs`。

**設計の精緻化（saved design からの改善、実装はこちらに従う）:**
1. clock 注入は行わず `sweep_expired(&self, now_ms: u64)` に now を引数で渡す。背景/inline は system now、テストは任意 now。insert の over-cap 判定は old-ULID エントリで age を制御でき決定的。
2. 周期フル再カウントは行わない（O(buckets) sweep と矛盾）。カウンタは「起動時 seed＋insert 加算＋sweep 削除数で減算」で正確に保つ。overwrite は server 生成 ULID が一意なので本番で発生せず、発生しても soft backstop の +1 slack として許容。

**共通事項:**
- 各タスクは TDD（失敗テスト→最小実装→pass→commit）。
- テストコマンドは worktree ルート `/home/ubuntu/fulgur-chart/.worktrees/sdp-shortlink-eviction` で実行。
- 定数: `const WIDTH_MS: u64 = 3_600_000;`（1h）, `const SWEEP_INTERVAL_SECS: u64 = 60;`（main のみ使用）。
- 時刻ヘルパー（file_store.rs 内）:
  ```rust
  fn system_now_ms() -> u64 {
      std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .map(|d| d.as_millis() as u64)
          .unwrap_or(0)
  }
  ```

---

### Task 1: time-bucket レイアウト（path_for / insert / get）

エントリ配置を `root/{id}` から `root/{bucket}/{id}` に変える。まだカウンタ/sweep/caps は入れない（純リファクタ）。

**Files:**
- Modify: `crates/chart-server/src/file_store.rs`

**Step 1: 失敗テストを書く** — `file_store.rs` の `#[cfg(test)] mod tests` に、id から導出される bucket ディレクトリに実ファイルが置かれることを検証するテストを追加。

```rust
use ulid::Ulid;

/// 指定 ms の ULID を生成（bucket/TTL の決定的テスト用）。
fn id_at_ms(ms: u64) -> String {
    Ulid::from_parts(ms, 0).to_string()
}

#[tokio::test]
async fn insert_places_entry_in_time_bucket_dir() {
    let (s, d) = store(1_000).await;
    let ms = 1_700_000_000_000; // 任意の固定時刻
    let id = id_at_ms(ms);
    s.insert(id.clone(), "c=x&f=svg".into()).await.unwrap();
    let bucket = ms / 3_600_000;
    let expected = d.path().join(bucket.to_string()).join(&id);
    assert!(expected.is_file(), "entry should live at root/{{bucket}}/{{id}}: {expected:?}");
    assert_eq!(s.get(&id).await.unwrap(), Some("c=x&f=svg".into()));
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p chart-server insert_places_entry_in_time_bucket_dir 2>&1 | tail -20`
Expected: FAIL（現状は flat `root/{id}` に置かれるため `expected.is_file()` が false）。

**Step 3: 最小実装** — `WIDTH_MS` 定数追加。`path_for` を ULID パースベースに置換し bucket を導出。`insert` は書き込み前に bucket dir を作成。

```rust
use ulid::Ulid;

const WIDTH_MS: u64 = 3_600_000; // 1h

impl FileShortlinkStore {
    /// id を検証し `root/{bucket}/{id}` パスへ写像する。ULID 以外は None。
    /// ULID のパース成功が 26 文字 Crockford base32 を保証し path traversal を構造的に排除する。
    fn path_for(&self, id: &str) -> Option<PathBuf> {
        let ulid = Ulid::from_string(id).ok()?;
        let bucket = ulid.timestamp_ms() / WIDTH_MS;
        Some(self.root.join(bucket.to_string()).join(id))
    }
}
```

`insert` の temp/final パス構築を bucket 対応に変更（`final_path` の親 = bucket dir を `create_dir_all`、temp も同じ bucket dir 内に置く＝同一 fs で atomic rename）:

```rust
async fn insert(&self, id: String, query: String) -> Result<(), BackendError> {
    if query.len() > self.entry_bytes {
        return Err(BackendError::TooLarge);
    }
    let Some(final_path) = self.path_for(&id) else {
        return Err(BackendError::Unavailable(
            format!("invalid shortlink id: {id}").into(),
        ));
    };
    let bucket_dir = final_path.parent().expect("path_for always has a bucket parent").to_path_buf();
    let tmp_path = bucket_dir.join(format!("{id}.tmp"));
    tokio::fs::create_dir_all(&bucket_dir)
        .await
        .map_err(|e| BackendError::Unavailable(Box::new(e)))?;
    tokio::task::spawn_blocking(move || write_then_rename(&tmp_path, &final_path, query.as_bytes()))
        .await
        .map_err(|e| BackendError::Unavailable(Box::new(e)))?
        .map_err(|e| BackendError::Unavailable(Box::new(e)))
}
```

（`get` は `path_for` 経由なので実装変更不要 — bucket 込みパスを自動で読む。）

**Step 4: pass 確認**

Run: `cargo test -p chart-server 2>&1 | tail -25`
Expected: 新テスト含め全 pass。特に `persists_across_restart`（同 id→同 bucket）, `invalid_id_is_treated_as_not_found`, `no_temp_file_left_after_insert`（root 直下ではなく bucket 内を見るよう次 Step で調整が必要なら修正）が pass。

補足: `no_temp_file_left_after_insert` は `d.path()` 直下を読む。bucket 化後は root 直下に id ファイルが無いのでこのテストは失敗する。次のように bucket dir を見るよう修正:

```rust
#[tokio::test]
async fn no_temp_file_left_after_insert() {
    let (s, d) = store(1_000).await;
    let id = valid_id();
    s.insert(id.clone(), "x".into()).await.unwrap();
    let bucket = Ulid::from_string(&id).unwrap().timestamp_ms() / 3_600_000;
    let bucket_dir = d.path().join(bucket.to_string());
    let mut rd = tokio::fs::read_dir(&bucket_dir).await.unwrap();
    let mut names = vec![];
    while let Some(e) = rd.next_entry().await.unwrap() {
        names.push(e.file_name().to_string_lossy().into_owned());
    }
    assert_eq!(names, vec![id]);
}
```

**Step 5: commit**

```bash
git add crates/chart-server/src/file_store.rs
git commit -m "refactor(chart-server): store shortlinks in time-bucket dirs (sdp)"
```

---

### Task 2: 容量カウンタ + backstop（over-cap で Full、sweep はまだ無し）

アトミックカウンタ（件数・バイト）を追加し、起動時 seed・insert 加算。caps 超過で `BackendError::Full`。

**Files:**
- Modify: `crates/chart-server/src/file_store.rs`

**Step 1: 失敗テストを書く**

```rust
use crate::backend::BackendError;

/// caps を明示した store ヘルパー（unlimited=0）。
async fn store_capped(entry_bytes: usize, max_bytes: u64, max_entries: u64) -> (FileShortlinkStore, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = FileShortlinkStore::new(dir.path(), entry_bytes)
        .await
        .unwrap()
        .with_capacity(max_bytes, max_entries);
    (s, dir)
}

#[tokio::test]
async fn insert_returns_full_when_entry_cap_reached() {
    let (s, _d) = store_capped(1_000, 0, 1).await; // 件数上限 1
    s.insert(id_at_ms(1_700_000_000_000), "a".into()).await.unwrap();
    let r = s.insert(id_at_ms(1_700_000_000_001), "b".into()).await;
    assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
}

#[tokio::test]
async fn insert_returns_full_when_byte_budget_reached() {
    let (s, _d) = store_capped(1_000, 4, 0).await; // バイト上限 4
    s.insert(id_at_ms(1_700_000_000_000), "abc".into()).await.unwrap(); // 3B
    let r = s.insert(id_at_ms(1_700_000_000_001), "de".into()).await;   // +2B > 4
    assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
}

#[tokio::test]
async fn counters_are_seeded_from_existing_entries_on_construction() {
    let dir = tempfile::tempdir().unwrap();
    {
        let s = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap();
        s.insert(id_at_ms(1_700_000_000_000), "abc".into()).await.unwrap();
    }
    // 再構築（件数上限 1）→ 既存 1 件を数えているので次 insert は Full
    let s2 = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap().with_capacity(0, 1);
    let r = s2.insert(id_at_ms(1_700_000_000_001), "x".into()).await;
    assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
}

#[tokio::test]
async fn zero_caps_mean_unlimited() {
    let (s, _d) = store_capped(1_000, 0, 0).await;
    for i in 0..50u64 {
        s.insert(id_at_ms(1_700_000_000_000 + i), "x".into()).await.unwrap();
    }
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p chart-server counters_are_seeded 2>&1 | tail -20`
Expected: FAIL（`with_capacity` 未定義でコンパイルエラー）。

**Step 3: 最小実装** — 構造体にカウンタと caps を追加。`use std::sync::atomic::{AtomicU64, Ordering};`。

```rust
pub struct FileShortlinkStore {
    root: PathBuf,
    entry_bytes: usize,
    /// 集約バイト上限（0 = 無制限）。超過は Full(→503)。
    max_bytes: u64,
    /// 件数上限（0 = 無制限）。超過は Full(→503)。
    max_entries: u64,
    /// TTL 秒（sweep のしきい値。既定 86_400）。Task 3 で使用。
    ttl_seconds: u64,
    /// 現在のエントリ数（accelerator。真実源はディスク）。
    count: AtomicU64,
    /// 現在の集約バイト数（accelerator）。
    bytes: AtomicU64,
}
```

`new` を「seed 込み」に変更（既存の probe はそのまま）。seed はバケット走査で件数/バイトを合算:

```rust
pub async fn new(root: impl AsRef<Path>, entry_bytes: usize) -> io::Result<Self> {
    let root = root.as_ref().to_path_buf();
    fs::create_dir_all(&root).await?;
    let store = Self {
        root,
        entry_bytes,
        max_bytes: 0,
        max_entries: 0,
        ttl_seconds: 86_400,
        count: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
    };
    store.probe_writable().await?;
    let (count, bytes) = store.scan_totals().await?;
    store.count.store(count, Ordering::Relaxed);
    store.bytes.store(bytes, Ordering::Relaxed);
    Ok(store)
}

/// builder: 容量上限を設定（0 = 無制限）。
pub fn with_capacity(mut self, max_bytes: u64, max_entries: u64) -> Self {
    self.max_bytes = max_bytes;
    self.max_entries = max_entries;
    self
}

/// builder: TTL 秒を設定（sweep しきい値）。
pub fn with_ttl_seconds(mut self, ttl_seconds: u64) -> Self {
    self.ttl_seconds = ttl_seconds;
    self
}

/// 全 bucket dir を走査して (件数, 総バイト) を返す。起動時 seed 用（O(n) 1 回）。
async fn scan_totals(&self) -> io::Result<(u64, u64)> {
    let mut count = 0u64;
    let mut bytes = 0u64;
    let mut buckets = match fs::read_dir(&self.root).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(e) => return Err(e),
    };
    while let Some(b) = buckets.next_entry().await? {
        // 数値名の bucket dir のみを対象（probe/tmp 等の root 直下ファイルは無視）。
        let name = b.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.parse::<u64>().is_err() { continue }
        if !b.file_type().await.map(|t| t.is_dir()).unwrap_or(false) { continue }
        let mut entries = fs::read_dir(b.path()).await?;
        while let Some(e) = entries.next_entry().await? {
            let fname = e.file_name();
            // temp ファイルは数えない。
            if fname.to_str().map(|s| s.ends_with(".tmp")).unwrap_or(true) { continue }
            if let Ok(meta) = e.metadata().await {
                if meta.is_file() {
                    count += 1;
                    bytes += meta.len();
                }
            }
        }
    }
    Ok((count, bytes))
}
```

`insert` の先頭（TooLarge 判定の後、書き込み前）に容量チェックを挿入。成功後にカウンタ加算:

```rust
// per-entry 上限チェック（既存）の後:
let new_len = query.len() as u64;
if self.would_exceed(new_len) {
    // Task 4 で inline sweep を挟む。現段階では即 Full。
    return Err(BackendError::Full);
}
// ... create_dir_all → write_then_rename（既存）...
// 書き込み成功後:
self.count.fetch_add(1, Ordering::Relaxed);
self.bytes.fetch_add(new_len, Ordering::Relaxed);
Ok(())
```

`would_exceed` ヘルパー:

```rust
/// new_len バイトの追加が件数 or バイト上限を超えるか（0 上限は無制限）。
fn would_exceed(&self, new_len: u64) -> bool {
    let count = self.count.load(Ordering::Relaxed);
    let bytes = self.bytes.load(Ordering::Relaxed);
    (self.max_entries != 0 && count + 1 > self.max_entries)
        || (self.max_bytes != 0 && bytes + new_len > self.max_bytes)
}
```

注意: `insert` は成功時のみ加算するよう、`spawn_blocking` の結果を受けてから `fetch_add` する形に整える（早期 return の Full/TooLarge/Unavailable では加算しない）。

**Step 4: pass 確認**

Run: `cargo test -p chart-server 2>&1 | tail -25`
Expected: 全 pass。

**Step 5: commit**

```bash
git add crates/chart-server/src/file_store.rs
git commit -m "feat(chart-server): capacity counters + Full backstop for file store (sdp)"
```

---

### Task 3: TTL sweep（`sweep_expired(now_ms)`）と保証厳守

`(b+1)·W + TTL ≤ now` の bucket のみ削除し、カウンタを削除数/バイトで減算。

**Files:**
- Modify: `crates/chart-server/src/file_store.rs`

**Step 1: 失敗テストを書く**

```rust
const TTL: u64 = 86_400;
const TTL_MS: u64 = TTL * 1_000;
const W: u64 = 3_600_000;

#[tokio::test]
async fn sweep_removes_buckets_older_than_ttl() {
    let (s, d) = store(1_000).await;
    let s = s.with_ttl_seconds(TTL);
    let old_ms = 1_700_000_000_000;
    let old_id = id_at_ms(old_ms);
    s.insert(old_id.clone(), "old".into()).await.unwrap();
    // bucket b の削除可能時刻 = (b+1)*W + TTL_MS
    let b = old_ms / W;
    let now = (b + 1) * W + TTL_MS; // ちょうど削除可能になる境界
    s.sweep_expired(now).await;
    assert_eq!(s.get(&old_id).await.unwrap(), None, "age≥TTL は削除される");
    let bucket_dir = d.path().join(b.to_string());
    assert!(!bucket_dir.exists(), "expired bucket dir は rmdir される");
}

#[tokio::test]
async fn sweep_preserves_entries_at_bucket_boundary() {
    // bucket 粒度の境界: 削除可能まで 1ms 足りない now では bucket ごと残る。
    let (s, _d) = store(1_000).await;
    let s = s.with_ttl_seconds(TTL);
    let ms = 1_700_000_000_000;
    let id = id_at_ms(ms);
    s.insert(id.clone(), "boundary".into()).await.unwrap();
    let b = ms / W;
    let now = (b + 1) * W + TTL_MS - 1;
    s.sweep_expired(now).await;
    assert_eq!(s.get(&id).await.unwrap(), Some("boundary".into()), "境界の 1ms 手前は残る");
}

/// Cache-Control: max-age 保証の核: 「今」作成した age≈0 のエントリは、system now での
/// sweep で決して消えない。issue の本丸なので明示的に検証する。
#[tokio::test]
async fn sweep_never_removes_a_genuinely_young_entry() {
    let (s, _d) = store(1_000).await;
    let s = s.with_ttl_seconds(TTL);
    let id = Ulid::new().to_string(); // age ≈ 0
    s.insert(id.clone(), "fresh".into()).await.unwrap();
    s.sweep_expired(super::system_now_ms()).await; // 実時刻での sweep（module private fn）
    assert_eq!(s.get(&id).await.unwrap(), Some("fresh".into()), "age<TTL は絶対に消えない");
}

#[tokio::test]
async fn sweep_decrements_counters() {
    // 消えた bucket の分だけ件数上限が空くことで間接的に検証。
    let (s, _d) = store(1_000).await;
    let s = s.with_ttl_seconds(TTL).with_capacity(0, 1);
    let old_ms = 1_700_000_000_000;
    s.insert(id_at_ms(old_ms), "old".into()).await.unwrap();
    // 上限 1 なので今は Full
    assert!(matches!(
        s.insert(id_at_ms(old_ms + 1), "x".into()).await,
        Err(BackendError::Full)
    ));
    let b = old_ms / W;
    s.sweep_expired((b + 1) * W + TTL_MS).await;
    // sweep でカウンタが 0 に戻り、新規 insert が通る（新しい若い id）
    s.insert(id_at_ms((b + 1) * W + TTL_MS + 1), "fresh".into()).await.unwrap();
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p chart-server sweep_ 2>&1 | tail -20`
Expected: FAIL（`sweep_expired` 未定義）。

**Step 3: 最小実装**

```rust
impl FileShortlinkStore {
    /// `(b+1)*WIDTH_MS + ttl_ms ≤ now_ms` を満たす bucket dir を丸ごと削除する。
    /// この条件により bucket 内の最も若いエントリでも age≥TTL が保証され、age<TTL は
    /// 構造的に削除されない（Cache-Control: max-age の下限保証を厳守）。
    /// 削除したファイル数/バイトぶんカウンタを減算する。I/O エラーは best-effort で無視
    /// （次回 sweep で再試行される。janitor なので単発失敗を致命にしない）。
    pub async fn sweep_expired(&self, now_ms: u64) {
        let ttl_ms = self.ttl_seconds.saturating_mul(1_000);
        let mut buckets = match fs::read_dir(&self.root).await {
            Ok(rd) => rd,
            Err(_) => return,
        };
        while let Ok(Some(b)) = buckets.next_entry().await {
            let name = b.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(bucket) = name.parse::<u64>() else { continue }; // 数値名のみ
            // (b+1)*W + TTL ≤ now で削除可能。オーバーフローは saturating で回避。
            let deletable_at = bucket
                .saturating_add(1)
                .saturating_mul(WIDTH_MS)
                .saturating_add(ttl_ms);
            if deletable_at > now_ms {
                continue;
            }
            // 削除前に件数/バイトを集計してカウンタを減算。
            let (mut n, mut nbytes) = (0u64, 0u64);
            if let Ok(mut entries) = fs::read_dir(b.path()).await {
                while let Ok(Some(e)) = entries.next_entry().await {
                    if e.file_name().to_str().map(|s| s.ends_with(".tmp")).unwrap_or(true) { continue }
                    if let Ok(meta) = e.metadata().await {
                        if meta.is_file() { n += 1; nbytes += meta.len(); }
                    }
                }
            }
            if fs::remove_dir_all(b.path()).await.is_ok() {
                self.count.fetch_sub(n.min(self.count.load(Ordering::Relaxed)), Ordering::Relaxed);
                self.bytes.fetch_sub(nbytes.min(self.bytes.load(Ordering::Relaxed)), Ordering::Relaxed);
            }
        }
    }
}
```

（`.min(current)` で減算アンダーフローを防ぐ。並行 insert が現在 bucket に入るのは想定内で、sweep 対象の expired bucket とは別 dir なので競合しない。）

**Step 4: pass 確認**

Run: `cargo test -p chart-server 2>&1 | tail -25`
Expected: 全 pass。特に境界テスト（`-1` で保持、ちょうどで削除）。

**Step 5: commit**

```bash
git add crates/chart-server/src/file_store.rs
git commit -m "feat(chart-server): TTL sweep_expired preserving the max-age guarantee (sdp)"
```

---

### Task 4: insert の inline sweep（over-cap 時）

over-cap 時にまず inline sweep で期限切れ bucket を drain し、再判定してから Full を返す（自己回復型）。

**Files:**
- Modify: `crates/chart-server/src/file_store.rs`

**Step 1: 失敗テストを書く**

```rust
#[tokio::test]
async fn insert_over_cap_drains_expired_then_succeeds() {
    // 件数上限 1。まず古いエントリで埋める。
    let (s, _d) = store(1_000).await;
    let s = s.with_ttl_seconds(1).with_capacity(0, 1); // TTL=1s（すぐ期限切れ）
    let old_ms: u64 = 1_000; // epoch 直後 → 現在時刻から見て遥かに age≥TTL
    s.insert(id_at_ms(old_ms), "old".into()).await.unwrap();
    // 新しい若い id を insert → over-cap だが inline sweep が古い bucket を drain して受理。
    let fresh = Ulid::new().to_string(); // 現在時刻の ULID
    s.insert(fresh.clone(), "fresh".into()).await.unwrap();
    assert_eq!(s.get(&fresh).await.unwrap(), Some("fresh".into()));
}

#[tokio::test]
async fn insert_over_cap_all_young_returns_full() {
    let (s, _d) = store(1_000).await;
    let s = s.with_ttl_seconds(86_400).with_capacity(0, 1); // TTL=24h
    // 2 件とも「今」作成＝ age<TTL。inline sweep は何も消せない → Full。
    s.insert(Ulid::new().to_string(), "a".into()).await.unwrap();
    let r = s.insert(Ulid::new().to_string(), "b".into()).await;
    assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p chart-server insert_over_cap 2>&1 | tail -20`
Expected: `insert_over_cap_drains_expired_then_succeeds` が FAIL（現状 over-cap で即 Full）。

**Step 3: 最小実装** — Task 2 で入れた即 Full 部分を inline sweep 込みに差し替え:

```rust
let new_len = query.len() as u64;
if self.would_exceed(new_len) {
    // 満杯: まず期限切れ bucket を drain して自己回復を試みる（system now）。
    self.sweep_expired(system_now_ms()).await;
    if self.would_exceed(new_len) {
        // drain 後もなお超過（＝全 live が age<TTL）→ 一時拒否（次 sweep で回復）。
        return Err(BackendError::Full);
    }
}
```

**Step 4: pass 確認**

Run: `cargo test -p chart-server 2>&1 | tail -25`
Expected: 全 pass。

**Step 5: commit**

```bash
git add crates/chart-server/src/file_store.rs
git commit -m "feat(chart-server): inline sweep on over-capacity insert before 503 (sdp)"
```

---

### Task 5: config 追加 + main の背景 sweep task 配線

新 config 2 本を追加し、main で store に caps/TTL を wire、背景 sweep task を spawn。migration guard を更新。

**Files:**
- Modify: `crates/chart-server/src/config.rs`
- Modify: `crates/chart-server/src/main.rs`
- Modify: `crates/chart-server/src/server.rs`（`base_config()` にフィールド追加）
- Modify: `crates/chart-server/src/handlers/shortlink.rs`（http_tests の `Config { .. }` リテラルにフィールド追加）
- Modify: `crates/chart-server/src/handlers/mcp.rs`（`FileShortlinkStore::new` 呼び出しは builder 無しでも既定 unlimited なので変更不要か確認。Config リテラルがあれば追加）

**Step 1: config.rs にフィールド追加**（`shortlink_ttl_seconds` の直後）:

```rust
/// shortlink ストアの集約バイト上限（0 = 無制限）。既定 512 MiB。
/// TTL sweep が主だが、TTL 窓(24h)内に埋め尽くされてもディスクを直接上限化する
/// hard guard。超過 insert は inline sweep→なお超過なら 503（次 sweep で自己回復）。
#[arg(long, env = "FULGUR_SHORTLINK_MAX_BYTES", default_value_t = 512 * 1024 * 1024)]
pub shortlink_max_bytes: u64,

/// shortlink ストアの件数上限（0 = 無制限）。既定 100_000。inode/dir 枯渇を上限化。
#[arg(long, env = "FULGUR_SHORTLINK_MAX_ENTRIES", default_value_t = 100_000)]
pub shortlink_max_entries: u64,
```

**Step 2: main.rs の guard 更新 + wiring**

guard リストから `FULGUR_SHORTLINK_MAX_BYTES` を除去（サポート復活）。`FULGUR_SHORTLINK_LIMIT`（旧件数名）は残し、メッセージを新 env に誘導:

```rust
// 旧集約上限 env（8tr.6 で撤去）を検出して fail-fast。sdp で MAX_BYTES/MAX_ENTRIES を
// 復活させたため、旧名 FULGUR_SHORTLINK_LIMIT のみ「改名」として弾く。
if std::env::var_os("FULGUR_SHORTLINK_LIMIT").is_some() {
    panic!(
        "FULGUR_SHORTLINK_LIMIT is renamed: use FULGUR_SHORTLINK_MAX_ENTRIES \
         (and FULGUR_SHORTLINK_MAX_BYTES for the byte budget)."
    );
}
```

store 構築に builder を連結し、背景 sweep task を spawn:

```rust
let store = std::sync::Arc::new(
    FileShortlinkStore::new(&cfg.shortlink_dir, cfg.shortlink_entry_bytes)
        .await
        .unwrap_or_else(|e| panic!("failed to open shortlink dir {:?}: {e}", cfg.shortlink_dir))
        .with_ttl_seconds(cfg.shortlink_ttl_seconds)
        .with_capacity(cfg.shortlink_max_bytes, cfg.shortlink_max_entries),
);

// 背景 janitor: 一定間隔で期限切れ bucket を sweep。concrete Arc を clone して回す
// （sweep は FileShortlinkStore 固有メソッドで trait には無い）。detached task で、
// プロセス終了時に drop される（main に graceful shutdown が無いため YAGNI）。
{
    let store = store.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tick.tick().await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            store.sweep_expired(now).await;
        }
    });
}
```

（`tokio::time::interval` は最初の `tick()` が即時に返るので、起動直後に 1 度 sweep が走る＝停止中に期限切れた bucket を drain。）

`build_router(&cfg, store)` はそのまま（`Arc<FileShortlinkStore>` → `Arc<dyn ShortlinkBackend>` に coercion）。

**Step 3: テスト内 Config リテラルにフィールド追加**

`server.rs` の `base_config()`（134 行）と `handlers/shortlink.rs` http_tests の `Config { .. }`（192 行）に:

```rust
shortlink_max_bytes: 0,      // テストは無制限（容量テストは file_store 側で実施）
shortlink_max_entries: 0,
```

`server.rs:153,162` は `..base_config()` なので追加不要。`handlers/mcp.rs` は Config リテラル無し・`FileShortlinkStore::new` は builder 無し（既定 unlimited）なので**変更不要**（確認済み）。

**重要（変更不要の確認）:** `crates/chart-server/tests/public_api.rs` の `default_config()` は構造体リテラルではなく `Config::parse_from(["chart-server"])`。新フィールドは `default_value_t` で埋まるため**このファイルは変更不要**。`default_value_t` はリテラル構築には効かないので、更新が要るのは `src/` の 2 リテラル（`server.rs::base_config`, `handlers/shortlink.rs` http_tests）のみ。

**guard 変更の確認:** 旧 env `FULGUR_SHORTLINK_LIMIT`/`FULGUR_SHORTLINK_MAX_BYTES` を assert するテストは存在しない（grep 済み）。deploy manifest（Dockerfile/railway.toml/docker-compose.yml）は `FULGUR_SHORTLINK_DIR` のみ参照で、これらの env は設定していない。

**Step 4: build + 全テスト**

Run: `cargo build -p chart-server 2>&1 | tail -20`
Expected: 成功（guard 変更・config 追加・literal 更新でコンパイル通過）。

Run: `cargo test -p chart-server 2>&1 | tail -25`
Expected: 全 pass。

**Step 5: commit**

```bash
git add crates/chart-server/src
git commit -m "feat(chart-server): wire shortlink caps config + background sweep task (sdp)"
```

---

### Task 6: doc コメント整理 + 最終検証

**Files:**
- Modify: `crates/chart-server/src/file_store.rs`（モジュール doc の「TTL 能動削除・LRU eviction は範囲外(sdp)」行を削除し、bucket/TTL/backstop の要約に更新）
- Modify: `crates/chart-server/src/backend.rs`（`Full` の doc に FileShortlinkStore も返すようになった旨を反映。handler の「FileShortlinkStore は返さず」コメントも更新: `handlers/shortlink.rs` の `BackendError::Full` アームのコメント）

**Step 1: コメント更新**（振る舞い変更なし、テスト不要）。`file_store.rs` 冒頭 doc を「time-bucket レイアウト＋TTL sweep＋容量 backstop を持つ durable 単一ノード backend」に更新。`handlers/shortlink.rs:66-67` の「FileShortlinkStore は返さず、external adapter 用に残すアーム」を「FileShortlinkStore は容量上限超過時に返す」に修正。`tests/public_api.rs:86` の同趣旨コメント（「FileShortlinkStore は Full を返さない」）も修正。

**CHANGELOG について:** `crates/chart-server/CHANGELOG.md` は release-plz が conventional commit から自動生成する。本 PR の `feat(chart-server): ...` コミットが新エントリを生む（`MAX_BYTES` をバイト予算として復活、`MAX_ENTRIES` を追加＝旧 `LIMIT` の改名、TTL eviction 追加）。**historical な既存 BREAKING 行（8tr.6 の記録）は編集しない**（過去リリースの事実として残す）。手動編集は不要。

**Step 2: 品質ゲート**

```bash
cargo fmt -p chart-server
cargo clippy -p chart-server --all-targets 2>&1 | tail -30   # warnings 0 を確認
cargo test -p chart-server 2>&1 | tail -25                    # 全 pass
```

Expected: fmt 差分なし（or 整形適用）、clippy warning 0、test 全 pass。

**Step 3: commit**

```bash
git add crates/chart-server/src
git commit -m "docs(chart-server): update shortlink store docs for TTL/eviction (sdp)"
```

---

## 受け入れ基準（beads acceptance と対応）

- [ ] age<TTL は sweep/eviction で決して削除されない（`sweep_never_removes_a_genuinely_young_entry`, `sweep_preserves_entries_at_bucket_boundary`, `insert_over_cap_all_young_returns_full`）
- [ ] `(b+1)·W + TTL ≤ now` の bucket が削除される、境界含む（`sweep_removes_buckets_older_than_ttl`）
- [ ] over-cap insert: 期限切れあれば inline sweep で受理、全若い→Full（Task 4 の 2 テスト）
- [ ] time-bucket roundtrip・再起動persist（`insert_places_entry_in_time_bucket_dir`, `persists_across_restart`）
- [ ] 不正/traversal id は None（`invalid_id_is_treated_as_not_found`）
- [ ] カウンタ整合（`counters_are_seeded_from_existing_entries_on_construction`, `sweep_decrements_counters`）
- [ ] caps=0 で無効化（`zero_caps_mean_unlimited`）
- [ ] 既存挙動維持（`rejects_entry_exceeding_per_entry_byte_limit`, `no_temp_file_left_after_insert`, `insert_overwrites_same_id`, http_tests 全て）
- [ ] clippy warning 0 / fmt clean / 全テスト pass
