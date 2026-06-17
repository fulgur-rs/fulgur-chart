//! fulgur-chart: chart.js v4 互換 JSON から決定的な静的 SVG/PNG を生成するライブラリ。

pub mod color;
pub mod font;
pub mod frontend;
pub mod ir;
pub mod num;
pub mod palette;
pub mod scale;
pub mod text;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
