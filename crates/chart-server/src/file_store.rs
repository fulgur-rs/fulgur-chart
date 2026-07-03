use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;

use crate::backend::{BackendError, ShortlinkBackend};

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
        // ULID 文字列は 26 文字。長さ + ASCII 英数字のみの検証で `/`・`.`・`\` を含む
        // id は構造的に弾かれ、path traversal は起こり得ない。
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
        // query と path を所有権ごと単一の blocking タスクへ move し、同期 std::fs で
        // 書く。tokio::fs::write に借用スライスを渡すと payload を to_owned で複製する
        // うえ write/rename が spawn_blocking を 2 回張るため、それを避けて 1 回に畳む。
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

/// temp に同期 I/O で書いて rename する（呼び出し側の spawn_blocking 内で実行）。
/// write/rename いずれの失敗でも temp を掃除して漏らさない。
fn write_then_rename(tmp: &Path, final_path: &Path, data: &[u8]) -> io::Result<()> {
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

    /// 有効な ULID 形状の id（26 文字 Crockford base32）。
    fn valid_id() -> String {
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()
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
