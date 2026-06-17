//! chart.js v4 既定カラーパレット。

use crate::ir::Color;

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
