//! bar/line が共有するプロット領域・軸・グリッド・凡例の構築。

use crate::ir::{
    AxisSpec, AxisTitleAlign, ChartKind, ChartSpec, Color, DatasetPointStyle, LegendAlign,
    LegendOptions, LegendPointStyle, LegendPos, RadialAxis, ScaleKind, SizeMode, XPositions,
};
use crate::num::fmt_num;
use crate::scale::{LinearScale, NiceTicks, ValueScale, vega_nice_ticks};
use crate::scene::{Anchor, Prim, StyledText};
use crate::temporal::{TemporalTick, temporal_ticks};
use crate::text::TextMeasurer;

/// 動径軸 (`options.scales.r`) のドメイン `[lo, hi]` を解決する。radar / polarArea 共通。
///
/// 意味論は chart.js の `LinearScaleBase.handleTickRangeOptions` に合わせている。要点は
/// **調整はすべて「自動計算側」にのみ作用し、`min` / `max` で明示された hard bound は
/// 決して動かさない** こと (chart.js の `setMin` / `setMax` が `minDefined` /
/// `maxDefined` のとき no-op になるのと同じ)。
///
/// - `min` / `max`: hard bound。指定された側はそのまま使う。
/// - `suggestedMin` / `suggestedMax`: 自動側を広げる方向にのみ効く。
/// - `beginAtZero`: 自動側の端を 0 へ寄せる。下端だけでなく **上端にも** 効くので、
///   全データが負でもドメインは 0 を含む (chart.js は min/max が同符号のとき
///   反対側を 0 に寄せる)。
/// - 縮退 (`hi <= lo`) / 非有限: やはり自動側を開いて解消する。
///
/// `data_min` / `data_max` は呼び出し側が全 finite 値から求めた実データ範囲。
/// データが無い場合は `INFINITY` / `NEG_INFINITY` を渡してよい。
pub(crate) fn resolve_radial_domain(ra: &RadialAxis, data_min: f64, data_max: f64) -> (f64, f64) {
    let lo_is_hard = ra.min.is_some();
    let hi_is_hard = ra.max.is_some();
    // 有限データが無い場合 (dataset が空など) は、片側だけの `suggested*` を
    // 暫定レンジとして使う。0 で埋めてしまうと `suggestedMin: 100` のような
    // 「0 の反対側にある片側指定」が expand-only 判定で捨てられ、100 付近ではなく
    // 0 付近のドメインになってしまう。
    let mut lo = ra.min.unwrap_or(if data_min.is_finite() {
        data_min
    } else {
        ra.suggested_min.or(ra.suggested_max).unwrap_or(0.0)
    });
    let mut hi = ra.max.unwrap_or(if data_max.is_finite() {
        data_max
    } else {
        ra.suggested_max
            .or(ra.suggested_min)
            .unwrap_or(f64::NEG_INFINITY)
    });

    if !lo_is_hard {
        if let Some(s) = ra.suggested_min
            && s < lo
        {
            lo = s;
        }
        if ra.begin_at_zero {
            lo = lo.min(0.0);
        }
    }
    if !hi_is_hard {
        if let Some(s) = ra.suggested_max
            && s > hi
        {
            hi = s;
        }
        if ra.begin_at_zero {
            hi = hi.max(0.0);
        }
    }

    if !hi.is_finite() || hi <= lo {
        // hard bound と自動側が逆転している場合、まず自動側を hard 側へ寄せる。
        // chart.js も `getMinMax` で自動側を hard bound へ引き上げてから
        // `handleTickRangeOptions` で 5% 広げる、という順序になっている。
        //
        // 先に寄せておかないと、既に無効になった自動側の値を base にしてしまい、
        // 5% 広げてもまだ hard bound の反対側に留まる。例えば `min: 100` (hard) で
        // データ最大が 50 のとき base=50 → 52.5 となり、最終救済で 1ULP 幅の
        // ドメインになってしまう (本来は [100, 105])。
        if lo_is_hard && !hi_is_hard && (!hi.is_finite() || hi < lo) {
            hi = lo;
        }
        if hi_is_hard && !lo_is_hard && (!lo.is_finite() || lo > hi) {
            lo = hi;
        }

        // chart.js は `min === max` のとき `offset = max == 0 ? 1 : |max * 0.05|` だけ
        // 開く。ここでも同じ比率を使い、定数データが radius 0 に潰れて不可視になるのを防ぐ。
        let base = if hi.is_finite() { hi } else { lo };
        let base = if base.is_finite() { base } else { 0.0 };
        let offset = if base == 0.0 {
            1.0
        } else {
            (base * 0.05).abs()
        };
        if !hi_is_hard {
            // f64::MAX 近傍では `base + offset` がオーバーフローする。上へ広げられない
            // ので base に留め、下側を広げて幅を作る (下の分岐が担当する)。
            let up = base + offset;
            hi = if up.is_finite() { up } else { base };
        }
        // 下端を下げるのは自動側のときだけ。`beginAtZero` が 0 起点を要求している間は
        // 下げないが、上端が hard で固定されている場合、あるいは上へ広げられずまだ
        // 幅が無い場合は、下げる以外に開く余地が無い。
        if !lo_is_hard && (!ra.begin_at_zero || hi_is_hard || hi <= lo) {
            let down = base - offset;
            if down.is_finite() {
                lo = down;
            }
        }
        // 両側 hard で `min > max` のように矛盾している場合は動かせる自動側が無い。
        // 描画が壊れないよう、決定的に hard な `min` を優先して上端を開く。
        //
        // `lo + 1.0` は絶対値が大きいと丸めで lo に戻ってしまう (f64::MAX 近傍の ulp は
        // 1 よりはるかに大きい)。ulp 相当の相対量で確実に differ させ、それも
        // オーバーフローする場合は下端を下げる。
        // (この時点で lo / hi は共に有限: base と offset が有限で、
        //  非有限になる代入は上でガードしてある)
        if hi <= lo {
            let step = lo.abs().max(1.0) * f64::EPSILON * 4.0;
            let up = lo + step;
            if up.is_finite() && up > lo {
                hi = up;
            } else {
                let down = hi - hi.abs().max(1.0) * f64::EPSILON * 4.0;
                if down.is_finite() && down < hi {
                    lo = down;
                }
            }
        }
    }

    (lo, hi)
}

/// 値 `v` を動径ドメイン `[lo, hi]` 上の 0..1 比率へ写す。
///
/// 通常は `(v - lo) / (hi - lo)`。ただし境界が個々には有限でも、その差が f64 の
/// 表現範囲を超えると span が `+inf` になり比率が壊れる (例 `min: -1e308, max: 1e308`)。
/// その場合は両辺を半分にしてから引く —— 数学的には同値だがオーバーフローしない。
///
/// 端点をクランプする方式は採らない。それだと `resolve_radial_domain` が保証している
/// 「hard bound は決して動かさない」という不変条件を破り、`min: -1e308, max: 1e308` +
/// データ `5e307` が本来の 75% ではなく外周に張り付いてしまう。
pub(crate) fn radial_ratio(v: f64, lo: f64, hi: f64) -> f64 {
    let span = hi - lo;
    if span.is_finite() {
        if span > 0.0 {
            ((v - lo) / span).clamp(0.0, 1.0)
        } else {
            0.0
        }
    } else {
        let half_span = hi * 0.5 - lo * 0.5;
        if half_span > 0.0 {
            ((v * 0.5 - lo * 0.5) / half_span).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// 動径ドメインが正の幅を持つか。`hi - lo` が `+inf` になるケースでも正しく判定する。
pub(crate) fn radial_domain_has_width(lo: f64, hi: f64) -> bool {
    let span = hi - lo;
    if span.is_finite() {
        span > 0.0
    } else {
        hi * 0.5 - lo * 0.5 > 0.0
    }
}

pub const OUTER_PAD: f64 = 8.0;
pub const TITLE_FONT: f64 = 16.0;
pub const LABEL_FONT: f64 = 12.0;
pub const TITLE_BAND: f64 = 28.0;
pub const LEGEND_BAND: f64 = 26.0;
/// 縦置き凡例(Left/Right)の 1 行の高さ(px)。
pub const LEGEND_ROW_H: f64 = 18.0;
pub const X_LABEL_BAND: f64 = 22.0;
/// X 軸タイトル帯の高さ(px)。ラベル帯の下側にさらに確保し、`plot_bottom` を上へ押し上げる。
pub const AXIS_TITLE_BAND: f64 = 20.0;
pub const TEXT_BASELINE_RATIO: f64 = 0.35;
pub const X_LABEL_CENTER_RATIO: f64 = 0.7;
/// データラベルの軸方向ギャップ(棒の端からラベルまでの余白, px)。
pub const LABEL_GAP: f64 = 4.0;
pub const GRID: Color = Color {
    r: 224,
    g: 224,
    b: 224,
    a: 1.0,
};
pub const INK: Color = Color {
    r: 102,
    g: 102,
    b: 102,
    a: 1.0,
};

/// プロット領域と y スケール・目盛り。
pub struct Frame {
    pub scene_width: f64,
    pub scene_height: f64,
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
    pub ticks: NiceTicks,
    pub ys: ValueScale,
    /// 対数軸のラベルなし minor 目盛(mantissa 2..9)。線形軸では常に空。
    pub minor_ticks: Vec<f64>,
    pub temporal_ticks: Vec<TemporalTick>,
}

/// 凡例の有無を判定する。
///
/// Top/Bottom/Left/Right で、supported temporal title-only legend または
/// 1 つ以上の名前付き系列がある場合に有効化する。
fn has_legend(spec: &ChartSpec) -> bool {
    matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && (legend_title(spec).is_some() || spec.series.iter().any(|series| !series.name.is_empty()))
}

/// 凡例タイトル。Chart.js の明示タイトルと Vega temporal line の既存タイトルを扱う。
pub(crate) fn legend_title(spec: &ChartSpec) -> Option<&str> {
    if spec.legend_options.title_display {
        spec.legend_title.as_deref()
    } else {
        temporal_plot_right_legend_title(spec)
    }
}

/// `legend_title` を描画・予約する supported semantics。
///
/// Vega-Lite temporal line が生成する PlotArea + Right legend だけを対象にし、
/// Canvas/category の既存 scene は `legend_title` の有無にかかわらず維持する。
pub(crate) fn temporal_plot_right_legend_title(spec: &ChartSpec) -> Option<&str> {
    if matches!(spec.x_positions, XPositions::Temporal { .. })
        && matches!(spec.size_mode, SizeMode::PlotArea)
        && spec.legend == LegendPos::Right
    {
        spec.legend_title.as_deref()
    } else {
        None
    }
}

/// 値ドメイン(線形軸: begin_at_zero尊重・空データ→0..1・縮退補正)を算出する。
/// 縦棒(compute)と横棒(build_horizontal)が同一の値域計算を共有する。
///
/// 対数軸(`scale_kind == Logarithmic`)は `log_value_domain` へ早期分岐する。
/// この分岐では上記2性質はどちらも成立しない: begin_at_zero は非適用、
/// 空データ/正データなしの場合は `0..1` ではなく `1..10` を返す。詳細は
/// `log_value_domain` のドキュメントを参照。
pub fn value_domain(spec: &ChartSpec, axis: &AxisSpec) -> (f64, f64) {
    if axis.scale_kind == ScaleKind::Logarithmic {
        return log_value_domain(spec, axis);
    }
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
    // Line の stacked area は Bar の value_stacked と同じ「カテゴリごと正負サム独立集計」
    // ロジックを共有する。対数軸との組み合わせは関数冒頭の early return
    // (axis.scale_kind == Logarithmic は log_value_domain へ委譲)がこの分岐の手前で
    // 弾くため、Bar/Line を問わずここには到達しない。
    if matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        } | crate::ir::ChartKind::Line { stacked: true, .. }
    ) {
        // 積み上げ: カテゴリ・stack ID ごとに正値の和(上限)・負値の和(下限)をとる。
        // chart.js 互換: beginAtZero=false のとき 0 ではなく実データの個別値を境界にする。
        // 全正値ケース(neg_sum が常に 0)では min_individual を下限として使う。
        // 全負値ケース(pos_sum が常に 0)では max_individual を上限として使う。
        let mut has_positive = false;
        let mut has_negative = false;
        let mut min_individual = f64::INFINITY;
        let mut max_individual = f64::NEG_INFINITY;
        let (series_groups, group_count) = stack_group_indices(&spec.series);
        let mut pos_sums = vec![0.0_f64; group_count];
        let mut neg_sums = vec![0.0_f64; group_count];
        for i in 0..spec.categories.len() {
            pos_sums.fill(0.0);
            neg_sums.fill(0.0);
            for (series_index, ser) in spec.series.iter().enumerate() {
                if let Some(&v) = ser.values.get(i)
                    && v.is_finite()
                {
                    if v < min_individual {
                        min_individual = v;
                    }
                    if v > max_individual {
                        max_individual = v;
                    }
                    let group = series_groups[series_index];
                    if v >= 0.0 {
                        pos_sums[group] += v;
                        has_positive = true;
                    } else {
                        neg_sums[group] += v;
                        has_negative = true;
                    }
                }
            }
            for &pos_sum in &pos_sums {
                if pos_sum > data_max {
                    data_max = pos_sum;
                }
            }
            for &neg_sum in &neg_sums {
                if neg_sum < data_min {
                    data_min = neg_sum;
                }
            }
        }
        if !has_negative && min_individual.is_finite() {
            data_min = min_individual;
        }
        if !has_positive && max_individual.is_finite() {
            data_max = max_individual;
        }
    } else {
        for s in &spec.series {
            for &v in &s.values {
                if v.is_finite() {
                    if v < data_min {
                        data_min = v;
                    }
                    if v > data_max {
                        data_max = v;
                    }
                }
            }
        }
    }
    resolve_axis_domain(axis, data_min, data_max)
}

/// Maps each series to a compact stack-group index in first-seen order.
/// Unset IDs share one legacy group; Chart.js fills omitted IDs with the type default before
/// constructing the IR, so explicit bar/line IDs join the corresponding default.
pub(crate) fn stack_group_indices(series: &[crate::ir::Series]) -> (Vec<usize>, usize) {
    let mut groups_by_id = std::collections::HashMap::<Option<&str>, usize>::new();
    let mut group_count = 0;
    let indices = series
        .iter()
        .map(|series| {
            *groups_by_id
                .entry(series.stack.as_deref())
                .or_insert_with(|| {
                    let index = group_count;
                    group_count += 1;
                    index
                })
        })
        .collect();
    (indices, group_count)
}

/// Cartesian 線形軸の自動 domain に hard min/max、suggested、beginAtZero を適用する。
/// 明示 min/max はその側の自動調整より優先し、反対側はデータから引き続き決める。
pub(crate) fn resolve_axis_domain(axis: &AxisSpec, data_min: f64, data_max: f64) -> (f64, f64) {
    let hard_min = axis.min.filter(|v| v.is_finite());
    let hard_max = axis.max.filter(|v| v.is_finite());
    let min_is_hard = hard_min.is_some();
    let max_is_hard = hard_max.is_some();

    // データなしでは suggested を初期値にする。hard 側が指定されていればそちらを使う。
    let mut lo = hard_min.unwrap_or_else(|| {
        if data_min.is_finite() {
            data_min
        } else {
            axis.suggested_min.filter(|v| v.is_finite()).unwrap_or(0.0)
        }
    });
    let mut hi = hard_max.unwrap_or_else(|| {
        if data_max.is_finite() {
            data_max
        } else {
            axis.suggested_max
                .filter(|v| v.is_finite())
                .unwrap_or(if lo == 0.0 { 1.0 } else { lo + 1.0 })
        }
    });

    // suggested と beginAtZero は hard 指定の無い側だけを動かす。
    if !min_is_hard {
        if let Some(s) = axis.suggested_min
            && s.is_finite()
            && s < lo
        {
            lo = s;
        }
        if axis.begin_at_zero {
            lo = lo.min(0.0);
        }
    }
    if !max_is_hard {
        if let Some(s) = axis.suggested_max
            && s.is_finite()
            && s > hi
        {
            hi = s;
        }
        if axis.begin_at_zero {
            hi = hi.max(0.0);
        }
    }

    if hi <= lo {
        if min_is_hard || max_is_hard {
            // hard と自動側が交差したら hard 端を維持して自動端を 5% 開く。
            // 両側 hard が矛盾する場合も決定的に min を優先し、描画 domain を有効にする。
            let anchor = hard_min.or(hard_max).unwrap_or(lo);
            let step = if anchor == 0.0 {
                1.0
            } else {
                (anchor.abs() * 0.05).max(anchor.abs().max(1.0) * f64::EPSILON * 4.0)
            };
            if min_is_hard {
                let upper = anchor + step;
                if upper.is_finite() && upper > anchor {
                    hi = upper;
                }
            } else {
                let lower = anchor - step;
                if lower.is_finite() && lower < anchor {
                    lo = lower;
                }
            }
        } else {
            // hard 制約のない縮退 domain は既存の挙動を保つ。
            hi = lo + 1.0;
        }
    }
    (lo, hi)
}

/// nice tick の外側丸めを、Chart.js の明示 `min` / `max` で固定する。
/// hard endpoint は目盛列にも含め、固定 domain と軸ラベルの範囲を一致させる。
pub(crate) fn apply_hard_axis_bounds(mut ticks: NiceTicks, axis: &AxisSpec) -> NiceTicks {
    let hard_min = axis.min.filter(|v| v.is_finite());
    let hard_max = axis
        .max
        .filter(|v| v.is_finite() && hard_min.is_none_or(|min| *v > min));
    if let Some(min) = hard_min {
        ticks.min = min;
    }
    if let Some(max) = hard_max {
        ticks.max = max;
    }
    ticks
        .ticks
        .retain(|&tick| tick >= ticks.min && tick <= ticks.max);
    if let Some(min) = hard_min
        && !ticks.ticks.contains(&min)
    {
        ticks.ticks.push(min);
    }
    if let Some(max) = hard_max
        && !ticks.ticks.contains(&max)
    {
        ticks.ticks.push(max);
    }
    ticks.ticks.sort_by(f64::total_cmp);
    ticks.ticks.dedup_by(|left, right| *left == *right);
    if axis.ticks.count.is_none()
        && let Some(configured_limit) = axis.ticks.max_ticks_limit
    {
        let limit = configured_limit.clamp(2, crate::scale::MAX_TICK_INTERVALS + 1);
        if ticks.ticks.len() > limit {
            let source = std::mem::take(&mut ticks.ticks);
            let last = source.len() - 1;
            ticks.ticks = (0..limit)
                .map(|index| (index * last + (limit - 1) / 2) / (limit - 1))
                .map(|index| source[index])
                .collect();
            if ticks.ticks.len() >= 2 {
                ticks.step = ticks.ticks[1] - ticks.ticks[0];
            }
        }
    }
    ticks
}

/// options.scales.{x,y}.ticks を適用した線形軸目盛り。
pub(crate) fn configured_axis_ticks(
    domain_min: f64,
    domain_max: f64,
    axis: &AxisSpec,
) -> NiceTicks {
    let ticks =
        crate::scale::configured_ticks(domain_min, domain_max, &axis.ticks, axis.min, axis.max);
    apply_hard_axis_bounds(ticks, axis)
}

/// 数値軸の目盛ラベル。対数軸は従来形式、線形軸は ticks.format を使う。
pub(crate) fn format_axis_tick(axis: &AxisSpec, tick: f64) -> String {
    if axis.scale_kind == ScaleKind::Logarithmic {
        crate::num::fmt_num_log(tick)
    } else {
        crate::num::fmt_axis_tick(tick, axis.ticks.format.as_ref())
    }
}

/// 値が線形/対数軸の可視 domain 内にあるかを返す。
pub(crate) fn axis_value_in_bounds(value: f64, ticks: &NiceTicks) -> bool {
    value.is_finite() && value >= ticks.min && value <= ticks.max
}

/// Returns whether the value interval intersects the visible axis domain before clipping.
pub(crate) fn axis_interval_intersects_range(start: f64, end: f64, ticks: &NiceTicks) -> bool {
    !start.is_nan() && !end.is_nan() && start.min(end) <= ticks.max && start.max(end) >= ticks.min
}

/// 範囲外の値を軸端へ制限し、描画座標がプロット領域から出ないようにする。
pub(crate) fn clip_axis_value(value: f64, ticks: &NiceTicks) -> f64 {
    value.clamp(ticks.min, ticks.max)
}

/// 対数軸専用のドメイン計算。線形版(上の `value_domain` 本体)と異なる点:
/// `begin_at_zero` は「0 をドメインに含める」という線形の意味では効かない(0 は
/// 対数軸に存在しえない)が、chart.js 実機で確認した通り、代わりに domain_min を
/// 最小正値の "decade floor"(10^floor(log10(min_positive)))へ切り下げる効果を
/// 持つ(`crate::scale::is_exact_decade_boundary` 参照。床がちょうど min_positive
/// 自身と一致する — 例えば min_positive がすでに 10^n の — 場合は、その値の
/// バー/点が軸の床と重なって高さ0になるのを避けるため、さらにもう1桁下げる)。
/// 0 は最小正値の1桁下に置換してドメインへ含め、負値は通常の有限値フィルタで
/// 自然に除外される。
/// `suggested_min`/`suggested_max` は正の値のみ尊重する。
/// 正の `min`/`max` は hard bound として指定側を固定し、suggested やデータによる
/// 拡張より優先する。非正の hard bound は対数軸で使えないため無視する。
///
/// `ChartKind::Bar { value_stacked: true, .. }` と `ChartKind::Line { stacked: true }` は
/// カテゴリ・stack ID ごとに正の値を合算して domain 上限へ含める。対数軸では非正値を
/// 写像できないため積み上げの合計にも含めない。
fn log_value_domain(spec: &ChartSpec, axis: &AxisSpec) -> (f64, f64) {
    let mut min_positive = f64::INFINITY;
    let mut max_positive = f64::NEG_INFINITY;
    let mut has_zero = false;
    let is_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        } | crate::ir::ChartKind::Line { stacked: true, .. }
    );
    let (series_groups, group_count) = stack_group_indices(&spec.series);
    let mut positive_stack_sums = if is_stacked {
        vec![vec![0.0_f64; spec.categories.len()]; group_count]
    } else {
        Vec::new()
    };
    for (series_index, s) in spec.series.iter().enumerate() {
        for (index, &v) in s.values.iter().enumerate() {
            if !v.is_finite() {
                continue;
            }
            if v == 0.0 {
                has_zero = true;
                continue;
            }
            if v > 0.0 {
                if v < min_positive {
                    min_positive = v;
                }
                if v > max_positive {
                    max_positive = v;
                }
                if let Some(sum) = positive_stack_sums
                    .get_mut(series_groups[series_index])
                    .and_then(|sums| sums.get_mut(index))
                {
                    // 正のスタック合計を有限に保つ。極端な IR 入力で加算が overflow
                    // した場合は、有限 f64 の最大値で飽和させる。
                    *sum = if *sum > f64::MAX - v {
                        f64::MAX
                    } else {
                        *sum + v
                    };
                }
            }
            // v < 0.0 は対数軸に写像できないため、ドメインから除外する。
        }
    }
    if is_stacked {
        for sums in positive_stack_sums {
            for sum in sums {
                if sum > max_positive {
                    max_positive = sum;
                }
            }
        }
    }

    log_axis_domain_from_extrema(axis, min_positive, max_positive, has_zero)
}

