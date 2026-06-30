use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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

/// `ShortlinkStore::insert` の失敗理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertError {
    /// 単一エントリが per-entry バイト上限を超過。再送しても通らない（→ 413）。
    TooLarge,
    /// ストアが満杯（件数上限 or 集約バイト上限）。一時的な拒否（→ 503）。
    Full,
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

    pub fn insert(&self, id: String, query: String) -> Result<(), InsertError> {
        let query_len = query.len();
        // per-entry 上限: このペイロード単体が大きすぎる。再送しても無駄なので即拒否。
        if query_len > self.entry_bytes {
            return Err(InsertError::TooLarge);
        }

        // 集約バイト/件数は global atomic で会計する。entry() は当該キーの shard を
        // ロックするが atomic は別キーの insert と競合しうるため、reserve-then-rollback
        // で「上限を恒久的に超えない」ことを保証する（瞬間的な超過は
        // max_concurrent × entry_bytes で上界が抑えられ、確定値は常に上限以下）。
        match self.map.entry(id) {
            dashmap::Entry::Occupied(mut entry) => {
                // 同一 id は決定的に同一 query になるため通常 old_len == query_len。
                // 一般化のためサイズ差分を正しく会計する。
                let old_len = entry.get().len();
                if query_len > old_len {
                    let additional = query_len - old_len;
                    let prev = self.bytes.fetch_add(additional, Ordering::AcqRel);
                    if prev.saturating_add(additional) > self.max_bytes {
                        self.bytes.fetch_sub(additional, Ordering::AcqRel);
                        return Err(InsertError::Full);
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
                    return Err(InsertError::Full);
                }
                // 次に件数を予約し、超えたらバイトと件数の両方を戻す。
                let prev = self.count.fetch_add(1, Ordering::AcqRel);
                if prev >= self.entry_limit {
                    self.count.fetch_sub(1, Ordering::AcqRel);
                    self.bytes.fetch_sub(query_len, Ordering::AcqRel);
                    Err(InsertError::Full)
                } else {
                    entry.insert(query);
                    Ok(())
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<String> {
        self.map.get(id).map(|v| v.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{InsertError, ShortlinkStore};

    #[test]
    fn accepts_entry_within_limits() {
        let store = ShortlinkStore::new(10, 1000, 100);
        let val = "x".repeat(50);
        assert_eq!(store.insert("a".into(), val.clone()), Ok(()));
        assert_eq!(store.get("a"), Some(val));
    }

    #[test]
    fn rejects_entry_exceeding_per_entry_byte_limit() {
        let store = ShortlinkStore::new(10, 10_000, 4);
        assert_eq!(
            store.insert("big".into(), "12345".into()),
            Err(InsertError::TooLarge)
        );
        assert!(store.get("big").is_none());
    }

    #[test]
    fn rejects_when_aggregate_byte_budget_is_full() {
        // entry_bytes は十分大きく、max_bytes=8 → 合計 8 バイトまで。
        let store = ShortlinkStore::new(10, 8, 1000);
        assert_eq!(store.insert("a".into(), "1234".into()), Ok(()));
        assert_eq!(store.insert("b".into(), "5678".into()), Ok(()));
        assert_eq!(store.insert("c".into(), "9".into()), Err(InsertError::Full));
        assert!(store.get("c").is_none());
    }

    #[test]
    fn rejects_when_entry_count_is_full() {
        // 件数上限 2、バイトは余裕。
        let store = ShortlinkStore::new(2, 10_000, 1000);
        assert_eq!(store.insert("a".into(), "x".into()), Ok(()));
        assert_eq!(store.insert("b".into(), "y".into()), Ok(()));
        assert_eq!(store.insert("c".into(), "z".into()), Err(InsertError::Full));
        assert!(store.get("c").is_none());
    }

    #[test]
    fn overwriting_same_id_does_not_double_count_bytes() {
        // max_bytes=8。同じ id に 4 バイトを 2 回入れても合計は 4 のまま。
        // その後 別 id に 4 バイトを入れても 8 に収まる。
        let store = ShortlinkStore::new(10, 8, 1000);
        assert_eq!(store.insert("a".into(), "1234".into()), Ok(()));
        assert_eq!(store.insert("a".into(), "1234".into()), Ok(()));
        assert_eq!(store.insert("b".into(), "5678".into()), Ok(()));
        assert!(store.get("b").is_some());
    }
}
