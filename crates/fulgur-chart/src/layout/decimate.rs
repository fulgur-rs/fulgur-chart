//! line/area 用デシメーション（Chart.js options.plugins.decimation 互換）。
//! 論理ピクセル空間の点列 (x, y, cat) を間引く。x は index 単調を前提とする。

use crate::ir::{Decimation, DecimationAlgorithm};

/// threshold 既定 = 論理プロット幅px × この係数（Chart.js 準拠）。
const DECIMATION_THRESHOLD_FACTOR: f64 = 4.0;

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

/// LTTB (Largest Triangle Three Buckets)。視覚形状を保ちつつ samples 点へ間引く。
/// 三角形面積は論理ピクセル空間で計算するため視覚的に正しい。count <= samples なら原データ返却。
// バケツ境界の index 計算が本質のため、index ベースのループを意図的に用いる。
#[allow(clippy::needless_range_loop)]
pub fn lttb(points: &[(f64, f64, usize)], samples: usize) -> Vec<(f64, f64, usize)> {
    let n = points.len();
    if samples < 3 || n <= samples {
        return points.to_vec();
    }
    let mut out: Vec<(f64, f64, usize)> = Vec::with_capacity(samples);
    let bucket_width = (n - 2) as f64 / (samples - 2) as f64;
    out.push(points[0]);
    let mut a = 0usize; // 直前に採用した点の index
    for i in 0..(samples - 2) {
        // 次バケツの平均点（三角形の第3点）
        let mut avg_start = ((i + 1) as f64 * bucket_width).floor() as usize + 1;
        let mut avg_end = ((i + 2) as f64 * bucket_width).floor() as usize + 1;
        avg_start = avg_start.min(n - 1);
        avg_end = avg_end.min(n);
        if avg_end <= avg_start {
            avg_end = avg_start + 1;
        }
        let mut avg_x = 0.0;
        let mut avg_y = 0.0;
        for j in avg_start..avg_end {
            avg_x += points[j].0;
            avg_y += points[j].1;
        }
        let cnt = (avg_end - avg_start) as f64;
        avg_x /= cnt;
        avg_y /= cnt;
        // 候補バケツ
        let range_start = (i as f64 * bucket_width).floor() as usize + 1;
        let range_end = ((i + 1) as f64 * bucket_width).floor() as usize + 1;
        let (ax, ay) = (points[a].0, points[a].1);
        let mut max_area = -1.0_f64;
        let mut next_a = range_start.min(n - 1);
        for j in range_start..range_end.min(n) {
            let area =
                ((ax - avg_x) * (points[j].1 - ay) - (ax - points[j].0) * (avg_y - ay)).abs() * 0.5;
            if area > max_area {
                max_area = area;
                next_a = j;
            }
        }
        out.push(points[next_a]);
        a = next_a;
    }
    out.push(points[n - 1]);
    out
}

/// 間引きを発動すべきか判定。発動するなら (algorithm, samples) を返す。
/// enabled=false / threshold 未満なら None。判定は系列全体の点数で（Chart.js セマンティクス）。
pub fn resolve(
    cfg: &Decimation,
    plot_width: f64,
    total_points: usize,
) -> Option<(DecimationAlgorithm, usize)> {
    if !cfg.enabled {
        return None;
    }
    let threshold = cfg
        .threshold
        .unwrap_or(plot_width.max(1.0) * DECIMATION_THRESHOLD_FACTOR);
    if (total_points as f64) <= threshold {
        return None;
    }
    let samples = cfg.samples.unwrap_or(plot_width.max(1.0)).max(3.0) as usize;
    Some((cfg.algorithm, samples))
}

/// 単一セグメント（gap を含まない連続点列）を間引く。gap 分割は呼び出し側の責務。
pub fn decimate_one(
    seg: &[(f64, f64, usize)],
    algo: DecimationAlgorithm,
    samples: usize,
) -> Vec<(f64, f64, usize)> {
    match algo {
        DecimationAlgorithm::MinMax => min_max(seg),
        DecimationAlgorithm::Lttb => lttb(seg, samples),
    }
}

