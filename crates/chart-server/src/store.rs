use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct ShortlinkStore {
    map: Arc<DashMap<String, String>>,
    limit: usize,
}

impl ShortlinkStore {
    pub fn new(limit: usize) -> Self {
        Self {
            map: Arc::new(DashMap::new()),
            limit,
        }
    }

    pub fn insert(&self, id: String, query: String) -> bool {
        if self.map.len() >= self.limit {
            return false;
        }
        self.map.insert(id, query);
        true
    }

    pub fn get(&self, id: &str) -> Option<String> {
        self.map.get(id).map(|v| v.clone())
    }
}
