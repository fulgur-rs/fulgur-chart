//! fulgur-chart: chart.js v4 互換 JSON から決定的な静的 SVG/PNG を生成するライブラリ。

pub mod font;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
