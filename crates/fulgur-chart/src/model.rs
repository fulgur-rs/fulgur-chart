//! チャート意味モデル: chart.js と数値照合するための、解決済み色・軸目盛り・
//! counts を持つシリアライズ可能な中間表現。描画はせず IR + layout から構築する。

use crate::ir::Color;

/// 解決済み色を正規化 rgba 文字列にする(plan の正規化規約に従う)。
pub fn rgba_string(c: &Color) -> String {
    format!("rgba({},{},{},{})", c.r, c.g, c.b, fmt_alpha(c.a))
}

/// alpha を正規化整形する(>=1→"1", <=0→"0", それ以外は 3 桁丸め・末尾ゼロ除去)。
fn fmt_alpha(a: f32) -> String {
    if a >= 1.0 {
        return "1".to_string();
    }
    if a <= 0.0 {
        return "0".to_string();
    }
    let r = (a as f64 * 1000.0).round() / 1000.0;
    let mut s = format!("{r}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Color;

    #[test]
    fn rgba_opaque_uses_1() {
        let c = Color {
            r: 54,
            g: 162,
            b: 235,
            a: 1.0,
        };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,1)");
    }

    #[test]
    fn rgba_half_alpha() {
        let c = Color {
            r: 54,
            g: 162,
            b: 235,
            a: 0.5,
        };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,0.5)");
    }

    #[test]
    fn rgba_transparent_uses_0() {
        let c = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        };
        assert_eq!(rgba_string(&c), "rgba(0,0,0,0)");
    }

    #[test]
    fn rgba_trims_trailing_zeros() {
        let c = Color {
            r: 1,
            g: 2,
            b: 3,
            a: 0.25,
        };
        assert_eq!(rgba_string(&c), "rgba(1,2,3,0.25)");
    }
}
