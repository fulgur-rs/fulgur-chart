//! monotone-X 補間を SVG の 3 次ベジエ path data へ変換する。

use crate::num::fmt_num;
use std::fmt::Write;

/// 点列を monotone-X の 3 次ベジエ path data へ変換する。
///
/// 入力座標が非有限でも SVG へ不正なトークンを出さないよう、計算前に 0 へ正規化する。
/// 同一 x の点は傾きを 0 として扱い、数値的な発散を避ける。
pub fn monotone_path(points: &[(f64, f64)]) -> String {
    let points: Vec<(f64, f64)> = points
        .iter()
        .map(|&(x, y)| (finite_or_zero(x), finite_or_zero(y)))
        .collect();

    match points.as_slice() {
        [] => return String::new(),
        &[p] => return format!("M {} {}", fmt_num(p.0), fmt_num(p.1)),
        &[p0, p1] => {
            return format!(
                "M {} {} L {} {}",
                fmt_num(p0.0),
                fmt_num(p0.1),
                fmt_num(p1.0),
                fmt_num(p1.1)
            );
        }
        _ => {}
    }

    let interior_tangents: Vec<f64> = points
        .windows(3)
        .map(|triple| slope3(triple[0], triple[1], triple[2]))
        .collect();
    let mut tangents = Vec::with_capacity(points.len());
    tangents.push(slope2(points[0], points[1], interior_tangents[0]));
    tangents.extend_from_slice(&interior_tangents);
    tangents.push(slope2(
        points[points.len() - 2],
        points[points.len() - 1],
        *interior_tangents.last().unwrap(),
    ));

    let mut d = String::new();
    write!(d, "M {} {}", fmt_num(points[0].0), fmt_num(points[0].1)).unwrap();
    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let h = p1.0 - p0.0;
        let lo = p0.1.min(p1.1);
        let hi = p0.1.max(p1.1);
        let cp1 = (
            p0.0 + h / 3.0,
            clamp_y(p0.1 + tangents[i] * h / 3.0, lo, hi),
        );
        let cp2 = (
            p1.0 - h / 3.0,
            clamp_y(p1.1 - tangents[i + 1] * h / 3.0, lo, hi),
        );
        write!(
            d,
            " C {} {} {} {} {} {}",
            fmt_num(cp1.0),
            fmt_num(cp1.1),
            fmt_num(cp2.0),
            fmt_num(cp2.1),
            fmt_num(p1.0),
            fmt_num(p1.1)
        )
        .unwrap();
    }
    d
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn secant(a: (f64, f64), b: (f64, f64)) -> f64 {
    let width = b.0 - a.0;
    if width == 0.0 {
        return 0.0;
    }
    let slope = (b.1 - a.1) / width;
    finite_or_zero(slope)
}

fn slope3(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)) -> f64 {
    let h0 = p1.0 - p0.0;
    let h1 = p2.0 - p1.0;
    let s0 = secant(p0, p1);
    let s1 = secant(p1, p2);
    let weighted = finite_or_zero((s0 * h1 + s1 * h0) / (h0 + h1));
    let limit = s0.abs().min(s1.abs()).min(0.5 * weighted.abs());
    finite_or_zero((s0.signum() + s1.signum()) * limit)
}

fn slope2(p0: (f64, f64), p1: (f64, f64), tangent: f64) -> f64 {
    if p1.0 == p0.0 {
        tangent
    } else {
        finite_or_zero((3.0 * secant(p0, p1) - tangent) / 2.0)
    }
}

fn clamp_y(value: f64, lo: f64, hi: f64) -> f64 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else if value.is_sign_negative() {
        lo
    } else {
        hi
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_y, monotone_path};

    #[test]
    fn irregular_spacing_matches_d3_monotone_x_controls() {
        assert_eq!(
            monotone_path(&[(0.0, 0.0), (1.0, 10.0), (3.0, 12.0)]),
            "M 0 0 C 0.33 4.67 0.67 9.33 1 10 C 1.67 11.33 2.33 11.67 3 12"
        );
    }

    #[test]
    fn two_points_degrade_to_a_line() {
        assert_eq!(monotone_path(&[(0.0, 1.0), (2.0, 3.0)]), "M 0 1 L 2 3");
    }

    #[test]
    fn singleton_and_duplicate_x_paths_remain_finite() {
        assert_eq!(monotone_path(&[(2.0, 3.0)]), "M 2 3");
        let duplicate_x = monotone_path(&[(1.0, 1.0), (1.0, 2.0), (2.0, 3.0)]);
        assert!(!duplicate_x.contains("NaN"));
        assert!(!duplicate_x.contains("inf"));
    }

    #[test]
    fn non_finite_controls_clamp_to_segment_bounds() {
        assert_eq!(clamp_y(f64::NEG_INFINITY, -1.0, 1.0), -1.0);
        assert_eq!(clamp_y(f64::INFINITY, -1.0, 1.0), 1.0);
        assert_eq!(clamp_y(f64::NAN, -1.0, 1.0), 1.0);
    }

    #[test]
    fn cubic_controls_stay_between_each_segment_endpoints() {
        let fixtures = [
            vec![(0.0, 0.0), (1.0, 2.0), (3.0, 5.0)],
            vec![(0.0, 5.0), (1.0, 2.0), (3.0, 0.0)],
            vec![(0.0, 2.0), (1.0, 2.0), (3.0, 2.0)],
            vec![(0.0, 0.0), (1.0, 4.0), (2.0, 1.0), (4.0, 3.0)],
        ];

        for points in fixtures {
            let path = monotone_path(&points);
            let tokens: Vec<&str> = path.split_ascii_whitespace().collect();
            let mut start_y: f64 = tokens[2].parse().unwrap();
            for cubic in tokens[3..].chunks_exact(7) {
                assert_eq!(cubic[0], "C");
                let cp1_y: f64 = cubic[2].parse().unwrap();
                let cp2_y: f64 = cubic[4].parse().unwrap();
                let end_y: f64 = cubic[6].parse().unwrap();
                let lo = start_y.min(end_y);
                let hi = start_y.max(end_y);
                assert!((lo..=hi).contains(&cp1_y), "{points:?}: {cubic:?}");
                assert!((lo..=hi).contains(&cp2_y), "{points:?}: {cubic:?}");
                start_y = end_y;
            }
        }
    }
}
