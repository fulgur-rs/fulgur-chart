use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone)]
pub struct ShortlinkStore {
    map: Arc<DashMap<String, String>>,
    count: Arc<AtomicUsize>,
    limit: usize,
}

impl ShortlinkStore {
    pub fn new(limit: usize) -> Self {
        Self {
            map: Arc::new(DashMap::new()),
            count: Arc::new(AtomicUsize::new(0)),
            limit,
        }
    }

    pub fn insert(&self, id: String, query: String) -> bool {
        match self.map.entry(id) {
            dashmap::Entry::Occupied(mut entry) => {
                // 既存 ID なら上書き（件数変化なし）
                entry.insert(query);
                true
            }
            dashmap::Entry::Vacant(entry) => {
                // 先に件数をインクリメント（予約）して limit を超えたら戻す
                let prev = self.count.fetch_add(1, Ordering::AcqRel);
                if prev >= self.limit {
                    self.count.fetch_sub(1, Ordering::AcqRel);
                    false
                } else {
                    entry.insert(query);
                    true
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<String> {
        self.map.get(id).map(|v| v.clone())
    }
}
