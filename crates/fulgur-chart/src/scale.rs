//! 線形スケールと nice ticks（1-2-5 ステップ）。すべて決定的な純関数。

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
        let t = (v - self.d0) / span;
        self.p0 + t * (self.p1 - self.p0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NiceTicks {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub ticks: Vec<f64>,
}

/// `data_min`〜`data_max` を 1-2-5 系列の「きれいな」目盛りに丸める。
/// `target_count` は目安の目盛り間隔数（ticks数 - 1）。chart.js `maxTicksLimit=11` に合わせる場合は 10 を渡す。
/// 範囲が 0（縮退）でも panic しない。極端な有限値でも panic しない。
pub fn nice_ticks(data_min: f64, data_max: f64, target_count: usize) -> NiceTicks {
    // 1. 0除算回避。目盛り間隔数も上限を設け、極端な有限値での過大確保を防ぐ。
    const MAX_TICK_INTERVALS: usize = 1_000;
    let count = target_count.clamp(1, MAX_TICK_INTERVALS);

    // 2. 縮退（range<=0）: range を 1.0 とみなし data_max を +1.0 して汎用処理に乗せる。
    let (data_min, data_max, range) = if data_max - data_min <= 0.0 {
        (data_min, data_min + 1.0, 1.0)
    } else {
        (data_min, data_max, data_max - data_min)
    };

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
    if !intervals.is_finite() || intervals < 1.0 || intervals > MAX_TICK_INTERVALS as f64 {
        return bounded_ticks(nice_min, nice_max, count);
    }
    let n = intervals as usize;
    let ticks = (0..=n).map(|i| nice_min + i as f64 * step).collect();

    // 8.
    NiceTicks {
        min: nice_min,
        max: nice_max,
        step,
        ticks,
    }
}

/// nice 丸めが使えない場合のフォールバック: データ範囲を等分して目盛りを返す。
fn bounded_ticks(data_min: f64, data_max: f64, count: usize) -> NiceTicks {
    let min = if data_min.is_finite() { data_min } else { 0.0 };
    let mut max = if data_max.is_finite() {
        data_max
    } else {
        min + 1.0
    };
    if max <= min {
        max = min + 1.0;
    }

    let range = max - min;
    let step = if range.is_finite() && range > 0.0 {
        range / count as f64
    } else {
        1.0
    };
    let ticks = (0..=count)
        .map(|i| {
            if i == count {
                max
            } else if range.is_finite() {
                min + step * i as f64
            } else {
                min
            }
        })
        .collect();

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
        assert!(t.min <= 5.0 && t.max >= 5.0);
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
