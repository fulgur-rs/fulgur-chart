use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use tokio::fs;
use ulid::Ulid;

use crate::backend::{BackendError, ShortlinkBackend};

/// time-bucket ディレクトリの幅（ms）。`bucket = ulid_ms(id) / WIDTH_MS` で 1h 単位に束ねる。
const WIDTH_MS: u64 = 3_600_000; // 1h

/// ファイルシステム上に id→query を永続化する durable な単一ノード backend。
///
/// 1 エントリ = 1 ファイル（ファイル名 = id、内容 = query 文字列）。filesystem の
/// パスが id→artifact 対応そのものになるため in-memory インデックスは持たない。
/// `root` が永続ストレージ上にある限り、再起動/デプロイをまたいでリンクを維持する
/// （揮発 FS 上では再デプロイで消える。Docker/Railway では volume マウントが前提）。
/// マルチノード/LB ハズレは解決しない（ローカルディスクはノード固有）。
/// TTL 能動削除・LRU eviction は範囲外（sdp）。
pub struct FileShortlinkStore {
    root: PathBuf,
    /// 単一エントリ（query 文字列）のバイト数上限。超過は `TooLarge`（→413）。
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

impl FileShortlinkStore {
    /// `root` ディレクトリを作成（存在すれば再利用）して store を構築する。
    /// ディレクトリを作成・書き込みできない場合はエラー（呼び出し側=main で fail-fast）。
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
        // create_dir_all は既存の書き込み不可 dir（read-only / root 所有の mount 等）でも
        // Ok を返す。実際に write→rename→remove の probe を行い、書けない dir を起動時に
        // 検出して fail-fast する（さもないと最初の /chart/create まで問題が顕在化しない）。
        store.probe_writable().await?;
        // 既存エントリを 1 度だけ走査してカウンタを seed（accelerator。真実源はディスク）。
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
    ///
    /// エラー方針は意図的に混在させている: `read_dir` の失敗は起動時 fail-fast
    /// （`new()` が Err を返す）。一方で個々のファイルの `metadata()` 失敗は
    /// best-effort でスキップする（カウンタは soft accelerator で、真実源はディスク、
    /// 走査取りこぼしは次回起動の再 scan で自己修復する）。
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
            if name.parse::<u64>().is_err() {
                continue;
            }
            if !b.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let mut entries = fs::read_dir(b.path()).await?;
            while let Some(e) = entries.next_entry().await? {
                let fname = e.file_name();
                // temp ファイルは数えない。
                if fname.to_str().map(|s| s.ends_with(".tmp")).unwrap_or(true) {
                    continue;
                }
                if let Ok(meta) = e.metadata().await
                    && meta.is_file()
                {
                    count += 1;
                    bytes += meta.len();
                }
            }
        }
        Ok((count, bytes))
    }

    /// new_len バイトの追加が件数 or バイト上限を超えるか（0 上限は無制限）。
    fn would_exceed(&self, new_len: u64) -> bool {
        let count = self.count.load(Ordering::Relaxed);
        let bytes = self.bytes.load(Ordering::Relaxed);
        (self.max_entries != 0 && count.saturating_add(1) > self.max_entries)
            || (self.max_bytes != 0 && bytes.saturating_add(new_len) > self.max_bytes)
    }

    /// `root` が実際に書き込み可能かを起動時に検証する。insert と同じ write→rename
    /// 経路を最小ペイロードで一度だけ実行し、成否に関わらず probe ファイルを掃除する。
    async fn probe_writable(&self) -> io::Result<()> {
        // 複数インスタンスが同一 dir を共有しても衝突しないよう一意名を使う。
        let name = format!(".fulgur-write-probe-{}", Ulid::new());
        let tmp = self.root.join(format!("{name}.tmp"));
        let final_path = self.root.join(&name);
        fs::write(&tmp, b"").await?;
        match fs::rename(&tmp, &final_path).await {
            Ok(()) => {
                let _ = fs::remove_file(&final_path).await;
                Ok(())
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp).await;
                Err(e)
            }
        }
    }

    /// id を検証し `root/{bucket}/{id}` パスへ写像する。ULID 以外は None。
    /// ULID のパース成功が 26 文字 Crockford base32 を保証し path traversal を構造的に排除する。
    fn path_for(&self, id: &str) -> Option<PathBuf> {
        let ulid = Ulid::from_string(id).ok()?;
        let bucket = ulid.timestamp_ms() / WIDTH_MS;
        Some(self.root.join(bucket.to_string()).join(id))
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
        // 容量 backstop: 件数 or バイト上限を超えるなら即 Full(→503)。
        // Task 4 で inline sweep を挟む。現段階では即 Full。
        // 注: この would_exceed→（write→）fetch_add の並びは意図的に非アトミック。
        // 並行 insert 下では cap を僅かに超過し得るが、soft backstop として許容する
        // 性質であり、ロックは張らない（真実源はディスク、再起動時に scan で自己修復）。
        let new_len = query.len() as u64;
        if self.would_exceed(new_len) {
            return Err(BackendError::Full);
        }
        // 同一 bucket ディレクトリ内の temp ファイルに書いてから rename で atomic に配置する
        // （並行 resolve の torn read 防止。同一 dir/同一 fs なので rename は atomic）。
        // ULID は一意なので temp 名（{id}.tmp）の衝突は起きない。fsync はしない
        // （保証は再起動/デプロイ耐性であって電源断耐性ではない）。
        let bucket_dir = final_path
            .parent()
            .expect("path_for always has a bucket parent")
            .to_path_buf();
        let tmp_path = bucket_dir.join(format!("{id}.tmp"));
        // query と path を所有権ごと単一の blocking タスクへ move し、同期 std::fs で
        // bucket dir 生成→write→rename を 1 回の spawn_blocking に畳む。tokio::fs に借用
        // スライスを渡すと payload を to_owned で複製するうえ dispatch も増えるため避ける。
        let result = tokio::task::spawn_blocking(move || {
            write_then_rename(&tmp_path, &final_path, query.as_bytes())
        })
        .await
        .map_err(|e| BackendError::Unavailable(Box::new(e)))?
        .map_err(|e| BackendError::Unavailable(Box::new(e)));
        // 書き込み成功後にのみカウンタを進める（accelerator。TooLarge/Full/Unavailable では進めない）。
        result?;
        self.count.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(new_len, Ordering::Relaxed);
        Ok(())
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

/// bucket dir を掘ってから temp に同期 I/O で書いて rename する（呼び出し側の spawn_blocking 内で実行）。
/// write/rename いずれの失敗でも temp を掃除して漏らさない。
fn write_then_rename(tmp: &Path, final_path: &Path, data: &[u8]) -> io::Result<()> {
    // time-bucket 化で final は root 直下ではなくなったため bucket dir を遅延生成する。
    // parent は path_for が必ず付与する（tmp と同一 dir）。生成失敗時は temp 未作成なので掃除不要。
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Err(e) = std::fs::write(tmp, data).and_then(|()| std::fs::rename(tmp, final_path)) {
        // write が temp を作る前に失敗した場合でも remove_file の失敗は無害（let _ で無視）。
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::FileShortlinkStore;
    use crate::backend::{BackendError, ShortlinkBackend};
    use tempfile::TempDir;
    use ulid::Ulid;

    /// 有効な ULID 形状の id（26 文字 Crockford base32）。
    fn valid_id() -> String {
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()
    }

    /// 指定 ms の ULID を生成（bucket/TTL の決定的テスト用）。
    fn id_at_ms(ms: u64) -> String {
        Ulid::from_parts(ms, 0).to_string()
    }

    async fn store(entry_bytes: usize) -> (FileShortlinkStore, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let s = FileShortlinkStore::new(dir.path(), entry_bytes)
            .await
            .unwrap();
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
        // 前半は長さゲート。後半は「26 文字だが Crockford base32 外（末尾 `/`・`.`）」で、
        // 長さチェックではなく ULID decode 拒否 = path traversal 経路そのものを検証する。
        for bad in [
            "../../etc/passwd",
            "..",
            "a/b",
            "short",
            long.as_str(),
            "01ARZ3NDEKTSV4RRFFQ69G5FA/",
            "01ARZ3NDEKTSV4RRFFQ69G5FA.",
        ] {
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
        let bucket = Ulid::from_string(&id).unwrap().timestamp_ms() / super::WIDTH_MS;
        let bucket_dir = d.path().join(bucket.to_string());
        let mut rd = tokio::fs::read_dir(&bucket_dir).await.unwrap();
        let mut names = vec![];
        while let Some(e) = rd.next_entry().await.unwrap() {
            names.push(e.file_name().to_string_lossy().into_owned());
        }
        assert_eq!(names, vec![id]);
    }

    #[tokio::test]
    async fn insert_places_entry_in_time_bucket_dir() {
        let (s, d) = store(1_000).await;
        let ms = 1_700_000_000_000; // 任意の固定時刻
        let id = id_at_ms(ms);
        s.insert(id.clone(), "c=x&f=svg".into()).await.unwrap();
        let bucket = ms / super::WIDTH_MS;
        let expected = d.path().join(bucket.to_string()).join(&id);
        assert!(
            expected.is_file(),
            "entry should live at root/{{bucket}}/{{id}}: {expected:?}"
        );
        assert_eq!(s.get(&id).await.unwrap(), Some("c=x&f=svg".into()));
    }

    /// caps を明示した store ヘルパー（unlimited=0）。
    async fn store_capped(
        entry_bytes: usize,
        max_bytes: u64,
        max_entries: u64,
    ) -> (FileShortlinkStore, TempDir) {
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
        s.insert(id_at_ms(1_700_000_000_000), "a".into())
            .await
            .unwrap();
        let r = s.insert(id_at_ms(1_700_000_000_001), "b".into()).await;
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
    }

    #[tokio::test]
    async fn insert_returns_full_when_byte_budget_reached() {
        let (s, _d) = store_capped(1_000, 4, 0).await; // バイト上限 4
        s.insert(id_at_ms(1_700_000_000_000), "abc".into())
            .await
            .unwrap(); // 3B
        let r = s.insert(id_at_ms(1_700_000_000_001), "de".into()).await; // +2B > 4
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
    }

    #[tokio::test]
    async fn counters_are_seeded_from_existing_entries_on_construction() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap();
            s.insert(id_at_ms(1_700_000_000_000), "abc".into())
                .await
                .unwrap();
        }
        // 再構築（件数上限 1）→ 既存 1 件を数えているので次 insert は Full
        let s2 = FileShortlinkStore::new(dir.path(), 1_000)
            .await
            .unwrap()
            .with_capacity(0, 1);
        let r = s2.insert(id_at_ms(1_700_000_000_001), "x".into()).await;
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
    }

    /// 上の姉妹テスト。COUNT ではなく scan_totals の BYTE seed 経路を検証する。
    #[tokio::test]
    async fn byte_counter_is_seeded_from_existing_entries_on_construction() {
        let dir = tempfile::tempdir().unwrap();
        {
            let s = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap();
            s.insert(id_at_ms(1_700_000_000_000), "abc".into()) // 3B
                .await
                .unwrap();
        }
        // 再構築（バイト上限 3・件数無制限）。scan が bytes=3 を seed していれば次 insert(+1B>3) は Full。
        // seed していなければ bytes=0 で通ってしまう＝この assert が BYTE seed 経路を実証する。
        let s2 = FileShortlinkStore::new(dir.path(), 1_000)
            .await
            .unwrap()
            .with_capacity(3, 0);
        let r = s2.insert(id_at_ms(1_700_000_000_001), "x".into()).await; // +1B > 3
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
    }

    /// scan_totals が bucket dir 内の `.tmp` と root 直下の非 bucket ファイルを数えないこと。
    #[tokio::test]
    async fn scan_seed_ignores_tmp_and_non_bucket_files() {
        let dir = tempfile::tempdir().unwrap();
        let ms = 1_700_000_000_000;
        let id = id_at_ms(ms);
        {
            let s = FileShortlinkStore::new(dir.path(), 1_000).await.unwrap();
            s.insert(id.clone(), "abc".into()).await.unwrap(); // 実エントリ 1 件
        }
        // bucket dir 内に `.tmp` を、root 直下に probe 風ファイルを手動配置（どちらも数えてはならない）。
        let bucket = ms / super::WIDTH_MS;
        let bucket_dir = dir.path().join(bucket.to_string());
        tokio::fs::write(bucket_dir.join(format!("{id}.tmp")), b"junk")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join(".fulgur-write-probe-xyz"), b"junk")
            .await
            .unwrap();
        // 件数上限 2 で再構築。フィルタが効いていれば seed=1 なので 1 件は通り、その次で Full。
        // `.tmp`/probe を数えていれば seed>=2 で最初の insert が即 Full になり、下の expect が落ちる。
        let s2 = FileShortlinkStore::new(dir.path(), 1_000)
            .await
            .unwrap()
            .with_capacity(0, 2);
        s2.insert(id_at_ms(ms + 1), "y".into())
            .await
            .expect("seed must count only the 1 real entry, ignoring .tmp/probe files");
        let r = s2.insert(id_at_ms(ms + 2), "z".into()).await; // count 2→上限到達
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
    }

    #[tokio::test]
    async fn zero_caps_mean_unlimited() {
        let (s, _d) = store_capped(1_000, 0, 0).await;
        for i in 0..50u64 {
            s.insert(id_at_ms(1_700_000_000_000 + i), "x".into())
                .await
                .unwrap();
        }
    }
}
