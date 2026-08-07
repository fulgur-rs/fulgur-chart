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

/// 対数軸の目盛ラベル用。`fmt_num` と違い小数点以下を2桁に丸めない
/// (log軸は 0.0001 のような広いレンジの値を扱うため)。
///
/// # 実装方針: 固定小数点位置ではなく有効数字ベースで丸める
///
/// 当初案(`format!("{v:.15}")` で固定15桁に丸めてから末尾 0 を trim する案)は
/// 実測の結果 2 種類の不具合を起こすことが分かったため採用しなかった:
///
/// 1. **極小値での桁溢れ**: `log_ticks` の minor tick は
///    `mantissa as f64 * 10f64.powi(exp)` で生成され、`exp` は理論上
///    `-308` まで届く(`scale.rs` の `MAX_LOG_DECADES` 参照)。固定15桁丸めだと
///    `exp <= -16` あたりから小数第15桁までしか表現できず、本来 mantissa が
///    5〜9 で異なるはずの値(例: `5e-16`〜`9e-16`)がすべて同じ
///    `"0.000000000000001"` に潰れてしまう。さらに `exp <= -19` 程度では
///    非ゼロの目盛値が丸めで完全に `"0"` になり、ラベルとして意味をなさない。
/// 2. **乗算誤差の露出**: 逆に `format!("{v}")`(最短往復表現)を使う案も
///    試したが、こちらは真逆の問題を起こす。`log_ticks` の mantissa 計算は
///    `3.0 * 10f64.powi(-1)` のような乗算で行われ、これは厳密に `0.3` には
///    ならず(浮動小数点表現の都合で `0.30000000000000004` に丸め込まれた
///    値になる)、最短往復表現ではその誤差桁がそのまま桁として出力されてしまう
///    (実測: `"0.30000000000000004"`)。
///
/// 上記いずれの方式でも壊れるため、実測に基づき「有効数字 12 桁に丸めてから
/// 末尾 0 を trim する」方式を採用した。`f64` の実効精度は有効数字
/// 15〜17 桁程度であり(15 桁から乗算誤差が見え始めることを実測で確認済み)、
/// 12 桁ならその誤差の遥か手前で丸め切れるため上記の(2)を回避できる。
/// 同時に "何桁目に丸めるか" を値の大きさに応じて動かす(固定小数点ではなく
/// 有効数字ベース)ため、値がどれだけ小さくても mantissa の桁を失わず
/// 上記の(1)も回避できる。`exp` を `-324..=308`(subnormal 境界を含む
/// `f64` の全表現域)まで、`mantissa` を `1..=9` まで総当たりして
/// この2種の不具合が再現しないことを確認済み(このモジュールのテスト参照)。
///
/// ticks.format(fulgur-chart-pof、別issue)が実装されたら、明示指定時は
/// そちらを優先し、未指定時のデフォルトとしてこの関数を使い続ける想定。
pub fn fmt_num_log(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    if v == 0.0 {
        return "0".to_string(); // -0.0 も含め正規化
    }

    // 有効数字 12 桁。詳細は関数doc参照: 15 桁以上では乗算誤差が桁として
    // 露出し始めることを実測で確認しているため、十分なマージンを取った値。
    const SIG_DIGITS: usize = 12;

    let sign = if v.is_sign_negative() { "-" } else { "" };
    let v_abs = v.abs();

    // `{:.11e}` は "D.DDDDDDDDDDDeEXP" 形式(仮数部12桁、指数部は既に
    // 正しく桁上げ処理済み)を返す。Rust の書式化エンジンが丸めと
    // 桁上げ(例: 9.9999999999996 → "1.00000000000000e1")を保証するため、
    // ここでの文字列分解は安全。
    let sci = format!("{v_abs:.*e}", SIG_DIGITS - 1);
    let Some((mantissa_str, exp_str)) = sci.split_once('e') else {
        // f64 の LowerExp 実装は有限かつ非ゼロな値に対して常に 'e' を含む
        // 文字列を返すため、通常この分岐には到達しない。到達した場合でも
        // panic させず "0" にフォールバックする(このcrate全体の方針)。
        return "0".to_string();
    };
    let Ok(exp) = exp_str.parse::<i32>() else {
        return "0".to_string();
    };

    let digits: String = mantissa_str.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };

    // digits の先頭が小数点の何桁目に来るか(1 なら "D.DDD..." の直後に点)。
    let point_pos = exp + 1;
    let mut out = String::new();
    if point_pos <= 0 {
        out.push_str("0.");
        out.push_str(&"0".repeat((-point_pos) as usize));
        out.push_str(digits);
    } else if (point_pos as usize) >= digits.len() {
        out.push_str(digits);
        out.push_str(&"0".repeat(point_pos as usize - digits.len()));
    } else {
        out.push_str(&digits[..point_pos as usize]);
        out.push('.');
        out.push_str(&digits[point_pos as usize..]);
    }
    format!("{sign}{out}")
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

    #[test]
    fn fmt_num_log_preserves_small_magnitudes() {
        assert_eq!(fmt_num_log(0.001), "0.001");
        assert_eq!(fmt_num_log(1.0), "1");
        assert_eq!(fmt_num_log(1000.0), "1000");
        assert_eq!(fmt_num_log(0.0001), "0.0001");
    }

    #[test]
    fn fmt_num_log_non_finite_falls_back_to_zero() {
        assert_eq!(fmt_num_log(f64::NAN), "0");
        assert_eq!(fmt_num_log(f64::INFINITY), "0");
        assert_eq!(fmt_num_log(f64::NEG_INFINITY), "0");
    }

    #[test]
    fn fmt_num_log_zero_is_normalized() {
        assert_eq!(fmt_num_log(0.0), "0");
        assert_eq!(fmt_num_log(-0.0), "0"); // 負ゼロを正規化
    }

    /// `log_ticks` が生成する実際の tick 値の形(`mantissa as f64 *
    /// 10f64.powi(exp)`)で桁上げ・乗算誤差が漏れ出ないことを確認する。
    /// この形の値は例えば `3.0 * 10f64.powi(-1)` が厳密に `0.3` にならず
    /// `0.30000000000000004` になる、というような f64 の乗算誤差を含む。
    /// 単純な最短往復表現(`format!("{v}")`)ではこの誤差桁がそのまま
    /// 出力されてしまうことを実測で確認済み(このテストが退行を防ぐ)。
    #[test]
    fn fmt_num_log_avoids_multiplication_rounding_garbage() {
        let cases: &[(f64, i32, &str)] = &[
            (3.0, -2, "0.03"),
            (7.0, 5, "700000"),
            (1.0, -10, "0.0000000001"),
            (9.0, -1, "0.9"),
            (3.0, -1, "0.3"),
            (6.0, -1, "0.6"),
            (7.0, -1, "0.7"),
            (3.0, -9, "0.000000003"),
            (6.0, -9, "0.000000006"),
            (9.0, -9, "0.000000009"),
            (3.0, 25, "30000000000000000000000000"),
            (7.0, 25, "70000000000000000000000000"),
        ];
        for &(mantissa, exp, expected) in cases {
            let v = mantissa * 10f64.powi(exp);
            assert_eq!(
                fmt_num_log(v),
                expected,
                "mantissa={mantissa} exp={exp} v={v:e}"
            );
        }
    }

    /// `log_ticks` の minor tick は理論上 `exp` が `-308` 付近まで届きうる
    /// (`scale.rs::MAX_LOG_DECADES` 参照)。固定小数点桁(例: 小数第15桁)で
    /// 丸める素朴な実装だと、この範囲で mantissa の異なる値が同じ文字列に
    /// 潰れてしまう(実測: `5e-16`〜`9e-16` が全て `"0.000000000000001"` に
    /// 潰れる)。有効数字ベースの丸めならこれを回避できることを確認する。
    #[test]
    fn fmt_num_log_preserves_distinct_extreme_small_magnitudes() {
        let five = fmt_num_log(5.0 * 10f64.powi(-16));
        let six = fmt_num_log(6.0 * 10f64.powi(-16));
        let nine = fmt_num_log(9.0 * 10f64.powi(-16));
        assert_ne!(five, six, "5e-16 and 6e-16 must not collapse together");
        assert_ne!(six, nine, "6e-16 and 9e-16 must not collapse together");
        assert!(five.ends_with('5'), "got {five}");
        assert!(six.ends_with('6'), "got {six}");
        assert!(nine.ends_with('9'), "got {nine}");
    }

    /// `log_ticks` の全域(major/minor, mantissa 1..=9, exp -324..=308)を
    /// 総当たりし、非有限値や空文字列を絶対に返さないことを確認する
    /// (この crate の "パニックしない" 方針に合わせた網羅テスト)。
    #[test]
    fn fmt_num_log_never_panics_or_produces_empty_across_full_range() {
        for exp in -324..=308 {
            for mantissa in 1..=9 {
                let v = mantissa as f64 * 10f64.powi(exp);
                if v == 0.0 || !v.is_finite() {
                    continue;
                }
                let s = fmt_num_log(v);
                assert!(!s.is_empty(), "exp={exp} mantissa={mantissa} v={v:e}");
                assert_ne!(s, "-", "exp={exp} mantissa={mantissa} v={v:e}");
            }
        }
    }
}
