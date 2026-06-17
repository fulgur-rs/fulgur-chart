//! 決定的な数値フォーマット。SVG 座標・寸法はすべてこれを通す。

/// 小数2桁に丸め、末尾の不要な 0 と小数点を除去する。
/// 負ゼロは "0" に正規化。ロケール非依存。
pub fn fmt_num(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded }; // -0.0 → 0.0
    let mut s = format!("{rounded:.2}");
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

    #[test]
    fn formats_with_two_decimals() {
        assert_eq!(fmt_num(1.0), "1");
        assert_eq!(fmt_num(1.005), "1");      // f64表現上 1.00499… のため "1" に丸まる
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(1.25), "1.25");
        assert_eq!(fmt_num(1.234), "1.23");
        assert_eq!(fmt_num(-0.0), "0");        // 負ゼロを正規化
        assert_eq!(fmt_num(100.0), "100");
    }
}
