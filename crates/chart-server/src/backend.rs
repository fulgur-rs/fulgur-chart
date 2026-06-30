use std::fmt;

use async_trait::async_trait;

/// Shortlink backend の失敗理由。
///
/// `TooLarge` / `Full` は容量系の拒否で、それぞれ HTTP 413 / 503 にマップする。
/// `Unavailable` は durable backend の I/O 障害用(→ 5xx)。in-memory 実装は
/// 決して `Unavailable` を返さないが、durable adapter(8tr.6 等)が trait
/// シグネチャを変えずに使えるよう、最初からこの variant を含めておく。
#[derive(Debug)]
pub enum BackendError {
    /// 単一エントリが per-entry バイト上限を超過（→ 413）。
    TooLarge,
    /// ストアが満杯（件数 or 集約バイト上限。→ 503）。
    Full,
    /// durable backend の一時的な I/O 障害（→ 503）。in-memory は返さない。
    Unavailable(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::TooLarge => f.write_str("shortlink entry is too large"),
            BackendError::Full => f.write_str("shortlink store is full"),
            BackendError::Unavailable(e) => write!(f, "shortlink backend unavailable: {e}"),
        }
    }
}

impl std::error::Error for BackendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BackendError::Unavailable(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

/// Shortlink の保存・解決を抽象化する backend。
///
/// `Arc<dyn ShortlinkBackend>` として `AppState` に保持するため `Send + Sync`。
/// メソッドは durable 実装(I/O を伴う)を見越して async + fallible。in-memory
/// 実装は await を持たず `Unavailable` も返さないが、同じ seam を共有する。
#[async_trait]
pub trait ShortlinkBackend: Send + Sync {
    /// `id` に `query` を保存する。容量超過時は `TooLarge` / `Full`。
    async fn insert(&self, id: String, query: String) -> Result<(), BackendError>;

    /// `id` に対応する query を返す。未登録は `Ok(None)`、I/O 障害は `Err`。
    async fn get(&self, id: &str) -> Result<Option<String>, BackendError>;
}
