use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::store::ShortlinkStore;

#[derive(Clone)]
pub struct AppState {
    pub store: ShortlinkStore,
    pub semaphore: Arc<Semaphore>,
    pub render_timeout_ms: u64,
}
