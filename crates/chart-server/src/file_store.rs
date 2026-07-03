use std::io;
use std::path::{Path, PathBuf};

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
}

impl FileShortlinkStore {
    /// `root` ディレクトリを作成（存在すれば再利用）して store を構築する。
    /// ディレクトリを作成・書き込みできない場合はエラー（呼び出し側=main で fail-fast）。
    pub async fn new(root: impl AsRef<Path>, entry_bytes: usize) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).await?;
        let store = Self { root, entry_bytes };
        // create_dir_all は既存の書き込み不可 dir（read-only / root 所有の mount 等）でも
        // Ok を返す。実際に write→rename→remove の probe を行い、書けない dir を起動時に
        // 検出して fail-fast する（さもないと最初の /chart/create まで問題が顕在化しない）。
        store.probe_writable().await?;
        Ok(store)
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
        tokio::task::spawn_blocking(move || {
            write_then_rename(&tmp_path, &final_path, query.as_bytes())
        })
        .await
        .map_err(|e| BackendError::Unavailable(Box::new(e)))?
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
}
