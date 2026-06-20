//! カラーパレットとテーマ定義。

use crate::ir::{Color, Theme};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// chart.js v4 の既定色循環（Colors プラグインのデフォルト borderColor 列）。
pub static PALETTE: &[Color] = &[
    rgb(54, 162, 235),  // #36A2EB blue
    rgb(255, 99, 132),  // #FF6384 red
    rgb(255, 159, 64),  // #FF9F40 orange
    rgb(255, 205, 86),  // #FFCD56 yellow
    rgb(75, 192, 192),  // #4BC0C0 green
    rgb(153, 102, 255), // #9966FF purple
    rgb(201, 203, 207), // #C9CBCF grey
];

pub fn palette_color(i: usize) -> Color {
    PALETTE[i % PALETTE.len()]
}

/// Vega-Lite デフォルトカラースキーム（Tableau10）。
pub static VEGALITE_PALETTE: &[Color] = &[
    rgb(76, 120, 168),  // #4c78a8 steel blue
    rgb(245, 133, 24),  // #f58518 orange
    rgb(228, 87, 86),   // #e45756 red
    rgb(114, 183, 178), // #72b7b2 teal
    rgb(84, 162, 75),   // #54a24b green
    rgb(238, 202, 59),  // #eeca3b yellow
    rgb(178, 121, 162), // #b279a2 purple
    rgb(255, 157, 166), // #ff9da6 pink
    rgb(157, 117, 93),  // #9d755d brown
    rgb(186, 176, 172), // #bab0ac light gray
];

/// Vega-Lite のデフォルトビジュアルスタイルを返す。
/// パレット: Tableau10、背景: 白、グリッド: #ddd、テキスト: #333。
pub fn vegalite_theme() -> Theme {
    Theme {
        palette: VEGALITE_PALETTE.to_vec(),
        grid_color: rgb(221, 221, 221), // #dddddd
        text_color: rgb(51, 51, 51),    // #333333
        background: Some(rgb(255, 255, 255)),
        font_size: 11.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_through_palette() {
        let n = PALETTE.len();
        assert!(n >= 6);
        // 循環すること
        assert_eq!(palette_color(0), palette_color(n));
        assert_eq!(palette_color(1), palette_color(n + 1));
    }

    #[test]
    fn first_color_is_chartjs_blue() {
        let c = palette_color(0);
        assert_eq!((c.r, c.g, c.b), (54, 162, 235)); // #36A2EB
    }
}
