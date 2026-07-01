use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::backend::{BackendError, ShortlinkBackend};

#[derive(Clone)]
pub struct ShortlinkStore {
    map: Arc<DashMap<String, String>>,
    count: Arc<AtomicUsize>,
    /// 現在ストアに保持している query 文字列の合計バイト数（集約上限の会計用）。
    bytes: Arc<AtomicUsize>,
    /// エントリ件数の上限。
    entry_limit: usize,
    /// 全エントリ合計バイト数の上限（OOM 防止の主対策）。
    max_bytes: usize,
    /// 単一エントリ（query 文字列）のバイト数上限。
    entry_bytes: usize,
}

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

#[async_trait]
impl ShortlinkBackend for ShortlinkStore {
    async fn insert(&self, id: String, query: String) -> Result<(), BackendError> {
        let query_len = query.len();
        // per-entry 上限: このペイロード単体が大きすぎる。再送しても無駄なので即拒否。
        if query_len > self.entry_bytes {
            return Err(BackendError::TooLarge);
        }

        // 集約バイト/件数は global atomic で会計する。entry() は当該キーの shard を
        // ロックするが atomic は別キーの insert と競合しうるため、reserve-then-rollback
        // で「上限を恒久的に超えない」ことを保証する（瞬間的な超過は
        // max_concurrent × entry_bytes で上界が抑えられ、確定値は常に上限以下）。
        //
        // 注意: 以下の entry() ガード保持区間には .await を入れないこと。
        // 同期ロックを suspension 跨ぎで保持するとデッドロック/ワーカーブロックの原因になる
        // (現状この区間に await は無く安全。durable backend 実装時もこの不変条件を守ること)。
        match self.map.entry(id) {
            dashmap::Entry::Occupied(mut entry) => {
                // id は非決定的(ULID)なため通常この分岐(Occupied)は発生しない。
                // 衝突や将来の backend 実装差異に備え、サイズ差分を正しく会計する一般化ロジックとして保持。
                let old_len = entry.get().len();
                if query_len > old_len {
                    let additional = query_len - old_len;
                    let prev = self.bytes.fetch_add(additional, Ordering::AcqRel);
                    if prev.saturating_add(additional) > self.max_bytes {
                        self.bytes.fetch_sub(additional, Ordering::AcqRel);
                        return Err(BackendError::Full);
                    }
                }
                // 上書き（件数変化なし）
                entry.insert(query);
                if old_len > query_len {
                    self.bytes.fetch_sub(old_len - query_len, Ordering::AcqRel);
                }
                Ok(())
            }
            dashmap::Entry::Vacant(entry) => {
                // 先に集約バイトを予約し、超えたら戻す。
                let prev_bytes = self.bytes.fetch_add(query_len, Ordering::AcqRel);
                if prev_bytes.saturating_add(query_len) > self.max_bytes {
                    self.bytes.fetch_sub(query_len, Ordering::AcqRel);
                    return Err(BackendError::Full);
                }
                // 次に件数を予約し、超えたらバイトと件数の両方を戻す。
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

#[cfg(test)]
mod tests {
    use super::ShortlinkStore;
    use crate::backend::{BackendError, ShortlinkBackend};

    #[tokio::test]
    async fn accepts_entry_within_limits() {
        let store = ShortlinkStore::new(10, 1000, 100);
        let val = "x".repeat(50);
        let r = store.insert("a".into(), val.clone()).await;
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(store.get("a").await.unwrap(), Some(val));
    }

    #[tokio::test]
    async fn rejects_entry_exceeding_per_entry_byte_limit() {
        let store = ShortlinkStore::new(10, 10_000, 4);
        let r = store.insert("big".into(), "12345".into()).await;
        assert!(matches!(&r, Err(BackendError::TooLarge)), "{r:?}");
        let g = store.get("big").await.unwrap();
        assert!(g.is_none(), "{g:?}");
    }

    #[tokio::test]
    async fn rejects_when_aggregate_byte_budget_is_full() {
        // entry_bytes は十分大きく、max_bytes=8 → 合計 8 バイトまで。
        let store = ShortlinkStore::new(10, 8, 1000);
        let r = store.insert("a".into(), "1234".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let r = store.insert("b".into(), "5678".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let r = store.insert("c".into(), "9".into()).await;
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
        let g = store.get("c").await.unwrap();
        assert!(g.is_none(), "{g:?}");
    }

    #[tokio::test]
    async fn rejects_when_entry_count_is_full() {
        // 件数上限 2、バイトは余裕。
        let store = ShortlinkStore::new(2, 10_000, 1000);
        let r = store.insert("a".into(), "x".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let r = store.insert("b".into(), "y".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let r = store.insert("c".into(), "z".into()).await;
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
        let g = store.get("c").await.unwrap();
        assert!(g.is_none(), "{g:?}");
    }

    #[tokio::test]
    async fn overwriting_same_id_does_not_double_count_bytes() {
        // max_bytes=8。同じ id に 4 バイトを 2 回入れても合計は 4 のまま。
        // その後 別 id に 4 バイトを入れても 8 に収まる。
        let store = ShortlinkStore::new(10, 8, 1000);
        let r = store.insert("a".into(), "1234".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let r = store.insert("a".into(), "1234".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let r = store.insert("b".into(), "5678".into()).await;
        assert!(r.is_ok(), "{r:?}");
        let g = store.get("b").await.unwrap();
        assert!(g.is_some(), "{g:?}");
    }

    #[tokio::test]
    async fn overwriting_with_invalid_query_retains_old_value() {
        // 上書き失敗時に古い値が保持され、バイト会計がロールバックされること
        // （TooLarge / Full 双方）を検証する。
        let store = ShortlinkStore::new(10, 8, 5);
        let r = store.insert("a".into(), "1234".into()).await;
        assert!(r.is_ok(), "{r:?}");

        // 1. per-entry 上限超過による上書き失敗 (TooLarge) → 古い値を保持。
        let r = store.insert("a".into(), "123456".into()).await;
        assert!(matches!(&r, Err(BackendError::TooLarge)), "{r:?}");
        assert_eq!(store.get("a").await.unwrap(), Some("1234".into()));

        // 2. 正常な上書き（5 バイト）。
        let r = store.insert("a".into(), "12345".into()).await;
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(store.get("a").await.unwrap(), Some("12345".into()));

        // 別エントリ "b" を 3 バイトで挿入 → 合計 8 バイトで満杯。
        let r = store.insert("b".into(), "123".into()).await;
        assert!(r.is_ok(), "{r:?}");

        // 3. 集約上限超過による上書き失敗 (Full) → 古い値を保持。
        let r = store.insert("b".into(), "1234".into()).await;
        assert!(matches!(&r, Err(BackendError::Full)), "{r:?}");
        assert_eq!(store.get("b").await.unwrap(), Some("123".into()));

        // ロールバックが正しく行われ、バイトがリークしていないこと
        // （より小さい値での上書きが通る）を検証。
        let r = store.insert("b".into(), "12".into()).await;
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(store.get("b").await.unwrap(), Some("12".into()));
    }
}
