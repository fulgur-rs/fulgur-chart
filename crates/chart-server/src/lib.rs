//! chart-server library crate.
//!
//! HTTP rendering server の本体。bin(`main.rs`)は薄い composition root として
//! このライブラリの `build_router` を呼ぶだけ。外部 crate はこのライブラリに
//! 依存し、`ShortlinkBackend` を実装した durable backend を `build_router` に
//! inject できる。

mod backend;
mod config;
mod handlers;
mod render;
mod response;
mod server;
mod state;
mod store;

pub use backend::{BackendError, ShortlinkBackend};
pub use config::Config;
pub use server::build_router;
pub use store::ShortlinkStore;
