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

/// 対数スケールの目盛りセット。`major` は 10^n(ラベル表示対象)、`minor` は
/// 各 decade の mantissa 2..9 倍(ラベルなしグリッド用)。両方とも値空間(データ空間)の
/// 実値であり、log10 変換は `ValueScale::Log` が写像時に行う。
///
/// # テストされている構造的不変条件
///
/// 以下は `log_ticks` のテストモジュール(`log_ticks_brackets_domain_for_reasonable_inputs`
/// 等の property スタイルテスト群、および単体テスト)で複数のドメインに対して
/// 固定されている契約。Task 9 が `ticks.min`/`ticks.max` を実写像のクランプ境界として
/// 直接使うため、ここに明示しておく:
/// - **ドメインブラケティング:** 縮退していない「妥当な」ドメインでは
///   `min <= data_min` かつ `max >= data_max`(極端/縮退したドメインでの例外は
///   `log_ticks` 関数のドキュメントコメントを参照)。
/// - `major`・`minor` はともに昇順ソート済み。
/// - `major` の各要素は厳密に 10 の整数乗(`10^n`)。
/// - `min == major[0]`、`max == major[major.len() - 1]`(どちらも 10 の整数乗)。
/// - `minor` は最上位 decade(`major` の最後の要素が表す decade)の mantissa
///   倍数を含まない。含めると必ず `max` を超えてしまうため、意図的に除外している。
/// - `log_ticks` はどんな有限入力の組(順序が `data_min > data_max` でも、
///   負値・0・NaN が混じっていても)に対してもパニックしない。
///
/// # 非目標: chart.js との tick-for-tick / ラベル可視性ルールの parity
///
/// chart.js 実機の `generateTicks()` は decade+mantissa(2..9) よりも複雑な規則
/// (単一 decade ドメインでの細分化、複数 decade ドメインでの非対称な mantissa=1.5
/// tick 挿入)とラベル可視性ルール(`Ticks.formatters.logarithmic`)を持つが、
/// `log_ticks` は意図的にこれらを再現しない。上記の構造的不変条件のみが
/// テスト・保証される範囲であり、それ以上を期待しないこと。
/// 経緯・実測根拠: `docs/plans/2026-08-08-fulgur-chart-smw-logarithmic-scale.md`
/// の「Task 6 実測結果」節(特に末尾の「スコープ決定」小節)、および
/// `bd show fulgur-chart-smw` の acceptance フィールド。
#[derive(Clone, Debug, PartialEq)]
pub struct LogTicks {
    pub min: f64,
    pub max: f64,
    pub major: Vec<f64>,
    pub minor: Vec<f64>,
}

/// nice_ticks の MAX_TICK_INTERVALS と同じ趣旨: 極端なドメイン(例 1..1e300)で
/// decade 数が爆発しないよう上限を設ける。この値は `10f64.powi` が有限を保てる
/// 指数の限界(10^308 は有限、10^309 は inf)とも一致させ、指数クランプの境界にも使う。
const MAX_LOG_DECADES: i32 = 308;

