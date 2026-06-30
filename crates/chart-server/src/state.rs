use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::render::Compression;
use crate::store::ShortlinkStore;

#[derive(Clone)]
pub struct AppState {
    pub store: ShortlinkStore,
    pub semaphore: Arc<Semaphore>,
    pub render_timeout_ms: u64,
    /// サーバ全体に適用する PNG 圧縮プリセット（起動時設定）。
    pub png_compression: Compression,
}
