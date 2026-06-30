//! line/area 用デシメーション（Chart.js options.plugins.decimation 互換）。
//! 論理ピクセル空間の点列 (x, y, cat) を間引く。x は index 単調を前提とする。

/// 列ごと min/max デシメーション。floor(x) を列キーにバケツ化し、各占有列で
/// start / min / max / end の最大4点を index 順に残す（Chart.js minMaxDecimation 準拠）。
/// 簡略化: Chart.js は min/max を列平均x に置くが、本実装は元 x を保つ（同一列内なのでサブピクセル差）。
pub fn min_max(points: &[(f64, f64, usize)]) -> Vec<(f64, f64, usize)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out: Vec<(f64, f64, usize)> = Vec::new();
    fn push_unique(out: &mut Vec<(f64, f64, usize)>, p: (f64, f64, usize)) {
        if out.last().map(|l| l.2) != Some(p.2) {
            out.push(p);
        }
    }
    fn flush(
        out: &mut Vec<(f64, f64, usize)>,
        start: usize,
        end: usize,
        pts: &[(f64, f64, usize)],
    ) {
        push_unique(out, pts[start]);
        let (mut min_i, mut max_i) = (start, start);
        for k in start..=end {
            if pts[k].1 < pts[min_i].1 {
                min_i = k;
            }
            if pts[k].1 > pts[max_i].1 {
                max_i = k;
            }
        }
        let (i1, i2) = (min_i.min(max_i), min_i.max(max_i));
        push_unique(out, pts[i1]);
        push_unique(out, pts[i2]);
        push_unique(out, pts[end]);
    }
    let mut col_start = 0usize;
    let mut prev_col = points[0].0.floor() as i64;
    for i in 1..points.len() {
        let col = points[i].0.floor() as i64;
        if col != prev_col {
            flush(&mut out, col_start, i - 1, points);
            col_start = i;
            prev_col = col;
        }
    }
    flush(&mut out, col_start, points.len() - 1, points);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_max_reduces_dense_columns_and_preserves_extremes() {
        let pts: Vec<(f64, f64, usize)> = vec![
            (0.0, 5.0, 0),
            (0.2, 1.0, 1),
            (0.4, 9.0, 2),
            (0.6, 3.0, 3),
            (0.8, 7.0, 4),
            (1.0, 2.0, 5),
            (1.2, 8.0, 6),
            (1.4, 0.0, 7),
            (1.6, 6.0, 8),
            (1.8, 4.0, 9),
        ];
        let out = min_max(&pts);
        assert!(out.len() < pts.len());
        assert!(out.iter().any(|p| p.2 == 2));
        assert!(out.iter().any(|p| p.2 == 1));
        assert!(out.iter().any(|p| p.2 == 6));
        assert!(out.iter().any(|p| p.2 == 7));
    }

    #[test]
    fn min_max_x_is_monotonic_nondecreasing() {
        let pts: Vec<(f64, f64, usize)> = (0..50)
            .map(|i| (i as f64 * 0.1, ((i * 7) % 13) as f64, i))
            .collect();
        let out = min_max(&pts);
        for w in out.windows(2) {
            assert!(w[1].0 >= w[0].0, "x must be monotonic non-decreasing");
        }
    }

    #[test]
    fn min_max_passthrough_when_tiny() {
        let pts = vec![(0.0, 1.0, 0), (1.0, 2.0, 1)];
        assert_eq!(min_max(&pts), pts);
    }

    #[test]
    fn min_max_is_deterministic() {
        let pts: Vec<(f64, f64, usize)> = (0..200)
            .map(|i| (i as f64 * 0.05, (i % 17) as f64, i))
            .collect();
        assert_eq!(min_max(&pts), min_max(&pts));
    }
}