/// 対数軸のデータ極値と `AxisSpec` から domain を解決する。
/// `min_positive` / `max_positive` が有限でない場合は正のデータが無いものとして扱う。
pub(crate) fn log_axis_domain_from_extrema(
    axis: &AxisSpec,
    min_positive: f64,
    max_positive: f64,
    has_zero: bool,
) -> (f64, f64) {
    let hard_min = axis.min.filter(|s| s.is_finite() && *s > 0.0);
    let hard_max = axis.max.filter(|s| s.is_finite() && *s > 0.0);
    let (mut domain_min, mut domain_max) = if !min_positive.is_finite() || !max_positive.is_finite()
    {
        // 正データが1つもない(空 / 0 のみ / 負のみ)。正の hard min/max があれば
        // suggested より優先し、なければ正の suggested を初期シードにする。
        // begin_at_zero は対数軸では無関係(0 はドメインに含められない)。下の suggested
        // 適用ブロックを素通りしないよう、ここで早期 return せず通常経路に合流させる。
        //
        // 実機バグ回帰テスト: 以前は片方だけ指定された場合の既定値が固定 1.0/10.0
        // だったため、suggested_max だけがサブユニット(例: 0.01)で指定されても
        // 固定の lo=1.0 と比較され「hi <= lo」に落ちて lo*10.0=10.0 に潰れ、
        // 明示的に設定した軸オプションが完全に無視されていた(PR #144 の自動
        // レビューで指摘)。min/suggested_min のみ指定時は hi=lo*10、max/suggested_max
        // のみ指定時は lo=hi/10 と、常に「指定された側」を基準に対辺を導出する。
        let lo = hard_min.or_else(|| axis.suggested_min.filter(|s| s.is_finite() && *s > 0.0));
        let hi = hard_max.or_else(|| axis.suggested_max.filter(|s| s.is_finite() && *s > 0.0));
        match (lo, hi) {
            (Some(lo), Some(hi)) if hi > lo => (lo, hi),
            (Some(lo), Some(_)) if hard_min.is_some() => {
                // hard min は維持し、矛盾する hard max / suggestion より上へ開く。
                (lo, (lo * 10.0).min(f64::MAX))
            }
            (Some(_), Some(hi)) if hard_max.is_some() => {
                // hard max は維持し、矛盾する suggestion より下へ開く。
                let below = hi / 10.0;
                if below.is_finite() && below > 0.0 {
                    (below, hi)
                } else {
                    (hi, (hi * 10.0).min(f64::MAX))
                }
            }
            // 両方指定されているが hi <= lo(不正な指定)の場合は lo を基準にする。
            (Some(lo), _) => (lo, (lo * 10.0).min(f64::MAX)),
            (None, Some(hi)) => {
                let below = hi / 10.0;
                if below.is_finite() && below > 0.0 {
                    (below, hi)
                } else {
                    (hi, hi * 10.0)
                }
            }
            (None, None) => (1.0, 10.0),
        }
    } else {
        // min_positive を1桁下げる(min_positive/10)、ただし非正規化数(subnormal)
        // 近傍で 0.0 へアンダーフローする場合は下げず min_positive のまま使う
        // (「対数ドメインの下端は正」という不変条件の防御。実用上あり得ない極小
        // データだが、パニックにも 0 除算的な誤ったドメインにもしないため)。
        let decade_below = || {
            let d = min_positive / 10.0;
            if d.is_finite() && d > 0.0 {
                d
            } else {
                min_positive
            }
        };
        let automatic_min = if has_zero {
            // データに 0 が含まれる場合は begin_at_zero の値によらず常に1桁下げる
            // (chart.js 実測で確認済み: beginAtZero:false でも 0 混在データの min は
            // 変わらない)。
            decade_below()
        } else if axis.begin_at_zero {
            // chart.js 実測(tools/ で node chart.js 実行し、40/80/11/5/2/10/100/999/12/99
            // 等の多数の値で確認): beginAtZero:true の対数軸は、min_positive の
            // "decade floor"(10^floor(log10(min_positive)))に常にドメイン下端を
            // 切り下げる。ちょうど decade 境界(min_positive 自身が 10^n)のときは
            // その切り下げが no-op になってしまう(床が自分自身と一致する)ため、
            // さらにもう1 decade 下げる。
            //
            // ピクセル写像は `log_ticks_within(domain_min, domain_max)` が返す
            // tight ドメインをそのまま使う(P1 修正済み、common.rs::compute() の
            // ValueScale::Log 構築部参照)ため、ここで求めた domain_min の値が
            // そのまま軸の描画上の床になる。この関数がまだ log_ticks の decade
            // 外側丸めに依存していた頃は、非境界値(例: 40)を切り下げなくても
            // log_ticks 自身が同じ丸めを暗黙に行っていたため見た目に差が出ず、
            // 一時的にこの一般規則を「境界のときだけ」に縮小していたが、tight
            // ドメイン化により log_ticks 側の暗黙の丸めが無くなったため、
            // この一般規則を明示的に適用しないと非境界の最小値バーの高さが
            // 0 になって消えてしまう(実機で再現・確認済み)。
            let exp = min_positive.log10().floor();
            let decade_floor = 10f64.powf(exp);
            if crate::scale::is_exact_decade_boundary(min_positive) {
                let one_more = decade_floor / 10.0;
                if one_more.is_finite() && one_more > 0.0 {
                    one_more
                } else {
                    decade_floor
                }
            } else if decade_floor.is_finite() && decade_floor > 0.0 && decade_floor < min_positive
            {
                decade_floor
            } else {
                // decade_floor の計算が破綻した極端な入力(非正規化数近傍等)への防御。
                min_positive
            }
        } else {
            min_positive
        };
        (
            hard_min.unwrap_or(automatic_min),
            hard_max.unwrap_or(max_positive),
        )
    };

    if hard_min.is_none()
        && let Some(s) = axis.suggested_min
        && s.is_finite()
        && s > 0.0
        && s < domain_min
    {
        domain_min = s;
    }
    if hard_max.is_none()
        && let Some(s) = axis.suggested_max
        && s.is_finite()
        && s > 0.0
        && s > domain_max
    {
        domain_max = s;
    }
    if domain_max <= domain_min {
        if hard_min.is_some() {
            // hard min は維持し、交差した自動側または矛盾した hard max を上へ開く。
            let expanded = domain_min * 10.0;
            domain_max = if expanded.is_finite() && expanded > domain_min {
                expanded
            } else {
                f64::MAX
            };
        } else if hard_max.is_some() {
            // hard max は維持し、交差した自動 min を1 decade 下へ開く。
            let lower = domain_max / 10.0;
            if lower.is_finite() && lower > 0.0 && lower < domain_max {
                domain_min = lower;
            }
        } else {
            // 下端×10 が overflow するか丸めで変化しない場合は、上端÷10 で下側へ広げる。
            // これにより単一の f64::MAX でも幅のある有限 domain になる。
            let expanded = domain_min * 10.0;
            if expanded.is_finite() && expanded > domain_min {
                domain_max = expanded;
            } else {
                let lower = domain_max / 10.0;
                if lower.is_finite() && lower > 0.0 && lower < domain_max {
                    domain_min = lower;
                } else {
                    // 既知の有限端点を維持する最終 fallback。
                    domain_max = f64::MAX;
                }
            }
        }
    }
    (domain_min, domain_max)
}

/// Text laid from an axis start edge toward its end edge may overflow either
/// side depending on its anchor. Returns `(before_start, after_end)`.
fn aligned_title_overflow(
    measured_extent: f64,
    plot_extent: f64,
    align: AxisTitleAlign,
) -> (f64, f64) {
    let excess = (measured_extent - plot_extent).max(0.0);
    match align {
        AxisTitleAlign::Start => (0.0, excess),
        AxisTitleAlign::End => (excess, 0.0),
        AxisTitleAlign::Center => (excess / 2.0, excess / 2.0),
    }
}