/// `data_min`..`data_max`(共に正の有限値)を 10^n の decade 境界に丸め、
/// 主目盛(10^n)と minor目盛(mantissa 2..9)を生成する。
/// 呼び出し側契約: `data_min > 0.0 && data_max >= data_min && 両方有限`。
///
/// 注意: `min <= data_min` は無条件の保証ではない。ドメインが
/// `MAX_LOG_DECADES`(308)decade 以内に収まり、かつ `data_min >= f64::MIN_POSITIVE`
/// である場合にのみ成り立つ。それ以外の極端/縮退した入力
/// (例: `data_min` が非正規化数(subnormal)に近い極小値、あるいはドメインが
/// 308 decade を超えて広がる場合)では、指数クランプにより `min` が
/// `data_min` を上回ることがある。
pub fn log_ticks(data_min: f64, data_max: f64) -> LogTicks {
    let data_min = if data_min.is_finite() && data_min > 0.0 {
        data_min
    } else {
        f64::MIN_POSITIVE
    };
    // data_min が f64::MAX 付近(例: 1.7e308)だと、この乗算自体が inf に
    // オーバーフローし得る。それでも正しく動くのは暗黙の頑健性による: 後段の
    // data_max.log10().ceil() as i32 という f64→i32 キャストは Rust の cast
    // セマンティクスにより inf を i32::MAX に飽和させ、続く
    // .min(MAX_LOG_DECADES) がそれを有限範囲に収める。
    let data_max = if data_max.is_finite() && data_max >= data_min {
        data_max
    } else {
        data_min * 10.0
    };

    // 指数を [-MAX_LOG_DECADES, MAX_LOG_DECADES] の範囲に収め、10f64.powi が
    // オーバーフローして inf にならないようにする。
    //
    // オーバーフロー防止に実際に効いているのは次の 2 箇所だけ:
    // - lo_exp の上限を MAX_LOG_DECADES - 1 に制限すること。これにより、
    //   直後の hi_exp = max(hi_exp_raw, lo_exp + 1) で lo_exp + 1 が
    //   MAX_LOG_DECADES を超えることがなくなる(例: data_min = f64::MAX の
    //   とき、この上限がなければ lo_exp=308 → hi_exp=309 → 10^309 = inf)。
    // - hi_exp_raw の上限を MAX_LOG_DECADES に制限すること。これがないと、
    //   data_max が f64::MAX 付近のとき hi_exp_raw が 309 になり、
    //   MAX_LOG_DECADES(=308) を超えて 10f64.powi(hi_exp) がオーバーフローし得る。
    //
    // lo_exp の下限(-MAX_LOG_DECADES)はオーバーフロー防止ではなく、
    // 10f64.powi(lo_exp) が 0 にアンダーフローするのを防ぐためのもの
    // (data_min が非正規化数(subnormal)に近い極小値だと、クランプ前の
    // floor(log10(data_min)) はこれよりもさらに負になり得る。例:
    // f64 最小の非正規化数 5e-324 では floor(log10(..)) = -324)。
    //
    // hi_exp_raw には下限クランプを付けていない: lo_exp の下限が
    // -MAX_LOG_DECADES である以上 lo_exp + 1 は常に -MAX_LOG_DECADES + 1
    // 以上になるため、hi_exp = max(hi_exp_raw, lo_exp + 1) は hi_exp_raw
    // 側に下限クランプを足しても足さなくても結果が変わらない(= 何を
    // 設定しても不活性)。追加すると「対称に見える」だけで実質的な意味は
    // ないため、意図的に付けていない。
    let lo_exp = (data_min.log10().floor() as i32).clamp(-MAX_LOG_DECADES, MAX_LOG_DECADES - 1);
    let hi_exp_raw = (data_max.log10().ceil() as i32).min(MAX_LOG_DECADES);
    let hi_exp = hi_exp_raw.max(lo_exp + 1);
    // decade 数が上限を超える場合は、実データの上端を表す hi_exp を保ったまま
    // lo_exp を引き上げて範囲を狭める。逆に hi_exp を下げてしまうと、実際の
    // data_max より小さい max を返すことになり、「min/max はドメインを覆う
    // decade 境界である」という契約を破ってしまう。この結果、極端な入力では
    // min(= 10^lo_exp) が data_min を上回ることがある(関数doc コメント参照)。
    let lo_exp = lo_exp.max(hi_exp - MAX_LOG_DECADES);

    let mut major = Vec::new();
    let mut minor = Vec::new();
    for exp in lo_exp..=hi_exp {
        let decade = 10f64.powi(exp);
        major.push(decade);
        if exp < hi_exp {
            for mantissa in 2..=9 {
                minor.push(mantissa as f64 * decade);
            }
        }
    }

    LogTicks {
        min: 10f64.powi(lo_exp),
        max: 10f64.powi(hi_exp),
        major,
        minor,
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

/// 値→ピクセル写像。線形はそのまま `LinearScale` に委譲し、対数は内部で
/// `log10` 変換してから同じ `LinearScale` に委譲する。呼び出し側は
/// `ValueScale::map(v)` だけを見ればよく、線形/対数の分岐を意識しない。
#[derive(Debug, Clone)]
pub enum ValueScale {
    Linear(LinearScale),
    Log {
        /// ログ空間(log10(d0)..log10(d1))を写す内部スケール。
        inner: LinearScale,
        /// この値以下は floor にクランプしてから log10 する(0 や丸め誤差での
        /// 負値が -inf/NaN を作らないための総関数化)。呼び出し側で「floor 未満は
        /// 描画しない」判断が必要な場合(負値スキップ)は、ここに来る前に
        /// 呼び出し側がフィルタ済みである前提。現時点では Linear 経路のみが
        /// 構築されるため、この分岐は未到達。
        floor: f64,
    },
}

impl ValueScale {
    pub fn map(&self, v: f64) -> f64 {
        match self {
            ValueScale::Linear(s) => s.map(v),
            ValueScale::Log { inner, floor } => inner.map(v.max(*floor).log10()),
        }
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

    #[test]
    fn log_ticks_single_decade() {
        let t = log_ticks(3.0, 7.0);
        assert_eq!(t.min, 1.0);
        assert_eq!(t.max, 10.0);
        assert_eq!(t.major, vec![1.0, 10.0]);
        assert_eq!(t.minor, vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn log_ticks_multi_decade() {
        let t = log_ticks(30.0, 4000.0);
        assert_eq!(t.min, 10.0);
        assert_eq!(t.max, 10000.0);
        assert_eq!(t.major, vec![10.0, 100.0, 1000.0, 10000.0]);
        assert!(t.minor.contains(&20.0));
        assert!(t.minor.contains(&9000.0));
    }

    #[test]
    fn log_ticks_rejects_non_positive_domain_does_not_panic() {
        let t = log_ticks(f64::MIN_POSITIVE, f64::MIN_POSITIVE * 10.0);
        assert!(t.min.is_finite() && t.min > 0.0);
        assert!(t.max.is_finite() && t.max > t.min);
    }

    #[test]
    fn log_ticks_non_positive_min_still_brackets_domain() {
        // data_min<=0 や NaN は呼び出し側契約違反だが、Task 4 でマスクされた負値が
        // ここまで来る現実的な経路がある。data_min を f64::MIN_POSITIVE に
        // フォールバックさせても、実データの上端(data_max)は必ず覆われること。
        for data_min in [0.0, -5.0, f64::NAN] {
            let t = log_ticks(data_min, 100.0);
            assert!(t.min.is_finite() && t.min > 0.0, "{data_min}: {t:?}");
            assert!(t.max.is_finite() && t.max >= 100.0, "{data_min}: {t:?}");
            assert!(t.major.iter().all(|v| v.is_finite()), "{data_min}: {t:?}");
        }
    }

    #[test]
    fn log_ticks_extreme_domain_stays_finite() {
        // data_max が f64 の指数上限付近にあると、ちょうど覆う decade 境界
        // (10^309)が inf になってしまう。この場合は正確なブラケットより
        // 有限性を優先し、min/max/major/minor が全て有限であること。
        let t = log_ticks(1.5e308, 1.7e308);
        assert!(t.min.is_finite() && t.min > 0.0, "{t:?}");
        assert!(t.max.is_finite() && t.max > t.min, "{t:?}");
        assert!(t.major.iter().all(|v| v.is_finite()), "{t:?}");
        assert!(t.minor.iter().all(|v| v.is_finite()), "{t:?}");
    }

    // --- 構造的不変条件の property スタイルテスト(Task 7) --------------------
    //
    // 以下は特定の1-2ケースの厳密値ではなく、「妥当な」(縮退していない)複数の
    // ドメインに対して log_ticks 自身の契約(chart.js の実装詳細には依存しない)
    // を汎用的に検証する。ケース一覧は tools/chartjs_ticks.mjs の log ケース
    // (single/multi decade, sub-one, wide, exact powers)に対応させている。

    /// property テスト共通の「妥当な」ドメイン一覧: 単一 decade・複数 decade・
    /// sub-one(1未満)・広域・ちょうど10の整数乗境界、をそれぞれ代表させる。
    const REASONABLE_LOG_DOMAINS: [(f64, f64); 5] = [
        (3.0, 7.0),         // 単一 decade
        (30.0, 4000.0),     // 複数 decade
        (0.003, 0.7),       // sub-one(1未満)にまたがる
        (1.0, 1_000_000.0), // 広域(6 decade)
        (1.0, 1000.0),      // ちょうど10の整数乗の境界
    ];

    #[test]
    fn log_ticks_brackets_domain_for_reasonable_inputs() {
        for &(data_min, data_max) in &REASONABLE_LOG_DOMAINS {
            let t = log_ticks(data_min, data_max);
            assert!(
                t.min <= data_min,
                "domain {data_min}..{data_max}: min {} > data_min {data_min}: {t:?}",
                t.min
            );
            assert!(
                t.max >= data_max,
                "domain {data_min}..{data_max}: max {} < data_max {data_max}: {t:?}",
                t.max
            );
        }
    }

    #[test]
    fn log_ticks_major_and_minor_are_strictly_ascending() {
        for &(data_min, data_max) in &REASONABLE_LOG_DOMAINS {
            let t = log_ticks(data_min, data_max);
            assert!(
                t.major.windows(2).all(|w| w[0] < w[1]),
                "domain {data_min}..{data_max}: major not ascending: {:?}",
                t.major
            );
            assert!(
                t.minor.windows(2).all(|w| w[0] < w[1]),
                "domain {data_min}..{data_max}: minor not ascending: {:?}",
                t.minor
            );
        }
    }

    #[test]
    fn log_ticks_major_values_are_exact_powers_of_ten() {
        for &(data_min, data_max) in &REASONABLE_LOG_DOMAINS {
            let t = log_ticks(data_min, data_max);
            for &v in &t.major {
                let rounded_exp = v.log10().round();
                assert!(
                    (v.log10() - rounded_exp).abs() < 1e-9,
                    "domain {data_min}..{data_max}: major value {v} is not a power of ten \
                     (log10={}, nearest integer={rounded_exp})",
                    v.log10()
                );
            }
        }
    }

    #[test]
    fn log_ticks_min_max_match_major_endpoints() {
        for &(data_min, data_max) in &REASONABLE_LOG_DOMAINS {
            let t = log_ticks(data_min, data_max);
            assert_eq!(
                t.min,
                *t.major.first().expect("major must be non-empty"),
                "domain {data_min}..{data_max}: {t:?}"
            );
            assert_eq!(
                t.max,
                *t.major.last().expect("major must be non-empty"),
                "domain {data_min}..{data_max}: {t:?}"
            );
        }
    }

    #[test]
    fn log_ticks_minor_excludes_top_decade() {
        for &(data_min, data_max) in &REASONABLE_LOG_DOMAINS {
            let t = log_ticks(data_min, data_max);
            assert!(
                t.minor.iter().all(|&v| v < t.max),
                "domain {data_min}..{data_max}: minor contains a value >= max ({}): {:?}",
                t.max,
                t.minor
            );
        }
    }

    #[test]
    fn log_ticks_never_panics_across_finite_and_pathological_inputs() {
        // 「有限な入力の組」に加えて、実運用で入り込みうる非有限値(NaN/inf)や
        // data_min > data_max の順序違反も総当たりで確認する(既存の単体テストは
        // それぞれ個別のケースをピン留めしているが、ここでは組み合わせを網羅する)。
        let probes = [
            f64::NEG_INFINITY,
            -1e300,
            -1.0,
            0.0,
            f64::MIN_POSITIVE,
            f64::EPSILON,
            1.0,
            100.0,
            1e300,
            f64::MAX,
            f64::INFINITY,
            f64::NAN,
        ];
        for &data_min in &probes {
            for &data_max in &probes {
                // パニックしないことが主目的だが、呼び出しに成功しただけでは
                // 一部の組み合わせ(例: data_min=0.0, data_max=100.0 のような
                // 縮退入力は f64::MIN_POSITIVE へのフォールバック経由で
                // 数百 decade に及ぶ major/minor を構築しうる、既知の
                // 未解決ギャップ: fulgur-chart-8so)を素通りしてしまう。
                // min/max だけでなく major/minor の全要素も有限であることまで
                // 確認し、この重い呼び出しを実際に検証に使う。
                let t = log_ticks(data_min, data_max);
                assert!(
                    t.min.is_finite() && t.max.is_finite(),
                    "data_min={data_min}, data_max={data_max}: min/max not finite: {t:?}"
                );
                assert!(
                    t.major.iter().all(|v| v.is_finite()),
                    "data_min={data_min}, data_max={data_max}: major contains non-finite value: {:?}",
                    t.major
                );
                assert!(
                    t.minor.iter().all(|v| v.is_finite()),
                    "data_min={data_min}, data_max={data_max}: minor contains non-finite value: {:?}",
                    t.minor
                );
            }
        }
    }
}
