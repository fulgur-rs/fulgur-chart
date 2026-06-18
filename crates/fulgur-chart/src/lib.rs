//! Render chart.js-compatible JSON specs to deterministic static SVG/PNG.

pub mod color;
pub mod font;
pub mod frontend;
pub mod ir;
pub mod layout;
pub mod num;
pub mod palette;
pub mod raster;
pub mod render;
pub mod scale;
pub mod scene;
pub mod schema;
pub mod svg;
pub mod text;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
