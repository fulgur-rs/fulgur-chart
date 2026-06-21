//! 決定的な数値フォーマット。SVG 座標・寸法はすべてこれを通す。

/// 小数2桁に丸め、末尾の不要な 0 と小数点を除去する。
/// 負ゼロは "0" に正規化。ロケール非依存。
/// 非有限値（NaN / ±Infinity）は不正な SVG トークンになるため "0" に落とす。
/// この関数は全座標の最終出口であり、ここで値を有限に保証する。
pub fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    // 巨大な有限値では v * 100.0 が ±Infinity に溢れて丸めが破綻する。その場合は
    // 2 桁丸めを諦め、値そのものをフォーマットする（有限なので "inf"/"NaN" は出ない）。
    // ここで "0" に潰すと有限の入力値が別物として描画されてしまうため避ける。
    let rounded = if v.abs() <= f64::MAX / 100.0 {
        (v * 100.0).round() / 100.0
    } else {
        v
    };
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
        assert_eq!(fmt_num(1.005), "1"); // f64表現上 1.00499… のため "1" に丸まる
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(1.25), "1.25");
        assert_eq!(fmt_num(1.234), "1.23");
        assert_eq!(fmt_num(-0.0), "0"); // 負ゼロを正規化
        assert_eq!(fmt_num(100.0), "100");
    }

    #[test]
    fn non_finite_falls_back_to_zero() {
        // NaN / ±Inf は不正な SVG トークンなので "0" に落とす
        assert_eq!(fmt_num(f64::NAN), "0");
        assert_eq!(fmt_num(f64::INFINITY), "0");
        assert_eq!(fmt_num(f64::NEG_INFINITY), "0");
    }

    #[test]
    fn fmt_num_huge_finite_does_not_emit_inf() {
        let s = fmt_num(1e308);
        assert!(!s.contains("inf") && !s.contains("NaN"), "got {s}");
        // 巨大でも有限の値は "0" に化けず、実際の桁数で描画される。
        assert_ne!(s, "0", "huge finite must not collapse to 0: {s}");
        assert!(s.starts_with('1') && s.len() > 100, "got {s}");
    }
}