/// gap 分割済みの全セグメントを間引く。`samples` はセグメント長で按分する
/// （`budget_i = max(3, samples × len_i / total)`）。これにより LTTB のマルチ
/// セグメント予算超過（合計 samples×セグメント数）を防ぎ、出力を
/// `samples + 3×セグメント数` 以下に上限化する。整数演算のみ = 決定的。
/// min-max は samples を無視するため按分は実質 LTTB のみに効く（呼び出しは一様）。
/// FLOOR=3 は LTTB の `samples≥3` ガードを満たすので decimate_one 側は無変更。
// `if total==0 {…} else {…/total}` は空セグメント時の意味的フォールバック（samples 復帰 +
// .max(3)）であり checked_div とは非等価。決定的意図で明示保持するため manual_checked_ops を抑止。
#[allow(clippy::manual_checked_ops)]
pub fn decimate_segments(
    segments: &[Vec<(f64, f64, usize)>],
    algo: DecimationAlgorithm,
    samples: usize,
) -> Vec<Vec<(f64, f64, usize)>> {
    let total: usize = segments.iter().map(|s| s.len()).sum();
    segments
        .iter()
        .map(|s| {
            let budget = if total == 0 {
                samples
            } else {
                (samples * s.len() / total).max(3)
            };
            decimate_one(s, algo, budget)
        })
        .collect()
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

    #[test]
    fn lttb_hits_target_sample_count() {
        let pts: Vec<(f64, f64, usize)> = (0..1000)
            .map(|i| (i as f64, ((i * 31) % 97) as f64, i))
            .collect();
        let out = lttb(&pts, 100);
        assert_eq!(out.len(), 100);
    }

    #[test]
    fn lttb_keeps_first_and_last() {
        let pts: Vec<(f64, f64, usize)> =
            (0..500).map(|i| (i as f64, (i % 11) as f64, i)).collect();
        let out = lttb(&pts, 50);
        assert_eq!(out.first().unwrap().2, 0);
        assert_eq!(out.last().unwrap().2, 499);
    }

    #[test]
    fn lttb_passthrough_when_count_le_samples() {
        let pts: Vec<(f64, f64, usize)> = (0..30).map(|i| (i as f64, 1.0, i)).collect();
        assert_eq!(lttb(&pts, 50), pts);
    }

    #[test]
    fn lttb_is_deterministic() {
        let pts: Vec<(f64, f64, usize)> = (0..800)
            .map(|i| (i as f64, ((i * 13) % 29) as f64, i))
            .collect();
        assert_eq!(lttb(&pts, 80), lttb(&pts, 80));
    }

    #[test]
    fn resolve_none_below_threshold() {
        use crate::ir::Decimation;
        let cfg = Decimation::default();
        assert!(resolve(&cfg, 100.0, 50).is_none());
    }

    #[test]
    fn resolve_some_above_threshold() {
        use crate::ir::{Decimation, DecimationAlgorithm};
        let cfg = Decimation::default();
        // 既定: algorithm=MinMax, samples=floor(plot_width)=100, threshold=plot_width*4=400。
        assert_eq!(
            resolve(&cfg, 100.0, 1000),
            Some((DecimationAlgorithm::MinMax, 100))
        );
    }

    #[test]
    fn resolve_uses_explicit_samples() {
        use crate::ir::Decimation;
        let cfg = Decimation {
            samples: Some(50.0),
            ..Decimation::default()
        };
        let (_, samples) = resolve(&cfg, 100.0, 1000).unwrap();
        assert_eq!(samples, 50);
    }

    #[test]
    fn resolve_floors_samples_to_three() {
        use crate::ir::Decimation;
        let cfg = Decimation {
            samples: Some(1.0),
            ..Decimation::default()
        };
        // .max(3.0) で最低3に切り上げられる。
        let (_, samples) = resolve(&cfg, 100.0, 1000).unwrap();
        assert_eq!(samples, 3);
    }

    #[test]
    fn resolve_threshold_boundary_is_exclusive() {
        use crate::ir::Decimation;
        let cfg = Decimation {
            threshold: Some(1000.0),
            ..Decimation::default()
        };
        // total == threshold は発動しない（<= 判定）。total > threshold で発動。
        assert!(resolve(&cfg, 100.0, 1000).is_none());
        assert!(resolve(&cfg, 100.0, 1001).is_some());
    }

    #[test]
    fn resolve_returns_configured_algorithm() {
        use crate::ir::{Decimation, DecimationAlgorithm};
        let cfg = Decimation {
            algorithm: DecimationAlgorithm::Lttb,
            ..Decimation::default()
        };
        let (algo, _) = resolve(&cfg, 100.0, 1000).unwrap();
        assert_eq!(algo, DecimationAlgorithm::Lttb);
    }

    #[test]
    fn resolve_none_when_disabled() {
        use crate::ir::Decimation;
        let cfg = Decimation {
            enabled: false,
            ..Decimation::default()
        };
        assert!(resolve(&cfg, 100.0, 1000).is_none());
    }

    #[test]
    fn decimate_one_dispatches_min_max() {
        use crate::ir::DecimationAlgorithm;
        let pts: Vec<(f64, f64, usize)> = (0..1000)
            .map(|i| (i as f64 * 0.1, (i % 7) as f64, i))
            .collect();
        let out = decimate_one(&pts, DecimationAlgorithm::MinMax, 100);
        assert!(out.len() < pts.len());
    }

    #[test]
    fn decimate_one_dispatches_lttb() {
        use crate::ir::DecimationAlgorithm;
        let pts: Vec<(f64, f64, usize)> = (0..1000)
            .map(|i| (i as f64, ((i * 31) % 97) as f64, i))
            .collect();
        let out = decimate_one(&pts, DecimationAlgorithm::Lttb, 100);
        assert_eq!(out.len(), 100);
    }

    // ヘルパ: k セグメント × 各 seg_len 点の連続系列群を作る（cat は連番、gap 相当に非連続）。
    fn make_segments(k: usize, seg_len: usize) -> Vec<Vec<(f64, f64, usize)>> {
        (0..k)
            .map(|s| {
                (0..seg_len)
                    .map(|i| {
                        let idx = s * (seg_len + 1) + i; // +1 で cat に隙間（gap）を作る
                        (idx as f64, ((idx * 31) % 97) as f64, idx)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn decimate_segments_lttb_bounds_few_large() {
        // 少数大セグメント: 5×5000, samples=100。素朴実装なら 5×100=500 点だが、
        // 按分すれば合計 ≈ samples に収まる。
        let segs = make_segments(5, 5000);
        let out = decimate_segments(&segs, DecimationAlgorithm::Lttb, 100);
        let total: usize = out.iter().map(|s| s.len()).sum();
        // 証明済み上限: samples + 3×num_segments。
        assert!(
            total <= 100 + 3 * 5,
            "total {total} must be <= samples + 3*num_segments"
        );
        // 素朴実装（各セグメントに full samples）なら 500。按分でそれを大幅に下回る。
        assert!(
            total < 100 * 5,
            "proration must beat naive per-segment budget"
        );
    }

    #[test]
    fn decimate_segments_lttb_many_small_no_passthrough() {
        // 多数小セグメント: 100×10, samples=100。素朴実装は各 n(=10)≤samples(=100) で
        // passthrough → 1000 点（削減ゼロ）。按分後は budget=max(3,1)=3 で LTTB 発動。
        let segs = make_segments(100, 10);
        let out = decimate_segments(&segs, DecimationAlgorithm::Lttb, 100);
        let total: usize = out.iter().map(|s| s.len()).sum();
        assert!(
            total <= 100 + 3 * 100,
            "total {total} must be <= samples + 3*num_segments"
        );
        // 元は 1000 点。passthrough せず明確に削減されること。
        assert!(
            total < 100 * 10,
            "many-small must not pass through unreduced"
        );
    }

    #[test]
    fn decimate_segments_min_max_ignores_budget() {
        // min-max は samples を無視するので、按分の有無で結果は変わらず、
        // 各セグメントを個別に min_max した結果と一致する。
        let segs = make_segments(4, 800);
        let out = decimate_segments(&segs, DecimationAlgorithm::MinMax, 100);
        let expected: Vec<Vec<(f64, f64, usize)>> = segs.iter().map(|s| min_max(s)).collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn decimate_segments_is_deterministic() {
        let segs = make_segments(6, 700);
        assert_eq!(
            decimate_segments(&segs, DecimationAlgorithm::Lttb, 80),
            decimate_segments(&segs, DecimationAlgorithm::Lttb, 80)
        );
    }

    #[test]
    fn decimate_segments_single_segment_matches_decimate_one() {
        // 単一セグメントでは total==len なので budget==samples。既存単体 golden が
        // バイト不変であることの根拠（decimate_one を素通しするのと一致）。
        let segs = make_segments(1, 5000);
        let out = decimate_segments(&segs, DecimationAlgorithm::Lttb, 100);
        let expected = vec![decimate_one(&segs[0], DecimationAlgorithm::Lttb, 100)];
        assert_eq!(out, expected);
    }
}