/// spec から y ドメイン(線形軸: begin_at_zero尊重。対数軸は `value_domain` 参照)・
/// nice_ticks・y軸ラベル幅・プロット領域・凡例帯を計算。
pub fn compute(spec: &ChartSpec, m: &TextMeasurer) -> Frame {
    // y ドメイン。
    let (domain_min, domain_max) = value_domain(spec, &spec.y_axis);
    let is_log = spec.y_axis.scale_kind == ScaleKind::Logarithmic;
    let (mut ticks, minor_ticks) = if is_log {
        let log = crate::scale::log_ticks_within(domain_min, domain_max);
        (
            NiceTicks {
                min: log.min,
                max: log.max,
                // 対数軸では decade 間隔が一定でない(1,10,100,...)ため "step" は
                // 意味を持たない。0.0 は「非対数の step とは値域が異なる」ことを示す
                // 番兵(nice_ticks/vega_nice_ticks は常に step>0 を返すため 0.0 は
                // log 専用の合図になる)。model.rs の introspection API はこの番兵を
                // 外部に漏らさないよう `step: None` に変換して公開する
                // (`model.rs::logarithmic_axis` 参照)。
                step: 0.0,
                ticks: log.major,
            },
            log.minor,
        )
    } else if matches!(spec.size_mode, SizeMode::PlotArea)
        && matches!(spec.kind, ChartKind::Line { .. })
        && matches!(spec.x_positions, XPositions::Temporal { .. })
    {
        (
            vega_nice_ticks(domain_min, domain_max, spec.height),
            Vec::new(),
        )
    } else {
        (
            crate::scale::configured_ticks(
                domain_min,
                domain_max,
                &spec.y_axis.ticks,
                spec.y_axis.min,
                spec.y_axis.max,
            ),
            Vec::new(),
        )
    };
    if !is_log {
        ticks = apply_hard_axis_bounds(ticks, &spec.y_axis);
    }

    // y 軸ラベル幅。対数軸は fmt_num_log を使う(幅の広いラベルでクリップさせない)。
    let mut max_w = 0.0_f32;
    for &t in &ticks.ticks {
        let s = format_axis_tick(&spec.y_axis, t);
        let w = m.width(&s, spec.theme.font_size as f32);
        if w > max_w {
            max_w = w;
        }
    }
    // y 軸タイトル(回転テキスト)の帯幅。text 幅(font_size)+ ベースラインギャップ(6px)。
    // Task 6 で spec.y_axis.title は display=false / text 空のとき None に潰されているので、
    // ここでは Some の場合だけ帯幅を足す。
    let y_title_w = spec
        .y_axis
        .title
        .as_ref()
        .map(|t| t.font_size.unwrap_or(spec.theme.font_size * 1.1) + 6.0)
        .unwrap_or(0.0);
    let y_axis_w = max_w as f64 + 10.0 + y_title_w;

    // 凡例の有無。
    let legend = has_legend(spec);
    let legend_title = legend_title(spec);
    let legend_font = legend_label_font_size(&spec.legend_options, spec.theme.font_size);
    let horizontal_legend_height = legend_horizontal_band_height(
        &spec.legend_options,
        spec.theme.font_size,
        legend_title.is_some(),
    );

    // プロット領域。
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_top = if legend && spec.legend == LegendPos::Top {
        horizontal_legend_height
    } else {
        0.0
    };
    let legend_bottom = if legend && spec.legend == LegendPos::Bottom {
        horizontal_legend_height
    } else {
        0.0
    };
    // Left/Right の凡例帯幅(系列名から算出)。Top/Bottom 時は 0。
    let mut series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
    series_names.extend(legend_title.map(str::to_owned));
    let legend_left = if legend && spec.legend == LegendPos::Left {
        legend_band_width_vertical_styled(m, &series_names, legend_font, &spec.legend_options)
    } else {
        0.0
    };
    let legend_right = if legend && spec.legend == LegendPos::Right {
        legend_band_width_vertical_styled(m, &series_names, legend_font, &spec.legend_options)
    } else {
        0.0
    };
    let vertical_legend_height =
        if legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
            let row_height = legend_vertical_row_height(&spec.legend_options, spec.theme.font_size);
            spec.series.len() as f64 * row_height
                + if legend_title.is_some() {
                    legend_vertical_title_height(&spec.legend_options, spec.theme.font_size)
                } else {
                    0.0
                }
        } else {
            0.0
        };
    let vertical_legend_overflow = ((vertical_legend_height - spec.height) / 2.0).max(0.0);
    let (rotated_y_title_top_overflow, rotated_y_title_bottom_overflow) =
        if matches!(spec.size_mode, SizeMode::PlotArea) {
            spec.y_axis
                .title
                .as_ref()
                .map(|title| {
                    let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
                    // Rotated -90° text is laid from bottom (axis Start) to top
                    // (axis End), so physical top/bottom reverse the helper tuple.
                    let (bottom, top) = aligned_title_overflow(
                        m.width(&title.text, font as f32) as f64,
                        spec.height,
                        title.align,
                    );
                    (top, bottom)
                })
                .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };
    let plot_area_top_overflow = vertical_legend_overflow.max(rotated_y_title_top_overflow);
    let plot_area_bottom_overflow = vertical_legend_overflow.max(rotated_y_title_bottom_overflow);
    // PlotArea の幅はこの時点で確定しているため、scene 寸法より先に temporal tick を
    // 作れる。端ラベルが中央寄せされても scene から切れないよう、その半幅を
    // 左側の最低 plot offset と右側の最低余白として使う。
    let plot_area_temporal_ticks = if matches!(spec.size_mode, SizeMode::PlotArea) {
        match &spec.x_positions {
            XPositions::Temporal { unix_millis } => unix_millis
                .first()
                .zip(unix_millis.last())
                .map(|(&min, &max)| temporal_ticks(min, max, spec.width))
                .unwrap_or_default(),
            XPositions::Category => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let temporal_edge_pad_left = plot_area_temporal_ticks
        .first()
        .map(|tick| m.width(&tick.label, spec.theme.font_size as f32) as f64 / 2.0)
        .unwrap_or(0.0);
    let temporal_edge_pad_right = plot_area_temporal_ticks
        .last()
        .map(|tick| m.width(&tick.label, spec.theme.font_size as f32) as f64 / 2.0)
        .unwrap_or(0.0);
    let (plot_area_title_left_overflow, plot_area_title_right_overflow) =
        if matches!(spec.size_mode, SizeMode::PlotArea) {
            let chart_title_side_overflow = spec
                .title
                .as_ref()
                .map(|title| {
                    ((m.width(title, TITLE_FONT as f32) as f64 - spec.width) / 2.0).max(0.0)
                })
                .unwrap_or(0.0);
            let (x_title_left_overflow, x_title_right_overflow) = spec
                .x_axis
                .title
                .as_ref()
                .map(|title| {
                    let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
                    aligned_title_overflow(
                        m.width(&title.text, font as f32) as f64,
                        spec.width,
                        title.align,
                    )
                })
                .unwrap_or((0.0, 0.0));
            (
                chart_title_side_overflow.max(x_title_left_overflow),
                chart_title_side_overflow.max(x_title_right_overflow),
            )
        } else {
            (0.0, 0.0)
        };
    // line(edge-to-edge)では先頭/末尾の点が plot_left/plot_right に乗り、中央寄せの
    // x ラベルが点の外側へ半幅はみ出してキャンバス端でクリップされる。chart.js が
    // chartArea を edge ラベル半幅ぶん内側へ取るのと同様に edge 余白を確保する。
    // 末尾は常に内側化し、先頭は y 軸ラベル幅で足りなければ拡張する。
    // offset:true の line は bar 同様 band 中心配置でラベルがプロット内に収まるため、
    // 端余白は取らない(bar と同じ chartArea を使う)。
    let (edge_pad_left, edge_pad_right) = if matches!(spec.kind, ChartKind::Line { .. })
        && spec.categories.len() > 1
        && !spec.x_axis.offset
    {
        let lf = spec.theme.font_size as f32;
        let half = |c: &String| (m.width(c, lf) as f64) / 2.0;
        let first = spec
            .categories
            .first()
            .filter(|c| !c.is_empty())
            .map_or(0.0, half);
        let last = spec
            .categories
            .last()
            .filter(|c| !c.is_empty())
            .map_or(0.0, half);
        (first, last)
    } else {
        (0.0, 0.0)
    };
    // 狭い幅 + 長い端ラベルで edge 余白が利用可能幅を超えると plot_right <= plot_left に
    // 反転し line_x が壊れる。余白合計を利用可能幅で比例縮小し、最後に plot_right >= plot_left
    // を保証する。
    // X 軸タイトルがあれば、x カテゴリラベル帯の下側にさらにタイトル帯を確保して plot_bottom を上へ押し上げる。
    let x_title_h = if spec.x_axis.title.is_some() {
        AXIS_TITLE_BAND
    } else {
        0.0
    };
    let (scene_width, scene_height, plot_left, plot_right, plot_top, plot_bottom) = match spec
        .size_mode
    {
        SizeMode::Canvas => {
            let base_left = OUTER_PAD + y_axis_w + legend_left;
            let base_right = spec.width - OUTER_PAD - legend_right;
            let edge_total = edge_pad_left + edge_pad_right;
            let scale = if edge_total > 0.0 {
                ((base_right - base_left).max(0.0) / edge_total).min(1.0)
            } else {
                1.0
            };
            let plot_left = base_left.max(OUTER_PAD + legend_left + edge_pad_left * scale);
            let plot_right = (base_right - edge_pad_right * scale).max(plot_left);
            let plot_top = OUTER_PAD + title_band + legend_top;
            let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;
            (
                spec.width,
                spec.height,
                plot_left,
                plot_right,
                plot_top,
                plot_bottom,
            )
        }
        SizeMode::PlotArea => {
            let required_title_left_band = OUTER_PAD + plot_area_title_left_overflow;
            let required_title_right_band = OUTER_PAD + plot_area_title_right_overflow;
            let plot_left = (OUTER_PAD + y_axis_w)
                .max(temporal_edge_pad_left)
                .max(required_title_left_band);
            let plot_top = OUTER_PAD + title_band + plot_area_top_overflow;
            let plot_right = plot_left + spec.width;
            let plot_bottom = plot_top + spec.height;
            let trailing_band = (OUTER_PAD + legend_right)
                .max(temporal_edge_pad_right)
                .max(required_title_right_band);
            let scene_width = plot_right + trailing_band;
            let scene_height = plot_bottom
                + X_LABEL_BAND
                + x_title_h
                + OUTER_PAD
                + legend_bottom
                + plot_area_bottom_overflow;
            (
                scene_width,
                scene_height,
                plot_left,
                plot_right,
                plot_top,
                plot_bottom,
            )
        }
    };

    // y スケール（上下反転）。対数軸は log10 空間の LinearScale を内側に持つ
    // ValueScale::Log。ticks.min/max は log_ticks_within(domain_min, domain_max) の
    // 戻り値で、渡した tight ドメイン(常に正、domain_min < domain_max)をそのまま
    // 折り返す(decade 境界には丸めない)。chart.js 実機は log 軸のピクセル写像を
    // tight データドメインでそのまま行う(scale.min/max がそれ)ため、これに合わせる
    // (PR #144 の自動レビュー P1 指摘)。floor は ValueScale::Log::map が
    // 0/負値/丸め誤差を log10 前にクランプする下限として使う。
    let ys = if is_log {
        ValueScale::Log {
            inner: LinearScale::new(ticks.min.log10(), ticks.max.log10(), plot_bottom, plot_top),
            floor: ticks.min,
        }
    } else {
        ValueScale::Linear(LinearScale::new(
            ticks.min,
            ticks.max,
            plot_bottom,
            plot_top,
        ))
    };
    let temporal_ticks = match &spec.x_positions {
        XPositions::Temporal { unix_millis } => unix_millis
            .first()
            .zip(unix_millis.last())
            .map(|(&min, &max)| temporal_ticks(min, max, plot_right - plot_left))
            .unwrap_or_default(),
        XPositions::Category => Vec::new(),
    };

    Frame {
        scene_width,
        scene_height,
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
        ticks,
        ys,
        minor_ticks,
        temporal_ticks,
    }
}

/// n カテゴリ中 i 番目の x 中心。band_w=(plot_right-plot_left)/n。
pub fn category_center(frame: &Frame, i: usize, n: usize) -> f64 {
    let band_w = (frame.plot_right - frame.plot_left) / n.max(1) as f64;
    frame.plot_left + (i as f64 + 0.5) * band_w
}

/// line/area の x 座標。chart.js の category スケール offset:false(edge-to-edge)に合わせ、
/// n 個のカテゴリを [plot_left, plot_right] へ i/(n-1) で等間隔配置する(先頭=左端・末尾=右端)。
/// bar の band 中心(category_center)とは異なる。n<=1 は (n-1)=0 で NaN になるため
/// プロット中央へフォールバックする(縮退ケース; 単一カテゴリの line fixture は存在しない)。
fn line_edge_x(frame: &Frame, i: usize, n: usize) -> f64 {
    if n <= 1 {
        return frame.plot_left + (frame.plot_right - frame.plot_left) / 2.0;
    }
    frame.plot_left + i as f64 * (frame.plot_right - frame.plot_left) / (n - 1) as f64
}

/// line/area の category x 座標を x 軸の offset 設定に応じて選ぶ単一窓口。
/// offset:true → category_center(bar 同様の band 中心)、false → line_x(edge-to-edge)。
/// line.rs の点計算と draw_frame の x ラベル(いずれも ChartKind::Line 経路)が共有し、
/// offset 判定の分岐を一元化する。mixed は mixed::build が category_center を直接使い
/// この関数を呼ばないため、ここで ChartKind を分岐する必要はない。
///
/// `category_center`/`line_x` が純粋な幾何プリミティブ(外部からの利用に意味がある)なのに対し、
/// これは spec.kind/offset を読む種別ディスパッチのラッパーであり、line レイアウトの内部都合。
/// 公開 API に晒すと「mixed 幾何にも使える」という誤解と契約を生むため `pub(crate)` に限定する。
pub(crate) fn line_category_x(spec: &ChartSpec, frame: &Frame, i: usize, n: usize) -> f64 {
    if spec.x_axis.offset {
        category_center(frame, i, n)
    } else {
        line_edge_x(frame, i, n)
    }
}

/// line/area の x 座標。カテゴリは従来どおり等間隔、temporal は経過時間に比例させる。
pub fn line_x(spec: &ChartSpec, frame: &Frame, index: usize) -> f64 {
    match &spec.x_positions {
        XPositions::Category => line_category_x(spec, frame, index, spec.categories.len().max(1)),
        XPositions::Temporal { unix_millis } => {
            let min = *unix_millis.first().unwrap_or(&0);
            let max = *unix_millis.last().unwrap_or(&min);
            let value = *unix_millis.get(index).unwrap_or(&min);
            temporal_x(frame, min, max, value)
        }
    }
}

fn temporal_x(frame: &Frame, min: i64, max: i64, value: i64) -> f64 {
    if min == max {
        (frame.plot_left + frame.plot_right) / 2.0
    } else {
        let numerator = value as i128 - min as i128;
        let denominator = max as i128 - min as i128;
        let ratio = numerator as f64 / denominator as f64;
        frame.plot_left + ratio * (frame.plot_right - frame.plot_left)
    }
}

pub fn band_width(frame: &Frame, n: usize) -> f64 {
    (frame.plot_right - frame.plot_left) / n.max(1) as f64
}

fn categorical_tick_step(
    categories: &[String],
    slot_w: f64,
    m: &TextMeasurer,
    label_font: f64,
) -> usize {
    let required_width = categories
        .iter()
        .find(|category| !category.is_empty())
        .map(|label| m.width(label, label_font as f32) as f64 + 4.0)
        // 空ラベルだけでも subpixel ごとに grid を積まないよう、既存の label gap を最小幅にする。
        .unwrap_or(4.0);
    if slot_w > 0.0 && required_width > slot_w {
        ((required_width / slot_w).ceil() as usize).max(1)
    } else {
        1
    }
}

/// 共有フレーム描画: タイトル→横グリッド+yラベル→xベースライン→xカテゴリラベル→凡例。
/// チャート本体(bar/line)はこの後に重ねて描く。
pub fn draw_frame(items: &mut Vec<Prim>, spec: &ChartSpec, frame: &Frame, m: &TextMeasurer) {
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 1. タイトル。
    if let Some(title) = &spec.title {
        let title_x = match spec.size_mode {
            SizeMode::Canvas => spec.width / 2.0,
            SizeMode::PlotArea => (frame.plot_left + frame.plot_right) / 2.0,
        };
        items.push(Prim::Text {
            x: title_x,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // 2. 横グリッド + y 軸ラベル(主目盛)。対数軸は fmt_num_log でラベルを描く。
    let grid_cfg = &spec.y_axis.grid;
    let grid_color = grid_cfg.color.unwrap_or(spec.theme.grid_color);
    for &t in &frame.ticks.ticks {
        let y = frame.ys.map(t);
        if grid_cfg.display {
            items.push(Prim::Line {
                x1: frame.plot_left,
                y1: y,
                x2: frame.plot_right,
                y2: y,
                stroke: grid_color,
                stroke_width: grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
        items.push(Prim::Text {
            x: frame.plot_left - 6.0,
            y: y + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: format_axis_tick(&spec.y_axis, t),
            rotate_deg: None,
        });
    }
    // 2b. 対数軸の minor グリッド(mantissa 2..9、ラベルなし)。線形軸では
    // frame.minor_ticks が常に空なので no-op。1 decade あたり major の8倍(mantissa
    // 2..9)の本数になり、major と同じ濃さだと decade 境界が埋もれる。Chart.js に
    // 倣い、視認性のため半透明(アルファ半減)で薄く描く(tick-for-tick parity ではなく
    // 見た目の可読性目的の意図的な調整。scale.rs::LogTicks の非目標セクション参照)。
    if grid_cfg.display {
        let minor_grid_color = Color {
            a: grid_color.a * 0.5,
            ..grid_color
        };
        for &t in &frame.minor_ticks {
            let y = frame.ys.map(t);
            items.push(Prim::Line {
                x1: frame.plot_left,
                y1: y,
                x2: frame.plot_right,
                y2: y,
                stroke: minor_grid_color,
                stroke_width: grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }

    // 3. カテゴリカル x grid。baseline より先に置き、交点を border が覆うようにする。
    if matches!(spec.x_positions, XPositions::Category) && spec.x_axis.grid.display {
        let n = spec.categories.len().max(1);
        let slot_w = (frame.plot_right - frame.plot_left) / n as f64;
        let step = categorical_tick_step(&spec.categories, slot_w, m, label_font);
        let mut grid_path = String::new();
        for i in (0..spec.categories.len()).step_by(step) {
            let x = if matches!(spec.kind, ChartKind::Line { .. }) {
                line_x(spec, frame, i)
            } else {
                category_center(frame, i, n)
            };
            if !grid_path.is_empty() {
                grid_path.push(' ');
            }
            grid_path.push_str(&format!(
                "M {} {} L {} {}",
                fmt_num(x),
                fmt_num(frame.plot_top),
                fmt_num(x),
                fmt_num(frame.plot_bottom)
            ));
        }
        if !grid_path.is_empty() {
            items.push(Prim::Path {
                d: grid_path,
                fill: None,
                stroke: Some(spec.x_axis.grid.color.unwrap_or(spec.theme.grid_color)),
                stroke_width: spec.x_axis.grid.line_width,
            });
        }
    }

    // 3b. x ベースライン。border.display / border.color / border.width / border.dash を反映。
    let border = &spec.x_axis.border;
    if border.display {
        let border_color = border.color.unwrap_or(ink);
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: frame.plot_bottom,
            x2: frame.plot_right,
            y2: frame.plot_bottom,
            stroke: border_color,
            stroke_width: border.width,
            dash: border.dash.clone(),
        });
    }

    // 3b. y 軸目盛(tick 刻み)。draw_ticks=true のとき、plot_left から外側へ短線を描く。
    // 色は grid.color を継承する(Chart.js 既定と同じ挙動: grid.color が gridline と tick の両方を制御)。
    // 対数軸では frame.minor_ticks(mantissa 2..9)にも同じ短線を描く。2b で minor
    // グリッド線をラベルなしで描いているのと対称に、tick 刻みも major/minor を
    // 揃えないと「グリッド線はあるのに対応する軸の刻みが無い」という見た目の
    // 不整合が生じるため(gridline と tick 刻みは 1:1 対応させる)。
    const TICK_LEN: f64 = 4.0;
    let ticks_cfg = &spec.y_axis.grid;
    if ticks_cfg.draw_ticks {
        let tick_color = if matches!(spec.x_positions, XPositions::Temporal { .. }) {
            ink
        } else {
            ticks_cfg.color.unwrap_or(ink)
        };
        for &t in frame.ticks.ticks.iter().chain(frame.minor_ticks.iter()) {
            let y = frame.ys.map(t);
            items.push(Prim::Line {
                x1: frame.plot_left - TICK_LEN,
                y1: y,
                x2: frame.plot_left,
                y2: y,
                stroke: tick_color,
                stroke_width: ticks_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }

    match &spec.x_positions {
        XPositions::Category => {
            // 4a. x カテゴリラベル（auto-skip は表示 tick に適用）。
            let n = spec.categories.len().max(1);
            let slot_w = (frame.plot_right - frame.plot_left) / n as f64;
            let step = categorical_tick_step(&spec.categories, slot_w, m, label_font);
            for (i, cat) in spec.categories.iter().enumerate() {
                if i % step != 0 {
                    continue;
                }
                // line は点と同じ配置(offset:false=edge-to-edge / offset:true=band 中心)で
                // grid/ラベルを点の真下に置く。bar/その他はバンド中心。mixed は band 中心。
                let x = if matches!(spec.kind, ChartKind::Line { .. }) {
                    line_x(spec, frame, i)
                } else {
                    category_center(frame, i, n)
                };
                if cat.is_empty() {
                    continue;
                }
                items.push(Prim::Text {
                    x,
                    y: frame.plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
                    size: label_font,
                    anchor: Anchor::Middle,
                    fill: ink,
                    content: cat.clone(),
                    rotate_deg: None,
                });
            }
        }
        XPositions::Temporal { unix_millis } => {
            // 4b. temporal x 軸。grid/tick は全 tick を描き、ラベルだけ決定的に重なり回避する。
            let min = *unix_millis.first().unwrap_or(&0);
            let max = *unix_millis.last().unwrap_or(&min);
            let x_grid = &spec.x_axis.grid;
            let grid_color = x_grid.color.unwrap_or(spec.theme.grid_color);
            let mut last_label_right = f64::NEG_INFINITY;
            for tick in &frame.temporal_ticks {
                let x = temporal_x(frame, min, max, tick.unix_millis);
                if x_grid.display {
                    items.push(Prim::Line {
                        x1: x,
                        y1: frame.plot_top,
                        x2: x,
                        y2: frame.plot_bottom,
                        stroke: grid_color,
                        stroke_width: x_grid.line_width,
                        dash: Vec::new(),
                    });
                }
                if x_grid.draw_ticks {
                    items.push(Prim::Line {
                        x1: x,
                        y1: frame.plot_bottom,
                        x2: x,
                        y2: frame.plot_bottom + TICK_LEN,
                        stroke: ink,
                        stroke_width: x_grid.line_width,
                        dash: Vec::new(),
                    });
                }
                let half_w = m.width(&tick.label, label_font as f32) as f64 / 2.0;
                let label_left = x - half_w;
                if label_left >= last_label_right + 4.0 {
                    items.push(Prim::Text {
                        x,
                        y: frame.plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
                        size: label_font,
                        anchor: Anchor::Middle,
                        fill: ink,
                        content: tick.label.clone(),
                        rotate_deg: None,
                    });
                    last_label_right = x + half_w;
                }
            }
        }
    }

    // 5. 凡例(Top/Bottom: 横並び)。
    if has_legend(spec) && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let entries: Vec<(String, Color)> = spec
            .series
            .iter()
            .map(|series| (series.name.clone(), series.fill_at(0)))
            .collect();
        let legend_title = legend_title(spec);
        let legend_height =
            legend_horizontal_band_height(&spec.legend_options, label_font, legend_title.is_some());
        let legend_cy = if spec.legend == LegendPos::Top {
            OUTER_PAD + title_band + legend_height / 2.0
        } else {
            spec.height - OUTER_PAD - legend_height / 2.0
        };
        draw_horizontal_legend(
            items,
            &entries,
            legend_title,
            spec.width,
            legend_cy,
            label_font,
            ink,
            m,
            &spec.legend_options,
        );
    }

    // 5b. 凡例(Left/Right: 縦並び)。
    if has_legend(spec) && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, Color)> = spec
            .series
            .iter()
            .map(|s| (s.name.clone(), s.fill_at(0)))
            .collect();
        let mut names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
        let legend_title = legend_title(spec);
        names.extend(legend_title.map(str::to_owned));
        let band_w = legend_band_width_vertical_styled(
            m,
            &names,
            legend_label_font_size(&spec.legend_options, label_font),
            &spec.legend_options,
        );
        let band_x = if spec.legend == LegendPos::Left {
            OUTER_PAD
        } else if matches!(spec.size_mode, SizeMode::PlotArea) {
            frame.plot_right + OUTER_PAD
        } else {
            spec.width - OUTER_PAD - band_w
        };
        draw_vertical_legend_styled(
            items,
            &entries,
            legend_title,
            band_x,
            frame.plot_top,
            frame.plot_bottom,
            ink,
            label_font,
            &spec.legend_options,
        );
    }

    // 6. Y 軸タイトル(回転テキスト)。プロット左端外側、キャンバス左端(OUTER_PAD)寄りに
    // -90deg で描く。Chart.js の core.scale.js は `_alignStartEnd(align, bottom, top)` を
    // Y 軸に使う: 回転タイトルは bottom-to-top で読むため "start"=下端、"end"=上端が読みの起点/終点。
    // 加えて anchor + -90deg の幾何:
    //   Anchor::Start + -90deg → アンカーから上方向へ伸びる → cy=plot_bottom と組み合わせる
    //   Anchor::End   + -90deg → アンカーから下方向へ伸びる → cy=plot_top    と組み合わせる
    // これで文字列は常にプロット領域内へ収まる。
    if let Some(title) = &spec.y_axis.title {
        let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
        let color = title.color.unwrap_or(ink);
        let cy_center = (frame.plot_top + frame.plot_bottom) / 2.0;
        let (cy, anchor) = match title.align {
            crate::ir::AxisTitleAlign::Start => (frame.plot_bottom, Anchor::Start),
            crate::ir::AxisTitleAlign::End => (frame.plot_top, Anchor::End),
            crate::ir::AxisTitleAlign::Center => (cy_center, Anchor::Middle),
        };
        let x = OUTER_PAD + font / 2.0;
        items.push(Prim::Text {
            x,
            y: cy,
            size: font,
            anchor,
            fill: color,
            content: title.text.clone(),
            rotate_deg: Some(-90.0),
        });
    }

    // 7. X 軸タイトル(水平テキスト)。X ラベル帯のさらに下に置く。
    // Chart.js の core.scale.js は X 軸で `_alignStartEnd(align, left, right)` を使い、
    // 左→右の自然な対応(Start=left, End=right)になる。Y 軸のような入れ替えは不要。
    if let Some(title) = &spec.x_axis.title {
        let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
        let color = title.color.unwrap_or(ink);
        let (cx, anchor) = match title.align {
            crate::ir::AxisTitleAlign::Start => (frame.plot_left, Anchor::Start),
            crate::ir::AxisTitleAlign::End => (frame.plot_right, Anchor::End),
            crate::ir::AxisTitleAlign::Center => {
                ((frame.plot_left + frame.plot_right) / 2.0, Anchor::Middle)
            }
        };
        let y = frame.plot_bottom + X_LABEL_BAND + font * 0.9;
        items.push(Prim::Text {
            x: cx,
            y,
            size: font,
            anchor,
            fill: color,
            content: title.text.clone(),
            rotate_deg: None,
        });
    }
}

const LEGEND_SWATCH_GAP: f64 = 4.0;
const LEGEND_BAND_SIDE_PAD: f64 = 16.0;
const LEGEND_ROW_GAP: f64 = LEGEND_ROW_H - 12.0;

pub fn legend_label_font_size(options: &LegendOptions, default_size: f64) -> f64 {
    options
        .labels_font_size
        .filter(|size| size.is_finite() && *size > 0.0)
        .map(|size| size.min(512.0))
        .unwrap_or(default_size.min(512.0))
}

pub fn legend_label_color(options: &LegendOptions, default_color: Color) -> Color {
    options.labels_color.unwrap_or(default_color)
}

fn legend_title_font_size(options: &LegendOptions, label_font_size: f64) -> f64 {
    options
        .title_font_size
        .filter(|size| size.is_finite() && *size > 0.0)
        .map(|size| size.min(512.0))
        .unwrap_or(label_font_size)
}

fn legend_box_width(options: &LegendOptions) -> f64 {
    options
        .labels_box_width
        .filter(|width| width.is_finite() && *width >= 0.0)
        .map(|width| width.min(10_000.0))
        .unwrap_or(12.0)
}

fn legend_box_height(options: &LegendOptions) -> f64 {
    options
        .labels_box_height
        .filter(|height| height.is_finite() && *height >= 0.0)
        .map(|height| height.min(10_000.0))
        .unwrap_or(12.0)
}

fn legend_labels_padding(options: &LegendOptions, default: f64) -> f64 {
    options
        .labels_padding
        .filter(|padding| padding.is_finite() && *padding >= 0.0)
        .map(|padding| padding.min(10_000.0))
        .unwrap_or(default)
}

fn legend_title_padding(options: &LegendOptions) -> crate::ir::LegendTitlePadding {
    let side = |value: f64| {
        if value.is_finite() && value >= 0.0 {
            value.min(10_000.0)
        } else {
            0.0
        }
    };
    crate::ir::LegendTitlePadding {
        top: side(options.title_padding.top),
        right: side(options.title_padding.right),
        bottom: side(options.title_padding.bottom),
        left: side(options.title_padding.left),
    }
}

fn legend_font_attrs(
    options: &LegendOptions,
    title: bool,
) -> (Option<String>, Option<String>, Option<String>) {
    if title {
        (
            options.title_font_family.clone(),
            options.title_font_weight.clone(),
            options.title_font_style.clone(),
        )
    } else {
        (
            options.labels_font_family.clone(),
            options.labels_font_weight.clone(),
            options.labels_font_style.clone(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn legend_text_prim(
    x: f64,
    y: f64,
    size: f64,
    anchor: Anchor,
    fill: Color,
    content: String,
    options: &LegendOptions,
    title: bool,
) -> Prim {
    let (font_family, font_weight, font_style) = legend_font_attrs(options, title);
    if font_family.is_some() || font_weight.is_some() || font_style.is_some() {
        Prim::StyledText(Box::new(StyledText {
            x,
            y,
            size,
            anchor,
            fill,
            content,
            rotate_deg: None,
            font_family,
            font_weight,
            font_style,
        }))
    } else {
        Prim::Text {
            x,
            y,
            size,
            anchor,
            fill,
            content,
            rotate_deg: None,
        }
    }
}

fn legend_marker(items: &mut Vec<Prim>, x: f64, y: f64, color: Color, options: &LegendOptions) {
    let width = legend_box_width(options);
    let height = legend_box_height(options);
    if !options.labels_use_point_style {
        items.push(Prim::Rect {
            x,
            y,
            w: width,
            h: height,
            fill: color,
        });
        return;
    }

    let cx = x + width / 2.0;
    let cy = y + height / 2.0;
    let stroke_width = (width.min(height) / 7.0).max(1.0);
    let line = |x1, y1, x2, y2| Prim::Line {
        x1,
        y1,
        x2,
        y2,
        stroke: color,
        stroke_width,
        dash: Vec::new(),
    };
    let polygon = |points: &[(f64, f64)]| {
        let mut data = String::new();
        for (index, (px, py)) in points.iter().enumerate() {
            if index == 0 {
                data.push_str(&format!("M{} {}", fmt_num(*px), fmt_num(*py)));
            } else {
                data.push_str(&format!(" L{} {}", fmt_num(*px), fmt_num(*py)));
            }
        }
        data.push_str(" Z");
        Prim::Path {
            d: data,
            fill: Some(color),
            stroke: None,
            stroke_width: 0.0,
        }
    };
    match options
        .labels_point_style
        .unwrap_or(LegendPointStyle::Circle)
    {
        LegendPointStyle::Circle => items.push(Prim::Circle {
            cx,
            cy,
            r: width.min(height) / 2.0,
            fill: color,
            stroke: color,
            stroke_width: 0.0,
        }),
        LegendPointStyle::Cross => {
            items.push(line(cx, y, cx, y + height));
            items.push(line(x, cy, x + width, cy));
        }
        LegendPointStyle::CrossRot => {
            items.push(line(x, y, x + width, y + height));
            items.push(line(x, y + height, x + width, y));
        }
        LegendPointStyle::Dash => {
            let dash_width = width * 0.6;
            items.push(line(cx - dash_width / 2.0, cy, cx + dash_width / 2.0, cy));
        }
        LegendPointStyle::Line => items.push(line(x, cy, x + width, cy)),
        LegendPointStyle::Rect | LegendPointStyle::RectRounded => items.push(Prim::Rect {
            x,
            y,
            w: width,
            h: height,
            fill: color,
        }),
        LegendPointStyle::RectRot => items.push(polygon(&[
            (cx, y),
            (x + width, cy),
            (cx, y + height),
            (x, cy),
        ])),
        LegendPointStyle::Triangle => items.push(polygon(&[
            (cx, y),
            (x + width, y + height),
            (x, y + height),
        ])),
        LegendPointStyle::Star => {
            let radius = width.min(height) / 2.0;
            let inner_radius = radius * 0.45;
            let points: Vec<(f64, f64)> = (0..10)
                .map(|index| {
                    let angle =
                        -std::f64::consts::FRAC_PI_2 + index as f64 * std::f64::consts::PI / 5.0;
                    let r = if index % 2 == 0 { radius } else { inner_radius };
                    (cx + angle.cos() * r, cy + angle.sin() * r)
                })
                .collect();
            items.push(polygon(&points));
        }
    }
}

/// Add one Chart.js dataset marker centered at `(cx, cy)` with radius `r`.
#[allow(clippy::too_many_arguments)]
pub fn dataset_point_marker(
    items: &mut Vec<Prim>,
    cx: f64,
    cy: f64,
    r: f64,
    fill: Color,
    stroke: Color,
    stroke_width: f64,
    style: Option<DatasetPointStyle>,
) {
    if !r.is_finite() || r <= 0.0 {
        return;
    }

    let style = style.unwrap_or(DatasetPointStyle::Circle);
    if style == DatasetPointStyle::Hidden {
        return;
    }
    let line = |x1, y1, x2, y2| Prim::Line {
        x1,
        y1,
        x2,
        y2,
        stroke,
        stroke_width: if stroke_width > 0.0 {
            stroke_width
        } else {
            1.0
        },
        dash: Vec::new(),
    };
    let polygon = |points: &[(f64, f64)]| {
        let mut d = String::new();
        for (index, (x, y)) in points.iter().enumerate() {
            if index == 0 {
                d.push_str(&format!("M {} {}", fmt_num(*x), fmt_num(*y)));
            } else {
                d.push_str(&format!(" L {} {}", fmt_num(*x), fmt_num(*y)));
            }
        }
        d.push_str(" Z");
        Prim::Path {
            d,
            fill: Some(fill),
            stroke: (stroke_width > 0.0).then_some(stroke),
            stroke_width,
        }
    };

    match style {
        DatasetPointStyle::Hidden => {}
        DatasetPointStyle::Circle => items.push(Prim::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
        }),
        DatasetPointStyle::Cross => {
            items.push(line(cx - r, cy, cx + r, cy));
            items.push(line(cx, cy - r, cx, cy + r));
        }
        DatasetPointStyle::CrossRot => {
            items.push(line(cx - r, cy - r, cx + r, cy + r));
            items.push(line(cx - r, cy + r, cx + r, cy - r));
        }
        DatasetPointStyle::Dash | DatasetPointStyle::Line => {
            let half_length = if style == DatasetPointStyle::Dash {
                r * 0.6
            } else {
                r
            };
            items.push(line(cx - half_length, cy, cx + half_length, cy));
        }
        DatasetPointStyle::Rect => items.push(polygon(&[
            (cx - r, cy - r),
            (cx + r, cy - r),
            (cx + r, cy + r),
            (cx - r, cy + r),
        ])),
        DatasetPointStyle::RectRounded => {
            let x0 = cx - r;
            let x1 = cx + r;
            let y0 = cy - r;
            let y1 = cy + r;
            let radius = r * 0.25;
            let curve = radius * 0.552_284_749_830_793_6;
            let d = format!(
                "M {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} Z",
                fmt_num(x0 + radius),
                fmt_num(y0),
                fmt_num(x1 - radius),
                fmt_num(y0),
                fmt_num(x1 - radius + curve),
                fmt_num(y0),
                fmt_num(x1),
                fmt_num(y0 + radius - curve),
                fmt_num(x1),
                fmt_num(y0 + radius),
                fmt_num(x1),
                fmt_num(y1 - radius),
                fmt_num(x1),
                fmt_num(y1 - radius + curve),
                fmt_num(x1 - radius + curve),
                fmt_num(y1),
                fmt_num(x1 - radius),
                fmt_num(y1),
                fmt_num(x0 + radius),
                fmt_num(y1),
                fmt_num(x0 + radius - curve),
                fmt_num(y1),
                fmt_num(x0),
                fmt_num(y1 - radius + curve),
                fmt_num(x0),
                fmt_num(y1 - radius),
                fmt_num(x0),
                fmt_num(y0 + radius),
                fmt_num(x0),
                fmt_num(y0 + radius - curve),
                fmt_num(x0 + radius - curve),
                fmt_num(y0),
                fmt_num(x0 + radius),
                fmt_num(y0),
            );
            items.push(Prim::Path {
                d,
                fill: Some(fill),
                stroke: (stroke_width > 0.0).then_some(stroke),
                stroke_width,
            });
        }
        DatasetPointStyle::RectRot => {
            items.push(polygon(&[
                (cx, cy - r),
                (cx + r, cy),
                (cx, cy + r),
                (cx - r, cy),
            ]));
        }
        DatasetPointStyle::Star => {
            let inner_radius = r * 0.45;
            let points: Vec<(f64, f64)> = (0..10)
                .map(|index| {
                    let angle =
                        -std::f64::consts::FRAC_PI_2 + index as f64 * std::f64::consts::PI / 5.0;
                    let radius = if index % 2 == 0 { r } else { inner_radius };
                    (cx + angle.cos() * radius, cy + angle.sin() * radius)
                })
                .collect();
            items.push(polygon(&points));
        }
        DatasetPointStyle::Triangle => {
            items.push(polygon(&[(cx, cy - r), (cx + r, cy + r), (cx - r, cy + r)]));
        }
    }
}

pub fn legend_entry_width_styled(
    m: &TextMeasurer,
    name: &str,
    font_size: f64,
    options: &LegendOptions,
) -> f64 {
    legend_box_width(options)
        + LEGEND_SWATCH_GAP
        + m.width(name, font_size.min(512.0) as f32) as f64
}

pub fn legend_band_width_vertical_styled(
    m: &TextMeasurer,
    names: &[String],
    font_size: f64,
    options: &LegendOptions,
) -> f64 {
    let measure_font_size = if options.title_display {
        font_size.max(legend_title_font_size(options, font_size))
    } else {
        font_size
    };
    let max_w = names
        .iter()
        .map(|name| m.width(name, measure_font_size.min(512.0) as f32) as f64)
        .fold(0.0, f64::max);
    let padding = legend_title_padding(options);
    legend_box_width(options)
        + LEGEND_SWATCH_GAP
        + max_w
        + LEGEND_BAND_SIDE_PAD
        + padding.left
        + padding.right
}

/// 凡例用の上側/下側帯高。既存の 26px 帯を下限にし、タイトルと指定 font/box に応じて広げる。
pub fn legend_horizontal_band_height(
    options: &LegendOptions,
    default_font_size: f64,
    has_title: bool,
) -> f64 {
    let font_size = legend_label_font_size(options, default_font_size);
    let labels_height = LEGEND_BAND
        .max(font_size + 8.0)
        .max(legend_box_height(options) + 8.0);
    if has_title {
        let title_font = legend_title_font_size(options, font_size);
        let padding = legend_title_padding(options);
        labels_height + title_font + padding.top + padding.bottom + 4.0
    } else {
        labels_height
    }
}

fn legend_vertical_row_height(options: &LegendOptions, default_font_size: f64) -> f64 {
    let font_size = legend_label_font_size(options, default_font_size);
    let gap = legend_labels_padding(options, LEGEND_ROW_GAP);
    let content_height = font_size.max(legend_box_height(options));
    if options.labels_padding.is_some() {
        content_height + gap
    } else {
        LEGEND_ROW_H.max(content_height + gap)
    }
}

fn legend_vertical_title_height(options: &LegendOptions, default_font_size: f64) -> f64 {
    let font_size =
        legend_title_font_size(options, legend_label_font_size(options, default_font_size));
    let padding = legend_title_padding(options);
    (font_size + LEGEND_ROW_GAP + padding.top + padding.bottom).max(LEGEND_ROW_H)
}

/// Top/Bottom 凡例を横並びに描く。
#[allow(clippy::too_many_arguments)]
pub fn draw_horizontal_legend(
    items: &mut Vec<Prim>,
    entries: &[(String, Color)],
    title: Option<&str>,
    canvas_width: f64,
    band_center_y: f64,
    default_font_size: f64,
    default_ink: Color,
    m: &TextMeasurer,
    options: &LegendOptions,
) {
    let font_size = legend_label_font_size(options, default_font_size);
    let label_ink = legend_label_color(options, default_ink);
    let box_width = legend_box_width(options);
    let box_height = legend_box_height(options);
    let gap = legend_labels_padding(options, LEGEND_BAND_SIDE_PAD);
    let widths: Vec<f64> = entries
        .iter()
        .map(|(name, _)| legend_entry_width_styled(m, name, font_size, options))
        .collect();
    let total_width = widths.iter().sum::<f64>() + gap * entries.len().saturating_sub(1) as f64;
    let start_x = match options.align {
        LegendAlign::Start => 0.0,
        LegendAlign::Center => (canvas_width - total_width) / 2.0,
        LegendAlign::End => canvas_width - total_width,
    };

    let band_height = legend_horizontal_band_height(options, default_font_size, title.is_some());
    let title_height = if let Some(title) = title {
        let title_size = legend_title_font_size(options, font_size);
        let padding = legend_title_padding(options);
        let title_block_height = title_size + padding.top + padding.bottom + 4.0;
        let band_top = band_center_y - band_height / 2.0;
        let title_y = band_top + padding.top + title_size / 2.0 + title_size * TEXT_BASELINE_RATIO;
        let title_ink = options.title_color.unwrap_or(label_ink);
        let title_x = match options.align {
            LegendAlign::Start => padding.left,
            LegendAlign::Center => canvas_width / 2.0,
            LegendAlign::End => canvas_width - padding.right,
        };
        let anchor = match options.align {
            LegendAlign::Start => Anchor::Start,
            LegendAlign::Center => Anchor::Middle,
            LegendAlign::End => Anchor::End,
        };
        items.push(legend_text_prim(
            title_x,
            title_y,
            title_size,
            anchor,
            title_ink,
            title.to_string(),
            options,
            true,
        ));
        title_block_height
    } else {
        0.0
    };
    let label_band_center =
        band_center_y - band_height / 2.0 + title_height + (band_height - title_height) / 2.0;

    let mut indices: Vec<usize> = (0..entries.len()).collect();
    if options.reverse {
        indices.reverse();
    }
    let mut cursor_x = start_x;
    for index in indices {
        let (name, color) = &entries[index];
        legend_marker(
            items,
            cursor_x,
            label_band_center - box_height / 2.0,
            *color,
            options,
        );
        items.push(legend_text_prim(
            cursor_x + box_width + LEGEND_SWATCH_GAP,
            label_band_center + font_size * TEXT_BASELINE_RATIO,
            font_size,
            Anchor::Start,
            label_ink,
            name.clone(),
            options,
            false,
        ));
        cursor_x += widths[index] + gap;
    }
}

/// 縦置き凡例(Left/Right)の帯幅。空名も幅計算に含める。
pub fn legend_band_width_vertical(m: &TextMeasurer, names: &[String], font_size: f64) -> f64 {
    legend_band_width_vertical_styled(m, names, font_size, &LegendOptions::default())
}

/// 縦置き凡例。既存の利用側に対する既定スタイルの互換 wrapper。
#[allow(clippy::too_many_arguments)]
pub fn draw_vertical_legend(
    items: &mut Vec<Prim>,
    entries: &[(String, Color)],
    title: Option<&str>,
    band_x: f64,
    plot_top: f64,
    plot_bottom: f64,
    ink: Color,
    font_size: f64,
) {
    draw_vertical_legend_styled(
        items,
        entries,
        title,
        band_x,
        plot_top,
        plot_bottom,
        ink,
        font_size,
        &LegendOptions::default(),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_vertical_legend_styled(
    items: &mut Vec<Prim>,
    entries: &[(String, Color)],
    title: Option<&str>,
    band_x: f64,
    plot_top: f64,
    plot_bottom: f64,
    default_ink: Color,
    default_font_size: f64,
    options: &LegendOptions,
) {
    let font_size = legend_label_font_size(options, default_font_size);
    let ink = legend_label_color(options, default_ink);
    let box_height = legend_box_height(options);
    let row_height = legend_vertical_row_height(options, default_font_size);
    let padding = legend_title_padding(options);
    let title_size = legend_title_font_size(options, font_size);
    let title_height = title.map_or(0.0, |_| legend_vertical_title_height(options, font_size));
    let group_h = entries.len() as f64 * row_height + title_height;
    let start_y = match options.align {
        LegendAlign::Start => plot_top,
        LegendAlign::Center => (plot_top + plot_bottom - group_h) / 2.0,
        LegendAlign::End => plot_bottom - group_h,
    };
    if let Some(title) = title {
        let title_y = start_y + title_height / 2.0 + title_size * TEXT_BASELINE_RATIO;
        let title_ink = options.title_color.unwrap_or(ink);
        items.push(legend_text_prim(
            band_x + padding.left,
            title_y,
            title_size,
            Anchor::Start,
            title_ink,
            title.to_string(),
            options,
            true,
        ));
    }
    let mut indices: Vec<usize> = (0..entries.len()).collect();
    if options.reverse {
        indices.reverse();
    }
    for (row, index) in indices.into_iter().enumerate() {
        let (name, color) = &entries[index];
        let row_top = start_y + title_height + row as f64 * row_height;
        let row_center = row_top + row_height / 2.0;
        legend_marker(
            items,
            band_x,
            row_center - box_height / 2.0,
            *color,
            options,
        );
        items.push(legend_text_prim(
            band_x + legend_box_width(options) + LEGEND_SWATCH_GAP,
            row_center + font_size * TEXT_BASELINE_RATIO,
            font_size,
            Anchor::Start,
            ink,
            name.clone(),
            options,
            false,
        ));
    }
}

/// 凡例 1 エントリの占有幅。既定では swatch(12) + gap(4) + label + trailing(16)。
pub fn legend_entry_width(m: &TextMeasurer, name: &str, font_size: f64) -> f64 {
    legend_entry_width_styled(m, name, font_size, &LegendOptions::default()) + LEGEND_BAND_SIDE_PAD
}

/// 値ラベルの Prim::Text を生成する(フォント=size、内容=fmt_num(v)/fmt_num_log(v))。
/// 全チャート種でデータラベル生成を一元化する。x/y/anchor/fill/size は呼び出し側が決める。
/// `is_log` は値軸が対数スケールかどうか(対数軸を持ちうるのは Bar/Line のみ。
/// pie/mixed/radial 等、対数軸を取りえない呼び出し元は常に `false` を渡す)。
/// `false` なら従来通り `fmt_num`(小数2桁丸め)、`true` なら `fmt_num_log`
/// (有効数字ベース、広レンジ対応)を使う — 対数軸では `fmt_num` の丸めにより
/// 0.0003 のような小さい実値のラベルが "0" に潰れてしまうため(実機バグ、
/// PR #144 の自動レビューで指摘)。
pub fn value_label(
    x: f64,
    y: f64,
    size: f64,
    anchor: Anchor,
    fill: Color,
    v: f64,
    is_log: bool,
) -> Prim {
    Prim::Text {
        x,
        y,
        size,
        anchor,
        fill,
        content: if is_log {
            crate::num::fmt_num_log(v)
        } else {
            fmt_num(v)
        },
        rotate_deg: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::ir::{
        AxisBorder, AxisGrid, AxisSpec, AxisTitle, AxisTitleAlign, ChartKind, ChartSpec, LegendPos,
        LineInterpolation, Point, ScaleKind, Series, SeriesType, SizeMode, XPositions,
    };
    use crate::text::TextMeasurer;

    fn make_bar_spec(n: usize, width: f64) -> ChartSpec {
        let palette = crate::palette::PALETTE.to_vec();
        ChartSpec {
            kind: ChartKind::Bar {
                horizontal: false,
                placement_stacked: false,
                value_stacked: false,
            },
            categories: (0..n).map(|i| format!("Cat{i:04}")).collect(),
            x_positions: XPositions::Category,
            series: vec![Series {
                name: String::new(),
                values: vec![1.0; n],
                points: Vec::<Point>::new(),
                fill: vec![palette[0]],
                stroke: vec![],
                stroke_width: 1.0,
                area: false,
                area_fill: None,
                interpolation: LineInterpolation::Linear,
                span_gaps: false,
                step_mode: None,
                line_style: None,
                stack: None,
                bar_geometry: None,
                series_type: SeriesType::Bar,
                point_radius: None,
                box_points: vec![],
                tree: vec![],
                links: vec![],
            }],
            x_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: true,
                offset: false,
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
                scale_kind: ScaleKind::Linear,
                ticks: crate::ir::AxisTickOptions::default(),
            },
            y_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: true,
                offset: false,
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
                scale_kind: ScaleKind::Linear,
                ticks: crate::ir::AxisTickOptions::default(),
            },
            legend: LegendPos::None,
            legend_options: crate::ir::LegendOptions::default(),
            legend_title: None,
            title: None,
            width,
            height: 400.0,
            size_mode: SizeMode::Canvas,
            data_labels: false,
            theme: crate::ir::Theme::default(),
            decimation: crate::ir::Decimation::default(),
            radial_axis: None,
        }
    }

    #[test]
    fn apply_hard_axis_bounds_does_not_apply_chartjs_default_limit_to_other_frontends() {
        let mut spec = make_bar_spec(1, 100.0);
        spec.x_axis.ticks = crate::ir::AxisTickOptions::default();
        let ticks = NiceTicks {
            min: 0.0,
            max: 11.0,
            step: 1.0,
            ticks: (0..=11).map(f64::from).collect(),
        };

        let bounded = apply_hard_axis_bounds(ticks, &spec.x_axis);

        assert_eq!(bounded.ticks.len(), 12);
    }

    fn temporal_spec(unix_millis: Vec<i64>) -> ChartSpec {
        let mut spec = make_bar_spec(unix_millis.len(), 720.0);
        spec.kind = ChartKind::Line {
            stacked: false,
            stacked_missing_values_are_gaps: false,
        };
        spec.categories = unix_millis
            .iter()
            .map(|millis| format!("source-{millis}"))
            .collect();
        spec.x_positions = XPositions::Temporal { unix_millis };
        spec.series[0].name = "regressions".into();
        spec.series[0].series_type = SeriesType::Line;
        spec.series[0].stroke = spec.series[0].fill.clone();
        spec.size_mode = SizeMode::PlotArea;
        spec
    }

    fn temporal_dogfood_spec() -> ChartSpec {
        let day = 86_400_000;
        let mut spec = temporal_spec(vec![0, 2 * day, 4 * day]);
        spec.height = 320.0;
        spec.legend = LegendPos::Right;
        spec.legend_title = Some("metric".into());
        spec.x_axis.title = Some(AxisTitle {
            text: "date".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        spec.y_axis.title = Some(AxisTitle {
            text: "subtests".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        let grid = Color {
            r: 224,
            g: 224,
            b: 224,
            a: 0.15,
        };
        spec.x_axis.grid.color = Some(grid);
        spec.x_axis.grid.draw_ticks = true;
        spec.y_axis.grid.color = Some(grid);
        spec.y_axis.grid.draw_ticks = true;
        spec
    }

    #[test]
    fn temporal_x_distances_follow_elapsed_time() {
        let spec = temporal_spec(vec![0, 86_400_000, 3 * 86_400_000]);
        let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let x0 = line_x(&spec, &frame, 0);
        let x1 = line_x(&spec, &frame, 1);
        let x2 = line_x(&spec, &frame, 2);
        assert!(((x1 - x0) / (x2 - x0) - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn temporal_x_handles_full_i64_domain() {
        let spec = temporal_spec(vec![i64::MIN, 0, i64::MAX]);
        let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let left = line_x(&spec, &frame, 0);
        let middle = line_x(&spec, &frame, 1);
        let right = line_x(&spec, &frame, 2);

        assert_eq!(left, frame.plot_left);
        assert_eq!(right, frame.plot_right);
        assert!(((middle - left) / (right - left) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn plot_area_mode_preserves_requested_plot_size() {
        let spec = temporal_dogfood_spec();
        let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        assert_eq!(frame.plot_right - frame.plot_left, 720.0);
        assert_eq!(frame.plot_bottom - frame.plot_top, 320.0);
        assert!(frame.scene_width > 720.0);
        assert!(frame.scene_height > 320.0);
    }

    #[test]
    fn plot_area_contains_long_start_and_end_x_axis_titles() {
        const TITLE: &str = "a long horizontal axis title that exceeds the plot width";
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();

        for (align, expected_anchor) in [
            (AxisTitleAlign::Start, Anchor::Start),
            (AxisTitleAlign::End, Anchor::End),
        ] {
            let mut spec = temporal_spec(vec![0, 1_000]);
            spec.width = 24.0;
            spec.x_axis.title = Some(AxisTitle {
                text: TITLE.into(),
                color: None,
                font_size: None,
                align,
            });
            let frame = compute(&spec, &m);
            let mut items = Vec::new();
            draw_frame(&mut items, &spec, &frame, &m);
            let (x, size, anchor) = items
                .iter()
                .find_map(|item| match item {
                    Prim::Text {
                        x,
                        size,
                        anchor,
                        content,
                        rotate_deg: None,
                        ..
                    } if content == TITLE => Some((*x, *size, *anchor)),
                    _ => None,
                })
                .expect("x-axis title");
            assert_eq!(anchor, expected_anchor);
            let width = m.width(TITLE, size as f32) as f64;
            let (left, right) = if anchor == Anchor::Start {
                (x, x + width)
            } else {
                assert_eq!(anchor, Anchor::End);
                (x - width, x)
            };
            assert!(left >= 0.0, "{align:?}: left={left}");
            assert!(
                right <= frame.scene_width,
                "{align:?}: right={right}, scene={}",
                frame.scene_width
            );
            assert_eq!(frame.plot_right - frame.plot_left, spec.width);
            assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
        }
    }

    #[test]
    fn plot_area_contains_long_start_and_end_rotated_y_axis_titles() {
        const TITLE: &str = "a long rotated axis title that exceeds the plot height";
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();

        for (align, expected_anchor) in [
            (AxisTitleAlign::Start, Anchor::Start),
            (AxisTitleAlign::End, Anchor::End),
        ] {
            let mut spec = temporal_spec(vec![0, 1_000]);
            spec.height = 24.0;
            spec.y_axis.title = Some(AxisTitle {
                text: TITLE.into(),
                color: None,
                font_size: None,
                align,
            });
            let frame = compute(&spec, &m);
            let mut items = Vec::new();
            draw_frame(&mut items, &spec, &frame, &m);
            let (y, size, anchor) = items
                .iter()
                .find_map(|item| match item {
                    Prim::Text {
                        y,
                        size,
                        anchor,
                        content,
                        rotate_deg: Some(-90.0),
                        ..
                    } if content == TITLE => Some((*y, *size, *anchor)),
                    _ => None,
                })
                .expect("rotated y-axis title");
            assert_eq!(anchor, expected_anchor);
            let height = m.width(TITLE, size as f32) as f64;
            let (top, bottom) = if anchor == Anchor::Start {
                (y - height, y)
            } else {
                assert_eq!(anchor, Anchor::End);
                (y, y + height)
            };
            assert!(top >= 0.0, "{align:?}: top={top}");
            assert!(
                bottom <= frame.scene_height,
                "{align:?}: bottom={bottom}, scene={}",
                frame.scene_height
            );
            assert_eq!(frame.plot_right - frame.plot_left, spec.width);
            assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
        }
    }

    #[test]
    fn plot_area_scene_contains_tall_right_legend() {
        let mut spec = temporal_spec(vec![0, 1]);
        spec.height = 18.0;
        spec.legend = LegendPos::Right;
        spec.legend_title = Some("metric".into());
        let template = spec.series[0].clone();
        spec.series = (0..6)
            .map(|i| {
                let mut series = template.clone();
                series.name = format!("series-{i}");
                series
            })
            .collect();

        let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let group_h = (spec.series.len() + 1) as f64 * LEGEND_ROW_H;
        let start_y = (frame.plot_top + frame.plot_bottom - group_h) / 2.0;
        assert!(start_y >= 0.0);
        assert!(start_y + group_h <= frame.scene_height);
        assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
    }

    #[test]
    fn plot_area_scene_contains_last_temporal_tick_label() {
        let spec = temporal_spec(vec![100, 900]);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let (x, label) = items
            .iter()
            .find_map(|item| match item {
                Prim::Text { x, content, .. } if content == ".900" => Some((*x, content)),
                _ => None,
            })
            .expect("last temporal tick label");
        let right = x + m.width(label, spec.theme.font_size as f32) as f64 / 2.0;
        assert!(
            right <= frame.scene_width,
            "label right {right} exceeds scene width {}",
            frame.scene_width
        );
    }

    #[test]
    fn plot_area_scene_contains_wide_first_temporal_tick_label() {
        let mut spec = temporal_spec(vec![1_788_220_800_000, 1_819_756_800_000]);
        spec.theme.font_size = 32.0;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let first = frame.temporal_ticks.first().expect("first temporal tick");
        assert_eq!(first.label, "September");

        let x = temporal_x(
            &frame,
            1_788_220_800_000,
            1_819_756_800_000,
            first.unix_millis,
        );
        let left = x - m.width(&first.label, spec.theme.font_size as f32) as f64 / 2.0;
        assert!(left >= 0.0, "label left {left} is outside the scene");
        assert_eq!(frame.plot_right - frame.plot_left, spec.width);
        assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
    }

    #[test]
    fn plot_area_chart_title_is_centered_over_plot() {
        let mut spec = temporal_dogfood_spec();
        spec.title = Some("plot title".into());
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let title_x = items.iter().rev().find_map(|item| match item {
            Prim::Text { x, content, .. } if content == "plot title" => Some(*x),
            _ => None,
        });
        assert_eq!(title_x, Some((frame.plot_left + frame.plot_right) / 2.0));
    }

    #[test]
    fn singleton_temporal_domain_maps_to_plot_center() {
        let spec = temporal_spec(vec![42]);
        let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        assert_eq!(
            line_x(&spec, &frame, 0),
            (frame.plot_left + frame.plot_right) / 2.0
        );
    }

    #[test]
    fn temporal_frame_draws_ticks_titles_and_titled_right_legend() {
        let spec = temporal_dogfood_spec();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let vertical_grids = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        stroke,
                        ..
                    } if (*x1 - *x2).abs() < 1e-9
                        && (*y1 - frame.plot_top).abs() < 1e-9
                        && (*y2 - frame.plot_bottom).abs() < 1e-9
                        && (stroke.a - 0.15).abs() < f32::EPSILON
                )
            })
            .count();
        assert_eq!(vertical_grids, frame.temporal_ticks.len());
        let bottom_ticks = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        stroke,
                        ..
                    } if (*x1 - *x2).abs() < 1e-9
                        && (*y1 - frame.plot_bottom).abs() < 1e-9
                        && (*y2 - frame.plot_bottom - 4.0).abs() < 1e-9
                        && *stroke == spec.theme.text_color
                )
            })
            .count();
        assert_eq!(bottom_ticks, frame.temporal_ticks.len());

        let labels: Vec<&str> = items
            .iter()
            .filter_map(|item| match item {
                Prim::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        for tick in &frame.temporal_ticks {
            assert!(labels.contains(&tick.label.as_str()));
        }
        assert!(labels.contains(&"1970"));
        assert!(
            spec.categories
                .iter()
                .all(|source_label| !labels.contains(&source_label.as_str()))
        );
        assert!(labels.contains(&"date"));
        assert!(labels.contains(&"subtests"));
        assert!(labels.contains(&"metric"));

        let legend_title_y = items.iter().find_map(|item| match item {
            Prim::Text { y, content, .. } if content == "metric" => Some(*y),
            _ => None,
        });
        let legend_entry_y = items.iter().find_map(|item| match item {
            Prim::Text { y, content, .. } if content == "regressions" => Some(*y),
            _ => None,
        });
        assert!(legend_title_y.unwrap() < legend_entry_y.unwrap());
    }

    #[test]
    fn categorical_canvas_ignores_legend_title_without_changing_scene() {
        let mut baseline = make_bar_spec(3, 600.0);
        baseline.series[0].name = "series".into();
        baseline.legend = LegendPos::Right;
        let mut titled = baseline.clone();
        titled.legend_title = Some("unsupported title".into());

        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let baseline_scene = crate::layout::build_scene(&baseline, &m);
        let titled_scene = crate::layout::build_scene(&titled, &m);
        assert_eq!(titled_scene, baseline_scene);
    }

    #[test]
    fn categorical_canvas_title_does_not_activate_unnamed_legend() {
        let mut baseline = make_bar_spec(3, 600.0);
        baseline.series[0].name.clear();
        baseline.legend = LegendPos::Right;
        let mut titled = baseline.clone();
        titled.legend_title = Some("unsupported title".into());

        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let titled_frame = compute(&titled, &m);
        let baseline_frame = compute(&baseline, &m);
        assert_eq!(titled_frame.scene_width, baseline_frame.scene_width);
        assert_eq!(titled_frame.scene_height, baseline_frame.scene_height);
        assert_eq!(titled_frame.plot_left, baseline_frame.plot_left);
        assert_eq!(titled_frame.plot_right, baseline_frame.plot_right);
        assert_eq!(titled_frame.plot_top, baseline_frame.plot_top);
        assert_eq!(titled_frame.plot_bottom, baseline_frame.plot_bottom);
        assert_eq!(titled_frame.ticks, baseline_frame.ticks);
        assert_eq!(titled_frame.temporal_ticks, baseline_frame.temporal_ticks);
        assert_eq!(
            titled_frame.ys.map(titled_frame.ticks.min),
            baseline_frame.ys.map(baseline_frame.ticks.min)
        );
        assert_eq!(
            titled_frame.ys.map(titled_frame.ticks.max),
            baseline_frame.ys.map(baseline_frame.ticks.max)
        );
        let titled_midpoint =
            titled_frame.ticks.min + (titled_frame.ticks.max - titled_frame.ticks.min) / 2.0;
        let baseline_midpoint =
            baseline_frame.ticks.min + (baseline_frame.ticks.max - baseline_frame.ticks.min) / 2.0;
        assert_eq!(
            titled_frame.ys.map(titled_midpoint),
            baseline_frame.ys.map(baseline_midpoint)
        );
        assert_eq!(
            crate::layout::build_scene(&titled, &m),
            crate::layout::build_scene(&baseline, &m)
        );
    }

    #[test]
    fn label_autoskip_fires_for_dense_categories() {
        let n = 100;
        let spec = make_bar_spec(n, 600.0);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        // title=None・legend=None なので anchor=Middle は x カテゴリラベルのみ。
        let x_label_count = items
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    Prim::Text {
                        anchor: Anchor::Middle,
                        ..
                    }
                )
            })
            .count();
        assert!(
            x_label_count < n,
            "dense spec (n={n}, width=600) でラベルが間引かれるべき: 実際 {x_label_count} 個"
        );
    }

    #[test]
    fn label_autoskip_no_panic_on_minimal_width() {
        // plot_left >= plot_right になりうる極小 width でパニックしないことを確認。
        let spec = make_bar_spec(10, 1.0);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
    }

    #[test]
    fn value_domain_sums_stacked_line_series_independently_by_sign() {
        let mut spec = make_bar_spec(2, 720.0);
        spec.kind = ChartKind::Line {
            stacked: true,
            stacked_missing_values_are_gaps: false,
        };
        spec.series = vec![
            Series {
                name: "a".to_string(),
                values: vec![10.0, 20.0],
                points: Vec::<Point>::new(),
                fill: vec![crate::palette::PALETTE[0]],
                stroke: vec![crate::palette::PALETTE[0]],
                stroke_width: 2.0,
                area: true,
                area_fill: None,
                interpolation: LineInterpolation::Linear,
                span_gaps: false,
                step_mode: None,
                line_style: None,
                stack: None,
                bar_geometry: None,
                series_type: SeriesType::Line,
                point_radius: None,
                box_points: vec![],
                tree: vec![],
                links: vec![],
            },
            Series {
                name: "b".to_string(),
                values: vec![5.0, -8.0],
                points: Vec::<Point>::new(),
                fill: vec![crate::palette::PALETTE[1]],
                stroke: vec![crate::palette::PALETTE[1]],
                stroke_width: 2.0,
                area: true,
                area_fill: None,
                interpolation: LineInterpolation::Linear,
                span_gaps: false,
                step_mode: None,
                line_style: None,
                stack: None,
                bar_geometry: None,
                series_type: SeriesType::Line,
                point_radius: None,
                box_points: vec![],
                tree: vec![],
                links: vec![],
            },
            Series {
                name: "c".to_string(),
                values: vec![8.0, -3.0],
                points: Vec::<Point>::new(),
                fill: vec![crate::palette::PALETTE[2]],
                stroke: vec![crate::palette::PALETTE[2]],
                stroke_width: 2.0,
                area: true,
                area_fill: None,
                interpolation: LineInterpolation::Linear,
                span_gaps: false,
                step_mode: None,
                line_style: None,
                stack: None,
                bar_geometry: None,
                series_type: SeriesType::Line,
                point_radius: None,
                box_points: vec![],
                tree: vec![],
                links: vec![],
            },
        ];
        let (lo, hi) = value_domain(&spec, &spec.y_axis);
        // cat0: 10+5+8=23(正のみ)。個別値の最大 20 ではなく積み上げ和が上限になる
        // ことを検証(非 stacked パスならここが 20 になってしまう)。
        // cat1: 20 が正、-8 と -3 が負 -> 負側も個別和ではなくサム(-11)になる
        // ことを検証(個別値の最小は -8 だが、負サムの合計 -11 が下限になるべき)。
        assert_eq!((lo, hi), (-11.0, 23.0));
    }

    #[test]
    fn value_domain_sums_stacked_lines_per_stack_id_and_sign() {
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"line","data":{"labels":["A","B"],"datasets":[
              {"stack":"warm","data":[2,-3]},
              {"stack":"warm","data":[4,-5]},
              {"stack":"cool","data":[10,-1]},
              {"stack":"cool","data":[20,-2]}
            ]}}"#,
            false,
        )
        .unwrap();
        spec.kind = ChartKind::Line {
            stacked: true,
            stacked_missing_values_are_gaps: false,
        };

        assert_eq!(value_domain(&spec, &spec.y_axis), (-8.0, 30.0));
    }

    #[test]
    fn value_domain_sums_stacked_bars_per_stack_id_and_sign() {
        let spec = crate::frontend::chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[
              {"stack":"warm","data":[2,-3]},
              {"stack":"warm","data":[4,-5]},
              {"stack":"cool","data":[10,-1]},
              {"stack":"cool","data":[20,-2]}
            ]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}}"#,
            false,
        )
        .unwrap();

        assert_eq!(value_domain(&spec, &spec.y_axis), (-8.0, 30.0));
    }

    #[test]
    fn log_value_domain_sums_stacked_bars_per_stack_id() {
        let spec = crate::frontend::chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"stack":"small","data":[10]},
              {"stack":"small","data":[20]},
              {"stack":"large","data":[100]},
              {"stack":"large","data":[200]}
            ]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true,"type":"logarithmic","beginAtZero":false}}}}"#,
            false,
        )
        .unwrap();

        assert_eq!(value_domain(&spec, &spec.y_axis), (10.0, 300.0));
    }

    #[test]
    fn log_value_domain_includes_positive_stacked_line_totals_per_stack_id() {
        let spec = crate::frontend::chartjs::parse(
            r#"{"type":"line","data":{"labels":["A"],"datasets":[
              {"stack":"small","data":[10]},
              {"stack":"small","data":[20]},
              {"stack":"large","data":[100]},
              {"stack":"large","data":[200]}
            ]},"options":{"scales":{"y":{"stacked":true,"type":"logarithmic","beginAtZero":false}}}}"#,
            false,
        )
        .unwrap();

        assert_eq!(value_domain(&spec, &spec.y_axis), (10.0, 300.0));
    }

    #[test]
    fn value_domain_suggested_min_expands_below_data() {
        // suggestedMin がデータより小さい場合 → ドメインが広がる。
        // データは 1.0、begin_at_zero=true なので data_min は 0.0 に引き上げられる。
        // suggested_min=-20 はその 0.0 より小さいのでドメインが -20 まで広がる。
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.suggested_min = Some(-20.0);
        let (min, _max) = value_domain(&spec, &spec.y_axis);
        assert!(
            min <= -20.0,
            "suggested_min=-20 はドメインを下方向に広げるべき: 実際 min={min}"
        );
    }

    #[test]
    fn value_domain_suggested_min_noop_when_data_lower() {
        // suggestedMin がデータより大きい場合 → no-op（データが優先）。
        // データは 1.0、begin_at_zero=true なので domain_min=0.0。
        // suggested_min=50 は domain_min(0.0) より大きいが、データ側が優先されるので無視。
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.suggested_min = Some(50.0);
        let (min, _max) = value_domain(&spec, &spec.y_axis);
        assert!(
            min <= 0.0,
            "suggested_min=50 はデータの下端(0.0)を縮小してはいけない: 実際 min={min}"
        );
    }

    #[test]
    fn value_domain_hard_min_max_override_data_suggestions_and_begin_at_zero() {
        let mut spec = make_bar_spec(2, 600.0);
        spec.series[0].values = vec![10.0, 90.0];
        spec.y_axis.min = Some(20.0);
        spec.y_axis.max = Some(80.0);
        spec.y_axis.suggested_min = Some(-50.0);
        spec.y_axis.suggested_max = Some(150.0);
        // make_bar_spec の既定 begin_at_zero=true も hard bound に負ける。

        assert_eq!(value_domain(&spec, &spec.y_axis), (20.0, 80.0));
    }

    #[test]
    fn compute_preserves_hard_min_max_after_nice_tick_rounding() {
        let mut spec = make_bar_spec(2, 600.0);
        spec.series[0].values = vec![20.0, 80.0];
        spec.y_axis.begin_at_zero = false;
        spec.y_axis.min = Some(13.0);
        spec.y_axis.max = Some(87.0);
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();

        let frame = compute(&spec, &measurer);

        assert_eq!((frame.ticks.min, frame.ticks.max), (13.0, 87.0));
    }

    #[test]
    fn log_value_domain_hard_min_max_override_data_and_suggestions() {
        let mut spec = make_bar_spec(2, 600.0);
        spec.series[0].values = vec![10.0, 1000.0];
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;
        spec.y_axis.min = Some(20.0);
        spec.y_axis.max = Some(800.0);
        spec.y_axis.suggested_min = Some(1.0);
        spec.y_axis.suggested_max = Some(10_000.0);

        assert_eq!(value_domain(&spec, &spec.y_axis), (20.0, 800.0));

        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &measurer);
        assert_eq!((frame.ticks.min, frame.ticks.max), (20.0, 800.0));
        assert_eq!(frame.ys.map(20.0), frame.plot_bottom);
        assert_eq!(frame.ys.map(800.0), frame.plot_top);
    }

    #[test]
    fn log_value_domain_hard_max_wins_over_conflicting_suggestion_without_positive_data() {
        let mut spec = make_bar_spec(2, 600.0);
        spec.series[0].values = vec![0.0, -5.0];
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.max = Some(10.0);
        spec.y_axis.suggested_min = Some(20.0);

        assert_eq!(value_domain(&spec, &spec.y_axis), (1.0, 10.0));
    }

    #[test]
    fn log_value_domain_uses_min_positive_and_max() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        // beginAtZero:false を明示: このテストの主眼は「素の min_positive/max を
        // そのまま使う」ことの検証であり、beginAtZero の decade floor 丸めは
        // 別テストで個別に検証している。
        spec.y_axis.begin_at_zero = false;
        spec.series[0].values = vec![5.0, 50.0, 500.0];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!(min, 5.0);
        assert_eq!(max, 500.0);
    }

    #[test]
    fn log_value_domain_includes_positive_stacked_bar_totals() {
        let mut spec = make_bar_spec(2, 600.0);
        spec.kind = ChartKind::Bar {
            horizontal: false,
            placement_stacked: true,
            value_stacked: true,
        };
        spec.series[0].values = vec![10.0, 20.0];
        let mut second = spec.series[0].clone();
        second.values = vec![30.0, -7.0];
        spec.series.push(second);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;

        let (min, max) = value_domain(&spec, &spec.y_axis);

        assert_eq!((min, max), (10.0, 40.0));
    }

    #[test]
    fn log_value_domain_stacked_bar_preserves_hard_max() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.kind = ChartKind::Bar {
            horizontal: false,
            placement_stacked: true,
            value_stacked: true,
        };
        spec.series[0].values = vec![10.0];
        let mut second = spec.series[0].clone();
        second.values = vec![30.0];
        spec.series.push(second);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;
        spec.y_axis.min = Some(8.0);
        spec.y_axis.max = Some(35.0);

        assert_eq!(value_domain(&spec, &spec.y_axis), (8.0, 35.0));
    }

    #[test]
    fn log_value_domain_substitutes_zero_with_decade_below_min_positive() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = vec![0.0, 30.0];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!(min, 3.0); // 30 の1桁下
        assert_eq!(max, 30.0);
    }

    #[test]
    fn log_value_domain_begin_at_zero_rounds_non_boundary_min_down_to_decade_floor() {
        // begin_at_zero=true でも 0 は含めない(対数軸に存在しえないため)。
        //
        // 実機バグ回帰テスト: chart.js 実測(tools/ で 40/80/11/5/2/10/100/999/12/99
        // 等、多数の値で確認)により、beginAtZero:true は min_positive がちょうど
        // decade 境界か否かによらず常に "decade floor"
        // (10^floor(log10(min_positive)))へドメイン下端を切り下げることが
        // 判明した。40 は decade 境界ではないが、床は 10 になる。
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = true;
        spec.series[0].values = vec![40.0, 80.0];
        let (min, _max) = value_domain(&spec, &spec.y_axis);
        assert_eq!(min, 10.0);
    }

    #[test]
    fn log_value_domain_begin_at_zero_false_leaves_non_boundary_min_untouched() {
        // beginAtZero:false では decade floor への切り下げは起きず、min_positive を
        // そのまま使う(上のテストとの対比、線形パスとの一貫性確認)。
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;
        spec.series[0].values = vec![40.0, 80.0];
        let (min, _max) = value_domain(&spec, &spec.y_axis);
        assert_eq!(min, 40.0);
    }

    /// chart.js 実測(tools/ で node chart.js を実行し、複数の min_positive 値について
    /// beginAtZero:true の scale.min を直接比較して確認)を網羅的に固定する。
    /// 規則: 常に decade floor(10^floor(log10(min_positive)))へ切り下げ、
    /// その床がちょうど min_positive 自身と一致する場合のみさらに1桁下げる。
    #[test]
    fn log_value_domain_begin_at_zero_decade_floor_rule_matches_chartjs_measurements() {
        let cases: &[(f64, f64)] = &[
            (40.0, 10.0),
            (80.0, 10.0),
            (11.0, 10.0),
            (5.0, 1.0),
            (2.0, 1.0),
            (10.0, 1.0),
            (100.0, 10.0),
            (999.0, 100.0),
            (12.0, 10.0),
            (99.0, 10.0),
        ];
        for &(min_positive, expected_min) in cases {
            let mut spec = make_bar_spec(1, 600.0);
            spec.y_axis.scale_kind = ScaleKind::Logarithmic;
            spec.y_axis.begin_at_zero = true;
            spec.series[0].values = vec![min_positive, min_positive * 10.0];
            let (min, _max) = value_domain(&spec, &spec.y_axis);
            assert_eq!(
                min, expected_min,
                "min_positive={min_positive}: expected beginAtZero min={expected_min}, got {min}"
            );
        }
    }

    /// 実機バグ回帰テスト: beginAtZero:true(縦棒の既定)かつ最小正値がちょうど
    /// decade 境界(10^n)のとき、そのままだとドメイン下端(=軸の描画上の床)と
    /// 最小値が完全一致し、そのバーの高さが 0 になって消えてしまう(実機
    /// レンダリングで再現・確認済み)。chart.js 実測(tools/ で node chart.js
    /// 実行して確認: [10,100] beginAtZero:true → min=1)に合わせ、この場合のみ
    /// さらに1桁下げる。PR #144 の自動レビュー(P1)で指摘。
    #[test]
    fn log_value_domain_begin_at_zero_widens_by_one_decade_when_min_is_exact_boundary() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = true;
        spec.series[0].values = vec![10.0, 100.0];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!((min, max), (1.0, 100.0));
    }

    #[test]
    fn log_value_domain_begin_at_zero_false_does_not_widen_exact_boundary() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;
        spec.series[0].values = vec![10.0, 100.0];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!((min, max), (10.0, 100.0));
    }

    #[test]
    fn log_value_domain_ignores_non_positive_suggested_bounds() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.suggested_min = Some(-10.0);
        spec.y_axis.suggested_max = Some(0.0);
        spec.series[0].values = vec![10.0, 20.0];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        // make_bar_spec の y_axis は begin_at_zero:true(縦棒の既定)で、最小正値 10.0 は
        // ちょうど decade 境界(10^1)なので、beginAtZero 特例で1桁下がって 1.0 になる
        // (chart.js 実測: [10,100] beginAtZero:true → min=1、下記コメント参照)。
        // 非正の suggested_min/suggested_max(-10/0)が無視されている点は変わらず検証できる。
        assert_eq!(min, 1.0);
        assert_eq!(max, 20.0);
    }

    #[test]
    fn log_value_domain_positive_suggested_bounds_widen_domain() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.suggested_min = Some(0.5);
        spec.y_axis.suggested_max = Some(1000.0);
        spec.series[0].values = vec![10.0, 20.0];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!((min, max), (0.5, 1000.0));
    }

    #[test]
    fn log_value_domain_falls_back_when_all_non_positive() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = vec![0.0, -5.0]; // 非正値はドメインから除外される
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!((min, max), (1.0, 10.0));
    }

    /// 実機バグ回帰テスト: データが空/0のみ/負のみの場合、正の
    /// suggested_min/suggested_max が指定されていてもハードコードされた 1..10 に
    /// 潰れ、明示的に設定した軸オプションが無視されていた(PR #144 の自動レビューで
    /// 指摘)。線形版(データなし → suggested を初期シードにする)と同じ契約に揃える。
    #[test]
    fn log_value_domain_honors_suggested_bounds_when_no_positive_data() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = vec![0.0]; // 正データなし
        spec.y_axis.suggested_min = Some(0.01);
        spec.y_axis.suggested_max = Some(100.0);
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!((min, max), (0.01, 100.0));
    }

    #[test]
    fn log_value_domain_honors_single_suggested_bound_when_no_positive_data() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = vec![0.0];
        spec.y_axis.suggested_max = Some(500.0);
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!(
            (min, max),
            (50.0, 500.0),
            "suggested_min 未指定時は suggested_max の1桁下を使う"
        );
    }

    /// 実機バグ回帰テスト: suggested_max のみがサブユニット(1.0未満)で指定された
    /// 場合、以前は固定の lo=1.0 と比較されて hi<=lo に落ち、lo*10.0=10.0 に潰れて
    /// suggested_max=0.01 の指定が完全に無視されていた。lo は常に hi の1桁下から
    /// 導出すべき(PR #144 の自動レビューで指摘)。
    #[test]
    fn log_value_domain_honors_sub_unit_suggested_max_when_no_positive_data() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = vec![0.0];
        spec.y_axis.suggested_max = Some(0.01);
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!((min, max), (0.001, 0.01));
    }

    #[test]
    fn log_value_domain_degenerate_domain_near_f64_max_stays_finite() {
        // domain_min == domain_max == 5e307 (> f64::MAX/10) だと、線形版の
        // `+1.0` に相当する縮退補正が単純な ×10 だと +inf にオーバーフローする。
        // 上端から下へ広げて、有限で幅を持つことを固定する回帰テスト。
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;
        spec.series[0].values = vec![5e307, 5e307];
        let (min, max) = value_domain(&spec, &spec.y_axis);
        assert_eq!(min, 5e306);
        assert!(max.is_finite(), "max should stay finite, got {max}");
        assert_eq!(max, 5e307);
    }

    #[test]
    fn log_value_domain_single_f64_max_widens_downward() {
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.y_axis.begin_at_zero = false;
        spec.series[0].values = vec![f64::MAX];

        let (min, max) = value_domain(&spec, &spec.y_axis);

        assert_eq!(max, f64::MAX);
        assert_eq!(min, f64::MAX / 10.0);
        assert!(
            max > min,
            "domain must have a positive width: [{min}, {max}]"
        );
    }

    #[test]
    fn log_value_domain_zero_substitution_never_produces_non_positive_min() {
        // min_positive が最小の非正規化数(subnormal)近傍だと ÷10 が 0.0 へ
        // アンダーフローしうる。ドメイン下端は常に正であるべき。
        let mut spec = make_bar_spec(1, 600.0);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = vec![0.0, 5e-324];
        let (min, _max) = value_domain(&spec, &spec.y_axis);
        assert!(min > 0.0, "min should stay positive, got {min}");
    }

    /// 対数 y 軸の ChartSpec を組み立てる共通ヘルパ。
    fn log_spec(values: Vec<f64>, width: f64) -> ChartSpec {
        let mut spec = make_bar_spec(1, width);
        spec.y_axis.scale_kind = ScaleKind::Logarithmic;
        spec.series[0].values = values;
        spec
    }

    /// 対数軸の compute() が返す `Frame` を検証する共通ヘルパ。
    fn compute_log_frame(values: Vec<f64>, width: f64) -> Frame {
        let spec = log_spec(values, width);
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        compute(&spec, &m)
    }

    #[test]
    fn compute_log_scale_ys_is_log_variant_with_ticks_min_as_floor() {
        let frame = compute_log_frame(vec![1.0, 100_000.0], 600.0);
        match &frame.ys {
            ValueScale::Log { inner: _, floor } => {
                assert_eq!(*floor, frame.ticks.min);
            }
            ValueScale::Linear(_) => panic!("expected ValueScale::Log for a logarithmic axis"),
        }
        // 軸は上下反転しているので、最小 tick はプロット下端、最大 tick は上端。
        assert_eq!(frame.ys.map(frame.ticks.min), frame.plot_bottom);
        assert_eq!(frame.ys.map(frame.ticks.max), frame.plot_top);
        // 中間の decade(10^2 = 100)は下端と上端の間、かつ単調減少の位置に来るべき。
        let mid_y = frame.ys.map(100.0);
        assert!(
            mid_y > frame.plot_top && mid_y < frame.plot_bottom,
            "mid_y={mid_y} should be strictly between plot_top={} and plot_bottom={}",
            frame.plot_top,
            frame.plot_bottom
        );
    }

    #[test]
    fn compute_log_scale_step_is_zero_sentinel() {
        // 対数軸では decade 間隔が一定でないため、step は 0.0 の番兵を返す
        // (nice_ticks/vega_nice_ticks は常に正の step を返すため、0.0 は
        // 呼び出し側が「これは log_ticks 経由の Frame だ」と判定できる合図になる)。
        let frame = compute_log_frame(vec![1.0, 100.0], 600.0);
        assert_eq!(frame.ticks.step, 0.0);
    }

    #[test]
    fn compute_log_scale_minor_ticks_populated_and_bracketed_by_majors() {
        let frame = compute_log_frame(vec![1.0, 1000.0], 600.0);
        assert!(
            !frame.minor_ticks.is_empty(),
            "minor ticks should be populated for a multi-decade log domain"
        );
        for &t in &frame.minor_ticks {
            assert!(
                t > frame.ticks.min && t < frame.ticks.max,
                "minor tick {t} should lie strictly inside [{}, {}]",
                frame.ticks.min,
                frame.ticks.max
            );
        }
    }

    #[test]
    fn compute_linear_scale_minor_ticks_stays_empty() {
        // 線形軸では常に minor_ticks が空であることを固定する回帰テスト
        // (log 専用フィールドが線形パスへ意図せず漏れ出さないことの保証)。
        let spec = make_bar_spec(3, 400.0);
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        assert!(frame.minor_ticks.is_empty());
        assert!(matches!(frame.ys, ValueScale::Linear(_)));
    }

    #[test]
    fn compute_log_scale_realistic_domains_produce_reasonable_tick_counts() {
        // fulgur-chart-8so(log_ticks の目盛数上限未実装)は既知の追跡issueだが、
        // それはドメインが極端(数百 decade)に広い/縮退した場合の話であり、
        // ここで確認するのは「現実的な」ドメイン(1..100000 や 0.01..1000 のような
        // 数桁レンジ)では目盛数が数十本程度に収まり、数千本には爆発しないこと。
        for (values, max_major, max_minor) in [
            (vec![1.0, 100_000.0], 10, 80),
            (vec![0.01, 1000.0], 10, 80),
            (vec![42.0, 1337.0], 6, 32),
        ] {
            let frame = compute_log_frame(values.clone(), 600.0);
            assert!(
                frame.ticks.ticks.len() <= max_major,
                "values={values:?}: major tick count {} exceeds {max_major}",
                frame.ticks.ticks.len()
            );
            assert!(
                frame.minor_ticks.len() <= max_minor,
                "values={values:?}: minor tick count {} exceeds {max_minor}",
                frame.minor_ticks.len()
            );
        }
    }

    #[test]
    fn draw_frame_log_scale_labels_major_ticks_only_with_fmt_num_log() {
        let spec = log_spec(vec![1.0, 100_000.0], 600.0);
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let y_labels: Vec<&String> = items
            .iter()
            .filter_map(|p| match p {
                Prim::Text { x, content, .. } if (*x - (frame.plot_left - 6.0)).abs() < 0.01 => {
                    Some(content)
                }
                _ => None,
            })
            .collect();
        // ラベルは major tick の数だけ(minor にはラベルを付けない)。
        assert_eq!(y_labels.len(), frame.ticks.ticks.len());
        // 対数フォーマッタなので大きい値も指数表記に丸められず全桁表示される。
        assert!(
            y_labels.iter().any(|s| s.as_str() == "100000"),
            "{y_labels:?}"
        );
        assert!(y_labels.iter().any(|s| s.as_str() == "1"), "{y_labels:?}");
    }

    #[test]
    fn draw_frame_log_scale_grid_includes_major_and_minor_lines() {
        let spec = log_spec(vec![1.0, 100.0], 600.0);
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let horizontal_grid_lines = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { y1, y2, x1, x2, .. }
                        if (y1 - y2).abs() < 0.01
                            && (*x1 - frame.plot_left).abs() < 0.01
                            && (*x2 - frame.plot_right).abs() < 0.01
                )
            })
            .count();
        // major(frame.ticks.ticks) + minor(frame.minor_ticks) 本のグリッド線 + baseline 1本。
        assert_eq!(
            horizontal_grid_lines,
            frame.ticks.ticks.len() + frame.minor_ticks.len() + 1
        );
    }

    #[test]
    fn draw_frame_log_scale_grid_display_false_skips_major_and_minor_lines() {
        let mut spec = log_spec(vec![1.0, 100.0], 600.0);
        spec.y_axis.grid.display = false;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let horizontal_grid_lines = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { y1, y2, x1, x2, .. }
                        if (y1 - y2).abs() < 0.01
                            && (*x1 - frame.plot_left).abs() < 0.01
                            && (*x2 - frame.plot_right).abs() < 0.01
                )
            })
            .count();
        // baseline (border) の 1 本だけが残る。
        assert_eq!(horizontal_grid_lines, 1);
    }

    #[test]
    fn draw_frame_log_scale_draw_ticks_covers_major_and_minor() {
        // gridline(2b)は major/minor 両方に描く一方、tick 刻み(3b)が major だけだと
        // 「グリッド線はあるのに対応する軸の刻みが無い」という見た目の不整合が生じる。
        // 対数軸では tick 刻みも major+minor の本数だけ描かれることを固定する。
        let mut spec = log_spec(vec![1.0, 100.0], 600.0);
        spec.y_axis.grid.draw_ticks = true;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let tick_count = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (y1 - y2).abs() < 0.01
                            && ((*x2 - *x1) - 4.0).abs() < 1e-9
                            && (*x2 - frame.plot_left).abs() < 0.01
                )
            })
            .count();
        assert_eq!(
            tick_count,
            frame.ticks.ticks.len() + frame.minor_ticks.len(),
            "log 軸の tick 刻み数は major+minor の本数と一致すべき"
        );
    }

    #[test]
    fn compute_log_scale_widens_plot_left_for_full_precision_labels() {
        // Step 4: y 軸ラベル幅計算も fmt_num_log を使うべき。fmt_num(0.0001) は
        // 小数2桁丸めで "0" に潰れる(幅が狭い)が、fmt_num_log(0.0001) は
        // "0.0001" をそのまま返す(幅が広い)。fmt_num のままだと "0.0001" ラベルが
        // 左マージンからはみ出してクリップされる。fmt_num と fmt_num_log で
        // 結果が分岐する値を使わない限り、この Step 4 の分岐は誤って fmt_num の
        // ままでもテストが通ってしまう(実際に一度そのバグを作って確認済み)。
        let mut spec = log_spec(vec![0.0001, 1.0], 600.0);
        // beginAtZero:false を明示: 最小値 0.0001 はちょうど decade 境界(10^-4)なので、
        // 既定の beginAtZero:true のままだとドメインが1桁広がり(0.00001 まで)、
        // このテストの主眼(ラベル幅計算)から逸れる余分な major tick が増えてしまう。
        spec.y_axis.begin_at_zero = false;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);

        let font = spec.theme.font_size as f32;
        assert_eq!(fmt_num(0.0001), "0", "sanity: fmt_num rounds this to 0");
        assert_eq!(
            crate::num::fmt_num_log(0.0001),
            "0.0001",
            "sanity: fmt_num_log keeps full precision"
        );
        let narrow_w = m.width("0", font) as f64;
        let wide_w = m.width("0.0001", font) as f64;
        assert!(
            wide_w > narrow_w,
            "test fixture assumption: {wide_w} > {narrow_w}"
        );

        // plot_left は Step 4 の fmt_num_log 経由の幅計算に一致する
        // (OUTER_PAD + max_w + 10.0; make_bar_spec は Canvas サイズモード・
        // 凡例なし・y軸タイトルなしなので legend_left/y_title_w は 0)。
        let expected_plot_left = OUTER_PAD + wide_w + 10.0;
        assert!(
            (frame.plot_left - expected_plot_left).abs() < 1e-6,
            "plot_left={} should match fmt_num_log-based width {expected_plot_left}",
            frame.plot_left
        );
        // fmt_num の(誤った)幅を使っていたら plot_left はこれより狭くなる。
        let plot_left_if_fmt_num_were_used = OUTER_PAD + narrow_w + 10.0;
        assert!(
            frame.plot_left > plot_left_if_fmt_num_were_used,
            "plot_left={} should exceed the fmt_num-based (narrower) width {plot_left_if_fmt_num_were_used}",
            frame.plot_left
        );
    }

    #[test]
    fn grid_display_false_produces_no_grid_lines() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.grid.display = false;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        // Baseline (border) は 1 本残る。gridline は 0。プロット両端の x を持つ y=const な水平線を数える。
        let horizontal_lines = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { y1, y2, x1, x2, .. }
                        if (y1 - y2).abs() < 0.01
                            && ((*x1 - frame.plot_left).abs() < 0.01
                                && (*x2 - frame.plot_right).abs() < 0.01)
                )
            })
            .count();
        assert_eq!(
            horizontal_lines, 1,
            "grid.display=false → gridline 0 本、baseline 1 本のみ"
        );
    }

    #[test]
    fn grid_color_override_reaches_prim() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.grid.color = Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let red_gridline = items.iter().any(|p| {
            matches!(p,
                Prim::Line { stroke: Color { r: 255, g: 0, b: 0, .. }, y1, y2, .. }
                    if (y1 - y2).abs() < 0.01
            )
        });
        assert!(
            red_gridline,
            "grid.color=red は Prim::Line の stroke に反映されるべき"
        );
    }

    #[test]
    fn grid_line_width_reaches_prim() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.grid.line_width = 3.0;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        // 少なくとも 1 本の水平線が stroke_width=3.0 のはず。
        let thick = items.iter().any(|p| {
            matches!(p,
                Prim::Line { stroke_width, y1, y2, .. }
                    if (stroke_width - 3.0).abs() < 1e-9 && (y1 - y2).abs() < 0.01
            )
        });
        assert!(
            thick,
            "grid.line_width=3.0 は stroke_width に反映されるべき"
        );
    }

    #[test]
    fn categorical_x_grid_uses_category_centers_and_style() {
        let mut spec = make_bar_spec(3, 400.0);
        let grid = Color {
            r: 12,
            g: 34,
            b: 56,
            a: 1.0,
        };
        spec.x_axis.grid.color = Some(grid);
        spec.x_axis.grid.line_width = 2.5;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let paths: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(stroke),
                    stroke_width,
                } if *stroke == grid && (*stroke_width - 2.5).abs() < 1e-9 => Some(d),
                _ => None,
            })
            .collect();

        let plot_width = frame.plot_right - frame.plot_left;
        let expected = [
            frame.plot_left + plot_width / 6.0,
            frame.plot_left + plot_width / 2.0,
            frame.plot_left + plot_width * 5.0 / 6.0,
        ];
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].matches("M ").count(), expected.len());
        let expected_path = expected
            .iter()
            .map(|x| {
                format!(
                    "M {} {} L {} {}",
                    fmt_num(*x),
                    fmt_num(frame.plot_top),
                    fmt_num(*x),
                    fmt_num(frame.plot_bottom)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(paths[0], &expected_path);
    }

    #[test]
    fn categorical_x_grid_precedes_x_axis_border() {
        let mut spec = make_bar_spec(3, 400.0);
        let grid = Color {
            r: 12,
            g: 34,
            b: 56,
            a: 1.0,
        };
        let border = Color {
            r: 65,
            g: 43,
            b: 21,
            a: 1.0,
        };
        spec.x_axis.grid.color = Some(grid);
        spec.x_axis.border.color = Some(border);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let grid_index = items
            .iter()
            .position(
                |item| matches!(item, Prim::Path { stroke: Some(color), .. } if *color == grid),
            )
            .expect("categorical grid path");
        let border_index = items
            .iter()
            .position(|item| {
                matches!(item,
                    Prim::Line { x1, y1, x2, y2, stroke, .. }
                        if (*x1 - frame.plot_left).abs() < 0.01
                            && (*y1 - frame.plot_bottom).abs() < 0.01
                            && (*x2 - frame.plot_right).abs() < 0.01
                            && (*y2 - frame.plot_bottom).abs() < 0.01
                            && *stroke == border
                )
            })
            .expect("x-axis border");
        assert!(
            grid_index < border_index,
            "the x-axis border must be painted over categorical grid endpoints"
        );
    }

    #[test]
    fn categorical_x_grid_display_false_keeps_labels_without_lines() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.grid.display = false;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let grid_paths = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Path {
                        fill: None,
                        stroke: Some(_),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(grid_paths, 0);
        for label in ["Cat0000", "Cat0001", "Cat0002"] {
            assert!(
                items
                    .iter()
                    .any(|item| { matches!(item, Prim::Text { content, .. } if content == label) })
            );
        }
    }

    #[test]
    fn categorical_x_grid_line_offset_false_uses_plot_edges() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.kind = ChartKind::Line {
            stacked: false,
            stacked_missing_values_are_gaps: false,
        };
        spec.series[0].series_type = SeriesType::Line;
        spec.x_axis.offset = false;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let paths: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(_),
                    ..
                } => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].matches("M ").count(), 3);
        assert!(paths[0].starts_with(&format!(
            "M {} {} L {} {}",
            fmt_num(frame.plot_left),
            fmt_num(frame.plot_top),
            fmt_num(frame.plot_left),
            fmt_num(frame.plot_bottom),
        )));
        assert!(paths[0].ends_with(&format!(
            "M {} {} L {} {}",
            fmt_num(frame.plot_right),
            fmt_num(frame.plot_top),
            fmt_num(frame.plot_right),
            fmt_num(frame.plot_bottom),
        )));
    }

    #[test]
    fn categorical_x_grid_follows_auto_skipped_ticks() {
        let spec = make_bar_spec(20, 80.0);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let grid_path = items
            .iter()
            .find_map(|item| match item {
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(_),
                    ..
                } => Some(d),
                _ => None,
            })
            .expect("visible categorical ticks should share one grid path");
        let label_count = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Text {
                        anchor: Anchor::Middle,
                        ..
                    }
                )
            })
            .count();
        assert!(
            label_count < 20,
            "dense category labels must be auto-skipped"
        );
        assert_eq!(
            grid_path.matches("M ").count(),
            label_count,
            "x-axis grid subpaths must follow the visible category ticks"
        );
    }

    #[test]
    fn categorical_x_grid_keeps_tick_for_empty_label() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.categories[1] = String::new();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let grid_path = items
            .iter()
            .find_map(|item| match item {
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(_),
                    ..
                } => Some(d),
                _ => None,
            })
            .expect("visible categorical ticks should share one grid path");
        let label_count = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Text {
                        anchor: Anchor::Middle,
                        ..
                    }
                )
            })
            .count();

        assert_eq!(grid_path.matches("M ").count(), 3);
        assert_eq!(label_count, 2);
    }

    #[test]
    fn categorical_x_grid_skips_empty_labels_by_geometry() {
        let mut spec = make_bar_spec(100, 80.0);
        for category in &mut spec.categories {
            category.clear();
        }
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);

        let grid_path = items
            .iter()
            .find_map(|item| match item {
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(_),
                    ..
                } => Some(d),
                _ => None,
            })
            .expect("empty category ticks should still produce a grid path");
        let label_count = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Text {
                        anchor: Anchor::Middle,
                        ..
                    }
                )
            })
            .count();

        assert!(
            grid_path.matches("M ").count() <= 20,
            "the 4px minimum tick spacing must cap this 80px-wide chart at 20 grid segments"
        );
        assert_eq!(label_count, 0);
    }

    #[test]
    fn border_display_false_produces_no_baseline() {
        // baseline (ink 色) と 最下段 gridline (theme.grid_color 色) は
        // 幾何 (y=plot_bottom, x=plot_left..plot_right) が一致するため、
        // baseline のみを識別するには stroke 色でも絞り込む必要がある。
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.border.display = false;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let ink = spec.theme.text_color;
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let baseline_count = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { y1, y2, x1, x2, stroke, .. }
                        if (y1 - y2).abs() < 0.01
                            && (*y1 - frame.plot_bottom).abs() < 0.01
                            && (*x1 - frame.plot_left).abs() < 0.01
                            && (*x2 - frame.plot_right).abs() < 0.01
                            && stroke.r == ink.r && stroke.g == ink.g && stroke.b == ink.b
                )
            })
            .count();
        assert_eq!(baseline_count, 0, "border.display=false → baseline なし");
    }

    #[test]
    fn border_dash_reaches_baseline() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.border.dash = vec![4.0, 4.0];
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let has_dashed_baseline = items.iter().any(|p| {
            matches!(p,
                Prim::Line { y1, y2, dash, .. }
                    if (y1 - y2).abs() < 0.01
                        && (*y1 - frame.plot_bottom).abs() < 0.01
                        && dash == &vec![4.0, 4.0]
            )
        });
        assert!(
            has_dashed_baseline,
            "border.dash が baseline に伝わっていない"
        );
    }

    #[test]
    fn border_color_and_width_reach_baseline() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.border.color = Some(Color {
            r: 0,
            g: 100,
            b: 0,
            a: 1.0,
        });
        spec.x_axis.border.width = 2.5;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let has_custom_baseline = items.iter().any(|p| {
            matches!(p,
                Prim::Line { y1, y2, stroke, stroke_width, .. }
                    if (y1 - y2).abs() < 0.01
                        && (*y1 - frame.plot_bottom).abs() < 0.01
                        && stroke.r == 0 && stroke.g == 100 && stroke.b == 0
                        && (stroke_width - 2.5).abs() < 1e-9
            )
        });
        assert!(has_custom_baseline);
    }

    #[test]
    fn grid_draw_ticks_true_adds_tick_marks() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.grid.draw_ticks = true;
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        // tick 短線: x1 = plot_left - 4, x2 = plot_left, y1 == y2
        let tick_count = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (y1 - y2).abs() < 0.01
                            && ((*x2 - *x1) - 4.0).abs() < 1e-9
                            && (*x2 - frame.plot_left).abs() < 0.01
                )
            })
            .count();
        assert!(
            tick_count > 0,
            "draw_ticks=true で tick 刻み描画されるべき: 実際 {tick_count}"
        );
        assert_eq!(
            tick_count,
            frame.ticks.ticks.len(),
            "tick 数は y ticks 数と一致"
        );
    }

    #[test]
    fn grid_draw_ticks_false_produces_no_tick_marks() {
        let spec = make_bar_spec(3, 400.0); // default: draw_ticks=false
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let tick_count = items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (y1 - y2).abs() < 0.01
                            && ((*x2 - *x1) - 4.0).abs() < 1e-9
                            && (*x2 - frame.plot_left).abs() < 0.01
                )
            })
            .count();
        assert_eq!(tick_count, 0, "draw_ticks=false は tick 刻みを描かない");
    }

    #[test]
    fn y_axis_title_shifts_plot_left_right() {
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let spec_no_title = make_bar_spec(3, 400.0);
        let mut spec_with_title = make_bar_spec(3, 400.0);
        spec_with_title.y_axis.title = Some(AxisTitle {
            text: "売上 (円)".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        let f_no = compute(&spec_no_title, &m);
        let f_ti = compute(&spec_with_title, &m);
        assert!(
            f_ti.plot_left > f_no.plot_left,
            "Y 軸タイトル分だけ plot_left が右にシフトすべき: no={} ti={}",
            f_no.plot_left,
            f_ti.plot_left
        );
    }

    #[test]
    fn y_axis_title_renders_rotated_text() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.title = Some(AxisTitle {
            text: "売上".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let has_rotated = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, rotate_deg: Some(deg), .. }
                    if content == "売上" && (deg.abs() - 90.0).abs() < 0.1
            )
        });
        assert!(has_rotated, "Y 軸タイトルは -90deg で描画されるべき");
    }

    #[test]
    fn y_axis_title_align_start_positions_at_plot_bottom() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.title = Some(AxisTitle {
            text: "T".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Start,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let found = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, y, rotate_deg: Some(_), .. }
                    if content == "T" && (y - frame.plot_bottom).abs() < 0.1
            )
        });
        assert!(
            found,
            "Chart.js 準拠: align=Start は Y 軸下端(bottom-to-top 読みの起点)"
        );
    }

    #[test]
    fn y_axis_title_align_end_positions_at_plot_top() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.title = Some(AxisTitle {
            text: "E".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::End,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let found = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, y, rotate_deg: Some(_), .. }
                    if content == "E" && (y - frame.plot_top).abs() < 0.1
            )
        });
        assert!(found, "Chart.js 準拠: align=End は Y 軸上端");
    }

    #[test]
    fn y_axis_title_color_and_font_size_override() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.y_axis.title = Some(AxisTitle {
            text: "X".into(),
            color: Some(Color {
                r: 128,
                g: 0,
                b: 128,
                a: 1.0,
            }),
            font_size: Some(20.0),
            align: AxisTitleAlign::Center,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let found = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, size, fill, rotate_deg: Some(_), .. }
                    if content == "X"
                        && (size - 20.0).abs() < 1e-9
                        && fill.r == 128 && fill.b == 128
            )
        });
        assert!(found);
    }

    #[test]
    fn no_y_axis_title_produces_no_rotated_text() {
        let spec = make_bar_spec(3, 400.0); // title=None default
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let any_rotated = items.iter().any(|p| {
            matches!(
                p,
                Prim::Text {
                    rotate_deg: Some(_),
                    ..
                }
            )
        });
        assert!(!any_rotated, "title=None なら rotated text は無し");
    }

    #[test]
    fn x_axis_title_shifts_plot_bottom_up() {
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let a = make_bar_spec(3, 400.0);
        let mut b = make_bar_spec(3, 400.0);
        b.x_axis.title = Some(AxisTitle {
            text: "時刻".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        let fa = compute(&a, &m);
        let fb = compute(&b, &m);
        assert!(
            fb.plot_bottom < fa.plot_bottom,
            "X タイトルぶん plot_bottom が上にシフトすべき: fa={} fb={}",
            fa.plot_bottom,
            fb.plot_bottom
        );
    }

    #[test]
    fn x_axis_title_renders_horizontal_text() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.title = Some(AxisTitle {
            text: "時刻".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let has_horizontal_title = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, rotate_deg, .. }
                    if content == "時刻" && rotate_deg.is_none()
            )
        });
        assert!(has_horizontal_title, "X タイトルは rotate なしで描画");
    }

    #[test]
    fn x_axis_title_align_start_positions_at_plot_left() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.title = Some(AxisTitle {
            text: "T".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Start,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let found = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, x, rotate_deg: None, .. }
                    if content == "T" && (x - frame.plot_left).abs() < 0.1
            )
        });
        assert!(found, "Chart.js 準拠: X の Start は plot_left");
    }

    #[test]
    fn x_axis_title_align_end_positions_at_plot_right() {
        let mut spec = make_bar_spec(3, 400.0);
        spec.x_axis.title = Some(AxisTitle {
            text: "T".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::End,
        });
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        let found = items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, x, rotate_deg: None, .. }
                    if content == "T" && (x - frame.plot_right).abs() < 0.1
            )
        });
        assert!(found, "Chart.js 準拠: X の End は plot_right");
    }

    #[test]
    fn no_x_axis_title_produces_no_extra_horizontal_text_below_labels() {
        // plot_bottom がシフトしないことは x_axis_title_shifts_plot_bottom_up で担保。
        // ここでは title=None で下側バンドの余分な text が生えないことを assert する。
        let spec = make_bar_spec(3, 400.0); // title=None
        let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let frame = compute(&spec, &m);
        let mut items = Vec::new();
        draw_frame(&mut items, &spec, &frame, &m);
        // plot_bottom + X_LABEL_BAND + font * 0.9 に近い y を持つ text がないこと
        let expected_y = frame.plot_bottom + X_LABEL_BAND + spec.theme.font_size * 1.1 * 0.9;
        let stray = items.iter().any(|p| {
            matches!(p,
                Prim::Text { y, rotate_deg: None, .. } if (y - expected_y).abs() < 1.0
            )
        });
        assert!(!stray, "title=None なら x-title 位置に余分な text は無い");
    }
}

