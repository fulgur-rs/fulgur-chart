use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::backend::ShortlinkBackend;
use crate::render::{Compression, WebpPolicy};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn ShortlinkBackend>,
    pub semaphore: Arc<Semaphore>,
    pub render_timeout_ms: u64,
    /// サーバ全体に適用する PNG 圧縮プリセット（起動時設定）。
    pub png_compression: Compression,
    /// WebP 出力のポリシー（有効/無効・面積予算。起動時設定）。
    pub webp: WebpPolicy,
    /// shortlink 解決成功時の Cache-Control max-age に使う保証有効期限（秒）。
    pub shortlink_ttl_seconds: u64,
}
