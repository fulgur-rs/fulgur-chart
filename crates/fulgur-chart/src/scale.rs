//! 線形スケールと nice ticks（1-2-5 ステップ）。すべて決定的な純関数。

const MAX_TICK_INTERVALS: usize = 1_000;

/// 値→ピクセルの線形写像。px_min>px_max（y軸の上下反転）も許容。
#[derive(Debug, Clone)]
pub struct LinearScale {
    d0: f64,
    d1: f64,
    p0: f64,
    p1: f64,
}

impl LinearScale {
    pub fn new(d0: f64, d1: f64, p0: f64, p1: f64) -> Self {
        Self { d0, d1, p0, p1 }
    }

    pub fn map(&self, v: f64) -> f64 {
        let span = self.d1 - self.d0;
        if span == 0.0 {
            return self.p0;
        }
        let t = if span.is_infinite() && self.d0.is_finite() && self.d1.is_finite() {
            let endpoint_scale = self.d0.abs().max(self.d1.abs());
            let scaled_d0 = self.d0 / endpoint_scale;
            let scaled_d1 = self.d1 / endpoint_scale;
            (v / endpoint_scale - scaled_d0) / (scaled_d1 - scaled_d0)
        } else {
            (v - self.d0) / span
        };
        let pixel_span = self.p1 - self.p0;
        if pixel_span.is_infinite() && self.p0.is_finite() && self.p1.is_finite() {
            self.p0 * (1.0 - t) + self.p1 * t
        } else {
            self.p0 + t * pixel_span
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NiceTicks {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub ticks: Vec<f64>,
}

fn expand_finite_degenerate_domain(value: f64) -> (f64, f64) {
    if !value.is_finite() {
        return (0.0, 1.0);
    }
    let upper = value + 1.0;
    if upper.is_finite() && upper > value {
        return (value, upper);
    }

    // At magnitudes where +1.0 cannot advance, move one representable value
    // toward zero. This keeps f64::MAX endpoints finite and expands inward.
    let inward = f64::from_bits(value.to_bits() - 1);
    if value.is_sign_positive() {
        (inward, value)
    } else {
        (value, inward)
    }
}

/// `data_min`〜`data_max` を 1-2-5 系列の「きれいな」目盛りに丸める。
/// `target_count` は目安の目盛り間隔数（ticks数 - 1）。chart.js `maxTicksLimit=11` に合わせる場合は 10 を渡す。
/// 範囲が 0（縮退）でも panic しない。極端な有限値でも panic しない。
pub fn nice_ticks(data_min: f64, data_max: f64, target_count: usize) -> NiceTicks {
    // 1. 0除算回避。目盛り間隔数も上限を設け、極端な有限値での過大確保を防ぐ。
    let count = target_count.clamp(1, MAX_TICK_INTERVALS);

    // 2. 縮退（range<=0）: range を 1.0 とみなし data_max を +1.0 して汎用処理に乗せる。
    let (data_min, data_max) = if data_max - data_min <= 0.0 {
        expand_finite_degenerate_domain(data_min)
    } else {
        (data_min, data_max)
    };
    let range = data_max - data_min;

    // 3-5. 1-2-5 ステップを選ぶ。
    let raw_step = range / count as f64;
    let magnitude = 10f64.powf(raw_step.log10().floor());
    let norm = raw_step / magnitude; // 1.0〜10.0
    let step = magnitude
        * if norm <= 1.0 {
            1.0
        } else if norm <= 2.0 {
            2.0
        } else if norm <= 5.0 {
            5.0
        } else {
            10.0
        };

    // 6. データ範囲を step グリッドに合わせて外側に丸める。
    let nice_min = (data_min / step).floor() * step;
    let nice_max = (data_max / step).ceil() * step;

    // 極端な有限値では、丸め計算が f64 の上限を超えて inf になる場合がある。
    // その場合は「nice」丸めを諦め、入力範囲を有限なまま等分して返す。
    if !nice_min.is_finite() || !nice_max.is_finite() || !step.is_finite() || step <= 0.0 {
        return bounded_ticks(data_min, data_max, count);
    }

    // 7. 整数 i から目盛りを生成（決定性のため浮動加算ループは使わない）。
    let intervals = ((nice_max - nice_min) / step).round();
    // nice_min と nice_max が両方有限でも、その差が f64::MAX を超えて span = inf に
    // なる場合がある（例: nice_min=-8e307, nice_max=1e308）。nice 境界を bounded_ticks
    // に渡すと LinearScale も同じ inf span を使い、全値が p0 にマップされる。
    // data_min/data_max は入力境界なので span が有限であることが保証されている。
    if !intervals.is_finite() || intervals < 1.0 || intervals > MAX_TICK_INTERVALS as f64 {
        return bounded_ticks(data_min, data_max, count);
    }
    let n = intervals as usize;
    let mut ticks: Vec<f64> = (0..=n).map(|i| nice_min + i as f64 * step).collect();
    ticks.dedup_by(|left, right| *left == *right);
    let step = if ticks.len() < n + 1 && ticks.len() == 2 {
        ticks[1] - ticks[0]
    } else {
        step
    };

    // 8.
    NiceTicks {
        min: nice_min,
        max: nice_max,
        step,
        ticks,
    }
}

/// Vega-Lite のdogfood line chart用に、ゼロ基準と半step余白を持つ目盛りを返す。
pub fn vega_nice_ticks(data_min: f64, data_max: f64, plot_height: f64) -> NiceTicks {
    let target = if plot_height.is_finite() && plot_height > 0.0 {
        (plot_height / 40.0).floor().clamp(2.0, 10.0) as usize
    } else {
        2
    };
    if !data_min.is_finite() || !data_max.is_finite() || data_min > data_max {
        return nice_ticks(data_min, data_max, target);
    }

    let span = data_max - data_min;
    if !span.is_finite() || span <= 0.0 {
        return nice_ticks(data_min, data_max, target);
    }
    let padding = span * 0.05;

    let (min, max, step) = if data_min >= 0.0 {
        let padded_max = data_max + padding;
        let Some(step) = finite_nice_step(padded_max, target) else {
            return nice_ticks(data_min, data_max, target);
        };
        let half_step = step / 2.0;
        let max = (padded_max / half_step).ceil() * half_step;
        if !max.is_finite() {
            return nice_ticks(data_min, data_max, target);
        }
        (0.0, max, step)
    } else if data_max <= 0.0 {
        let padded_min = data_min - padding;
        let Some(step) = finite_nice_step(-padded_min, target) else {
            return nice_ticks(data_min, data_max, target);
        };
        let half_step = step / 2.0;
        let min = (padded_min / half_step).floor() * half_step;
        if !min.is_finite() {
            return nice_ticks(data_min, data_max, target);
        }
        (min, 0.0, step)
    } else {
        let padded_min = data_min - padding;
        let padded_max = data_max + padding;
        let Some(step) = finite_nice_step(padded_max - padded_min, target) else {
            return nice_ticks(data_min, data_max, target);
        };
        let half_step = step / 2.0;
        let min = (padded_min / half_step).floor() * half_step;
        let max = (padded_max / half_step).ceil() * half_step;
        if !min.is_finite() || !max.is_finite() {
            return nice_ticks(data_min, data_max, target);
        }
        (min, max, step)
    };

    let Some(ticks) = full_step_ticks(min, max, step) else {
        return nice_ticks(data_min, data_max, target);
    };
    NiceTicks {
        min,
        max,
        step,
        ticks,
    }
}

fn finite_nice_step(numerator: f64, target: usize) -> Option<f64> {
    let raw_step = numerator / target.max(1) as f64;
    if !raw_step.is_finite() || raw_step <= 0.0 {
        return None;
    }
    let step = nice_step(raw_step);
    (step.is_finite() && step > 0.0).then_some(step)
}

fn nice_step(raw_step: f64) -> f64 {
    let magnitude = 10f64.powf(raw_step.log10().floor());
    let normalized = raw_step / magnitude;
    magnitude
        * if normalized <= 1.0 {
            1.0
        } else if normalized <= 2.0 {
            2.0
        } else if normalized <= 5.0 {
            5.0
        } else {
            10.0
        }
}

fn full_step_ticks(min: f64, max: f64, step: f64) -> Option<Vec<f64>> {
    if !min.is_finite() || !max.is_finite() || !step.is_finite() || step <= 0.0 {
        return None;
    }
    let first = (min / step).ceil() * step;
    if !first.is_finite() || first > max {
        return None;
    }
    let intervals = ((max - first) / step).floor();
    if !intervals.is_finite() || intervals < 0.0 || intervals > MAX_TICK_INTERVALS as f64 {
        return None;
    }
    let count = intervals as usize;
    Some(
        (0..=count)
            .map(|index| first + index as f64 * step)
            .collect(),
    )
}

/// nice 丸めが使えない場合のフォールバック: データ範囲を等分して目盛りを返す。
fn bounded_ticks(data_min: f64, data_max: f64, count: usize) -> NiceTicks {
    let min = if data_min.is_finite() { data_min } else { 0.0 };
    let max = if data_max.is_finite() {
        data_max
    } else {
        min + 1.0
    };
    let (min, max) = if max <= min {
        expand_finite_degenerate_domain(min)
    } else {
        (min, max)
    };

    let range = max - min;
    // range が inf になる場合（例: min=-f64::MAX, max=f64::MAX）は
    // range / count も inf になるため、分配してから減算する形で step を計算する。
    let step = if range.is_finite() && range > 0.0 {
        let divided = range / count as f64;
        if divided > 0.0 { divided } else { range }
    } else {
        max / count as f64 - min / count as f64
    };
    // 同じ理由で tick 生成も lerp を使う: min + range*t は中間でオーバーフローするが
    // min*(1-t) + max*t は各係数が 1 以下なので有限を保てる。
    let mut ticks: Vec<f64> = (0..=count)
        .map(|i| {
            if i == count {
                max
            } else if range.is_finite() {
                min + range * (i as f64 / count as f64)
            } else {
                let t = i as f64 / count as f64;
                min * (1.0 - t) + max * t
            }
        })
        .collect();
    ticks.dedup_by(|left, right| *left == *right);
    let step = if ticks.len() < count + 1 && ticks.len() == 2 {
        ticks[1] - ticks[0]
    } else {
        step
    };

    NiceTicks {
        min,
        max,
        step,
        ticks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_extreme_singleton_domain(ticks: &NiceTicks, value: f64) {
        assert!(ticks.min.is_finite(), "{ticks:?}");
        assert!(ticks.max.is_finite(), "{ticks:?}");
        assert!(ticks.min < ticks.max, "{ticks:?}");
        assert!(ticks.step.is_finite() && ticks.step > 0.0, "{ticks:?}");
        assert!(
            ticks
                .ticks
                .windows(2)
                .all(|pair| pair[0].is_finite() && pair[0] < pair[1]),
            "{ticks:?}"
        );
        assert!(ticks.ticks.last().is_some_and(|tick| tick.is_finite()));
        assert_eq!(ticks.step, ticks.ticks[1] - ticks.ticks[0]);
        if value.is_sign_positive() {
            assert_eq!(ticks.max, value);
        } else {
            assert_eq!(ticks.min, value);
        }

        let scale = LinearScale::new(ticks.min, ticks.max, 0.0, 1.0);
        assert_eq!(scale.map(ticks.min), 0.0);
        assert_eq!(scale.map(ticks.max), 1.0);
    }

    #[test]
    fn nice_ticks_expands_extreme_singletons_inward() {
        for value in [f64::MAX, -f64::MAX] {
            assert_extreme_singleton_domain(&nice_ticks(value, value, 5), value);
        }
    }

    #[test]
    fn vega_nice_ticks_fallback_expands_extreme_singletons_inward() {
        for value in [f64::MAX, -f64::MAX] {
            assert_extreme_singleton_domain(&vega_nice_ticks(value, value, 320.0), value);
        }
    }

    #[test]
    fn bounded_ticks_expands_extreme_singletons_inward() {
        for value in [f64::MAX, -f64::MAX] {
            assert_extreme_singleton_domain(&bounded_ticks(value, value, 5), value);
        }
    }

    #[test]
    fn vega_dogfood_domain_is_zero_to_sixty_five() {
        let ticks = vega_nice_ticks(0.0, 61.0, 320.0);
        assert_eq!((ticks.min, ticks.max, ticks.step), (0.0, 65.0, 10.0));
        assert_eq!(ticks.ticks, vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
    }

    #[test]
    fn vega_nice_ticks_mirrors_negative_and_pads_mixed_domains() {
        let negative = vega_nice_ticks(-61.0, -1.0, 320.0);
        assert_eq!(
            (negative.min, negative.max, negative.step),
            (-65.0, 0.0, 10.0)
        );
        assert_eq!(
            negative.ticks,
            vec![-60.0, -50.0, -40.0, -30.0, -20.0, -10.0, 0.0]
        );

        let mixed = vega_nice_ticks(-10.0, 10.0, 320.0);
        assert!(mixed.min <= -10.0);
        assert!(mixed.max >= 10.0);
        assert!(
            mixed
                .ticks
                .iter()
                .all(|tick| *tick >= mixed.min && *tick <= mixed.max)
        );
    }

    #[test]
    fn vega_nice_ticks_falls_back_for_invalid_input() {
        let ticks = vega_nice_ticks(f64::NAN, 61.0, 320.0);
        assert!(ticks.min.is_finite());
        assert!(ticks.max.is_finite());
        assert!(!ticks.ticks.is_empty());
    }

    #[test]
    fn vega_nice_ticks_falls_back_for_invalid_size_and_extreme_domains() {
        let invalid_size = vega_nice_ticks(0.0, 10.0, f64::NAN);
        assert!(!invalid_size.ticks.is_empty());

        for (min, max) in [
            (-f64::MAX, f64::MAX),
            (0.0, f64::MAX),
            (-f64::MAX, 0.0),
            (-f64::MAX, 1.0),
        ] {
            let ticks = vega_nice_ticks(min, max, 320.0);
            assert!(ticks.min.is_finite(), "{min}..{max}: {ticks:?}");
            assert!(ticks.max.is_finite(), "{min}..{max}: {ticks:?}");
            assert!(ticks.ticks.iter().all(|tick| tick.is_finite()));
        }
    }

    #[test]
    fn vega_nice_ticks_preserves_tiny_finite_domains() {
        for (data_min, data_max) in [(0.0, 1e-20), (-1e-20, 0.0), (-1e-20, 1e-20)] {
            let ticks = vega_nice_ticks(data_min, data_max, 320.0);
            assert!(
                ticks.min.is_finite()
                    && ticks.max.is_finite()
                    && ticks.step.is_finite()
                    && ticks.ticks.iter().all(|tick| tick.is_finite()),
                "{data_min}..{data_max}: {ticks:?}"
            );
            assert!(ticks.step > 0.0, "{data_min}..{data_max}: {ticks:?}");
            assert!(
                ticks.min <= data_min && ticks.max >= data_max,
                "{data_min}..{data_max}: {ticks:?}"
            );
            assert!(
                ticks.max - ticks.min <= 5e-20,
                "{data_min}..{data_max}: {ticks:?}"
            );

            let scale = LinearScale::new(ticks.min, ticks.max, 0.0, 1.0);
            let mapped_min = scale.map(data_min);
            let mapped_max = scale.map(data_max);
            assert!(
                mapped_min.is_finite() && mapped_max.is_finite() && mapped_max - mapped_min > 0.1,
                "{data_min}..{data_max}: {mapped_min}..{mapped_max}"
            );
        }
    }

    #[test]
    fn vega_nice_ticks_falls_back_safely_for_subnormal_domains() {
        let min_subnormal = f64::from_bits(1);
        for (data_min, data_max) in [
            (0.0, min_subnormal),
            (-min_subnormal, 0.0),
            (-min_subnormal, min_subnormal),
            (min_subnormal, f64::from_bits(2)),
        ] {
            let ticks = vega_nice_ticks(data_min, data_max, 320.0);
            assert!(
                ticks.min.is_finite()
                    && ticks.max.is_finite()
                    && ticks.step.is_finite()
                    && ticks.ticks.iter().all(|tick| tick.is_finite()),
                "{data_min}..{data_max}: {ticks:?}"
            );
            assert!(ticks.step > 0.0, "{data_min}..{data_max}: {ticks:?}");
            assert!(
                ticks.min <= data_min && ticks.max >= data_max,
                "{data_min}..{data_max}: {ticks:?}"
            );
            assert!(
                !ticks.ticks.is_empty() && ticks.ticks.len() <= 1_001,
                "{data_min}..{data_max}: {ticks:?}"
            );
        }
    }

    #[test]
    fn vega_nice_ticks_falls_back_when_rounded_span_overflows() {
        let data_min: f64 = -7.5e307;
        let data_max: f64 = 7.5e307;
        assert!((data_max - data_min).is_finite());

        let ticks = vega_nice_ticks(data_min, data_max, 320.0);
        assert!(
            ticks.min.is_finite()
                && ticks.max.is_finite()
                && ticks.step.is_finite()
                && ticks.ticks.iter().all(|tick| tick.is_finite()),
            "{ticks:?}"
        );
        assert!(ticks.step > 0.0, "{ticks:?}");
        assert!(ticks.min <= data_min && ticks.max >= data_max, "{ticks:?}");
        assert!(
            !ticks.ticks.is_empty() && ticks.ticks.len() <= 1_001,
            "{ticks:?}"
        );
    }

    #[test]
    fn vega_nice_ticks_falls_back_when_half_step_rounding_overflows() {
        for (data_min, data_max) in [(0.0, 1.6e308), (-1.6e308, 0.0), (-1.0, 1.46e308)] {
            let ticks = vega_nice_ticks(data_min, data_max, 80.0);
            assert!(ticks.min.is_finite(), "{data_min}..{data_max}: {ticks:?}");
            assert!(ticks.max.is_finite(), "{data_min}..{data_max}: {ticks:?}");
            assert!(ticks.step.is_finite(), "{data_min}..{data_max}: {ticks:?}");
            assert!(
                ticks.ticks.iter().all(|tick| tick.is_finite()),
                "{data_min}..{data_max}: {ticks:?}"
            );
            assert!(ticks.min <= data_min, "{data_min}..{data_max}: {ticks:?}");
            assert!(ticks.max >= data_max, "{data_min}..{data_max}: {ticks:?}");
        }
    }

    #[test]
    fn vega_step_selection_and_empty_tick_ranges_are_bounded() {
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(nice_step(2.0), 2.0);
        assert_eq!(nice_step(5.0), 5.0);
        assert_eq!(nice_step(6.0), 10.0);
        assert!(full_step_ticks(f64::NAN, 1.0, 1.0).is_none());
        assert!(full_step_ticks(1.0, 0.0, 1.0).is_none());
        assert!(full_step_ticks(f64::MAX, f64::MAX, f64::EPSILON).is_none());
        assert!(full_step_ticks(0.0, 1_001.0, 1.0).is_none());
    }

    #[test]
    fn nice_ticks_round_numbers() {
        let t = nice_ticks(0.0, 200.0, 5);
        assert_eq!(t.ticks, vec![0.0, 50.0, 100.0, 150.0, 200.0]);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, 200.0);
        assert_eq!(t.step, 50.0);
    }

    #[test]
    fn nice_ticks_non_round_range() {
        let t = nice_ticks(0.0, 173.0, 5);
        assert_eq!(t.step, 50.0);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, 200.0);
        assert_eq!(t.ticks, vec![0.0, 50.0, 100.0, 150.0, 200.0]);
    }

    #[test]
    fn nice_ticks_handles_negative_min() {
        let t = nice_ticks(-30.0, 70.0, 5);
        assert_eq!(t.step, 20.0);
        assert_eq!(t.min, -40.0);
        assert_eq!(t.max, 80.0);
        assert_eq!(t.ticks, vec![-40.0, -20.0, 0.0, 20.0, 40.0, 60.0, 80.0]);
    }

    #[test]
    fn nice_ticks_flat_range_does_not_panic() {
        let t = nice_ticks(5.0, 5.0, 5);
        assert!(t.step > 0.0);
        assert!(!t.ticks.is_empty());
        assert_eq!((t.min, t.max), (5.0, 6.0));
    }

    #[test]
    fn nice_ticks_reversed_nonfinite_domain_uses_finite_default() {
        let ticks = nice_ticks(f64::INFINITY, 1.0, 5);
        assert_eq!((ticks.min, ticks.max), (0.0, 1.0));
        assert_eq!(
            ticks.ticks,
            vec![0.0, 0.2, 0.4, 0.6000000000000001, 0.8, 1.0]
        );
    }

    #[test]
    fn nice_ticks_extreme_finite_range_is_bounded() {
        let t = nice_ticks(0.0, f64::MAX, 5);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, f64::MAX);
        assert!(t.step.is_finite());
        assert_eq!(t.ticks.len(), 6);
        assert!(t.ticks.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn nice_ticks_caps_requested_tick_count() {
        let t = nice_ticks(0.0, 10.0, usize::MAX);
        assert!(t.ticks.len() <= 1_001);
    }

    #[test]
    fn nice_ticks_near_f64_max_span_has_finite_domain() {
        // nice 丸めが境界を拡張して span = inf になるケース。
        // nice_min=-8e307, nice_max=1e308 → 差が f64::MAX を超えて inf になる。
        // bounded_ticks に nice 境界を渡すと LinearScale が壊れるため、
        // data 境界にフォールバックして min/max が有限の span に収まること。
        let t = nice_ticks(-8e307, 9e307, 10);
        assert!(t.min.is_finite());
        assert!(t.max.is_finite());
        let span = t.max - t.min;
        assert!(span.is_finite(), "span={span}");
        assert!(t.step.is_finite());
        assert!(t.ticks.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn nice_ticks_full_f64_range_is_bounded() {
        // min=-f64::MAX, max=f64::MAX: range がオーバーフローして inf になる場合でも
        // 全 tick が有限値で等分されること。
        let t = nice_ticks(-f64::MAX, f64::MAX, 5);
        assert_eq!(t.min, -f64::MAX);
        assert_eq!(t.max, f64::MAX);
        assert!(t.step.is_finite());
        assert_eq!(t.ticks.len(), 6);
        assert!(t.ticks.iter().all(|v| v.is_finite()));
        // 中間 tick が全て -f64::MAX ではなく単調増加していること。
        assert!(t.ticks.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn linear_scale_maps_endpoints_and_midpoint() {
        let s = LinearScale::new(0.0, 200.0, 0.0, 400.0);
        assert!((s.map(0.0) - 0.0).abs() < 1e-9);
        assert!((s.map(100.0) - 200.0).abs() < 1e-9);
        assert!((s.map(200.0) - 400.0).abs() < 1e-9);
    }

    #[test]
    fn linear_scale_inverted_pixel_range() {
        let s = LinearScale::new(0.0, 100.0, 300.0, 0.0);
        assert!((s.map(0.0) - 300.0).abs() < 1e-9);
        assert!((s.map(100.0) - 0.0).abs() < 1e-9);
        assert!((s.map(50.0) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn linear_scale_maps_overflowing_domain_to_normal_pixel_range() {
        let s = LinearScale::new(-1e308, 1e308, 0.0, 400.0);

        for (value, expected) in [(-1e308, 0.0), (0.0, 200.0), (1e308, 400.0)] {
            let mapped = s.map(value);
            assert!(mapped.is_finite(), "{value} mapped to {mapped}");
            assert!((mapped - expected).abs() < 1e-9, "{value}: {mapped}");
        }
    }

    #[test]
    fn linear_scale_maps_overflowing_domain_to_inverted_pixel_range() {
        let s = LinearScale::new(-1e308, 1e308, 300.0, 0.0);

        for (value, expected) in [(-1e308, 300.0), (0.0, 150.0), (1e308, 0.0)] {
            let mapped = s.map(value);
            assert!(mapped.is_finite(), "{value} mapped to {mapped}");
            assert!((mapped - expected).abs() < 1e-9, "{value}: {mapped}");
        }
    }

    #[test]
    fn linear_scale_maps_overflowing_domain_and_pixel_range() {
        for (p0, p1) in [(-1e308, 1e308), (1e308, -1e308)] {
            let s = LinearScale::new(-f64::MAX, f64::MAX, p0, p1);

            for (value, expected) in [(-f64::MAX, p0), (0.0, 0.0), (f64::MAX, p1)] {
                let mapped = s.map(value);
                assert!(mapped.is_finite(), "{value} mapped to {mapped}");
                assert_eq!(mapped, expected, "{value}: {mapped}");
            }
        }
    }

    #[test]
    fn linear_scale_zero_domain_does_not_panic() {
        let s = LinearScale::new(5.0, 5.0, 0.0, 400.0);
        assert!(s.map(5.0).is_finite());
    }

    // chart.js v4（maxTicksLimit=11、10インターバル）の実出力に対するピンテスト。
    // 期待値は tools/chartjs_ticks.mjs の実行結果で確定。
    // 再生成: cd tools && node chartjs_ticks.mjs > chartjs_ticks_output.json

    #[test]
    fn chartjs_compat_0_to_100() {
        // chart.js: [0,100] → step=10, min=0, max=100, 11本
        let t = nice_ticks(0.0, 100.0, 10);
        assert_eq!(t.step, 10.0);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, 100.0);
        assert_eq!(t.ticks.len(), 11);
        assert_eq!(t.ticks[0], 0.0);
        assert_eq!(t.ticks[10], 100.0);
    }

    #[test]
    fn chartjs_compat_0_to_173() {
        // chart.js: [0,173] → step=20, min=0, max=180, 10本
        let t = nice_ticks(0.0, 173.0, 10);
        assert_eq!(t.step, 20.0);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, 180.0);
        assert_eq!(t.ticks.len(), 10);
    }

    #[test]
    fn chartjs_compat_neg30_to_70() {
        // chart.js: [-30,70] → step=10, min=-30, max=70, 11本
        let t = nice_ticks(-30.0, 70.0, 10);
        assert_eq!(t.step, 10.0);
        assert_eq!(t.min, -30.0);
        assert_eq!(t.max, 70.0);
        assert_eq!(t.ticks.len(), 11);
    }

    #[test]
    fn chartjs_compat_0_to_1() {
        // chart.js: [0,1] → step=0.1, min=0, max=1, 11本
        // step は浮動小数点誤差を許容して比較する
        let t = nice_ticks(0.0, 1.0, 10);
        assert!((t.step - 0.1).abs() < 1e-9, "step={}", t.step);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, 1.0);
        assert_eq!(t.ticks.len(), 11);
    }

    #[test]
    fn chartjs_compat_100_to_10000() {
        // chart.js: [100,10000] → step=1000, min=0, max=10000, 11本
        // nice_min = floor(100/1000)*1000 = 0 (データ範囲外に拡張)
        let t = nice_ticks(100.0, 10000.0, 10);
        assert_eq!(t.step, 1000.0);
        assert_eq!(t.min, 0.0);
        assert_eq!(t.max, 10000.0);
        assert_eq!(t.ticks.len(), 11);
    }
}