#[cfg(test)]
mod radial_domain_tests {
    use super::resolve_radial_domain;
    use crate::ir::RadialAxis;

    fn ra(
        min: Option<f64>,
        max: Option<f64>,
        suggested_min: Option<f64>,
        suggested_max: Option<f64>,
        begin_at_zero: bool,
    ) -> RadialAxis {
        RadialAxis {
            min,
            max,
            suggested_min,
            suggested_max,
            begin_at_zero,
        }
    }

    /// 有限データが無い場合、片側だけの `suggested*` を暫定レンジとして使う。
    /// 0 で埋めると `suggestedMin: 100` が expand-only 判定 (s < lo) で捨てられ、
    /// 100 付近ではなく 0 付近のドメインになってしまう。
    ///
    /// レンダリング経由では観測できない (データが無い radar はリングを等間隔に
    /// 描くだけなので SVG が同一になる) ため、resolver を直接検証する。
    #[test]
    fn empty_data_seeds_domain_from_one_sided_suggestion() {
        let (lo, hi) = resolve_radial_domain(
            &ra(None, None, Some(100.0), None, false),
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        assert!(
            lo > 90.0 && hi > 100.0 && hi > lo,
            "suggestedMin:100 は 100 付近へ展開されるべき: [{lo}, {hi}]"
        );

        // 対称ケース: suggestedMax のみ。
        let (lo, hi) = resolve_radial_domain(
            &ra(None, None, None, Some(-100.0), false),
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        assert!(
            hi < -90.0 && lo < -100.0 && hi > lo,
            "suggestedMax:-100 は -100 付近へ展開されるべき: [{lo}, {hi}]"
        );

        // suggestion が無い場合は従来通り 0 起点。
        let (lo, hi) = resolve_radial_domain(
            &ra(None, None, None, None, true),
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        assert_eq!((lo, hi), (0.0, 1.0));
    }

    /// hard bound と自動側が逆転している場合、自動側を hard 側へ寄せてから展開する。
    /// chart.js の `getMinMax` → `handleTickRangeOptions` と同じ順序。
    /// 寄せずに無効な自動側を base にすると、5% 展開後もまだ hard bound の反対側に
    /// 留まり、最終救済で 1ULP 幅のドメインになってしまう。
    #[test]
    fn inverted_range_expands_from_the_hard_bound() {
        // min: 100 (hard) / データ最大 50 → [100, 105] になるべき (1ULP 幅ではなく)。
        let (lo, hi) = resolve_radial_domain(&ra(Some(100.0), None, None, None, false), 10.0, 50.0);
        assert_eq!(lo, 100.0, "hard min は動かさない");
        assert!(
            hi > 100.0 && (hi - lo) > 1.0,
            "hard min から 5% 展開されるべき: [{lo}, {hi}]"
        );

        // 対称ケース: max: -100 (hard) / データ最小 -50 → [-105, -100] 相当。
        let (lo, hi) =
            resolve_radial_domain(&ra(None, Some(-100.0), None, None, false), -50.0, -10.0);
        assert_eq!(hi, -100.0, "hard max は動かさない");
        assert!(
            lo < -100.0 && (hi - lo) > 1.0,
            "hard max から 5% 展開されるべき: [{lo}, {hi}]"
        );
    }

    /// f64::MAX 近傍の縮退ドメインでも、有限かつ幅のあるドメインになること。
    /// `base + offset` はオーバーフローし、`lo + 1.0` は丸めで lo に戻るため、
    /// 素朴な実装だと幅 0 のまま描画が消える。
    #[test]
    fn degenerate_domain_near_f64_max_keeps_finite_width() {
        let v = f64::MAX;
        let (lo, hi) = resolve_radial_domain(&ra(None, None, None, None, false), v, v);
        assert!(
            lo.is_finite() && hi.is_finite(),
            "有限であること: [{lo}, {hi}]"
        );
        assert!(hi > lo, "幅を持つこと: [{lo}, {hi}]");

        // 両側 hard で min == max == f64::MAX の矛盾指定でも壊れないこと。
        let (lo, hi) = resolve_radial_domain(&ra(Some(v), Some(v), None, None, false), v, v);
        assert!(lo.is_finite() && hi.is_finite() && hi > lo, "[{lo}, {hi}]");
    }
}
