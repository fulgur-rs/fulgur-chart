//! bar チャートのレイアウト: ChartSpec → Scene。
//! 縦棒・横棒に対応。決定的に組み立て、NaN/Inf/panic を出さない。

use crate::ir::{BarBorderRadius, BarGeometryOptions, BarThickness, ChartSpec};
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

const DEFAULT_CATEGORY_PERCENTAGE: f64 = 0.8;
const DEFAULT_BAR_PERCENTAGE: f64 = 0.9;
const DEFAULT_CATEGORY_PADDING: f64 = 0.1;
const MAX_BAR_THICKNESS: f64 = 32768.0;
/// 極端に長い目盛ラベルでも LinearScale のプロット幅を 0 にしない下限。
const MIN_HORIZONTAL_PLOT_WIDTH: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarSide {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy)]
pub(crate) struct BarBounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

#[derive(Clone, Copy)]
struct StackedHorizontalSegment {
    series_index: usize,
    stack_group: usize,
    value: f64,
    base: f64,
    head: f64,
    bounds: BarBounds,
}

/// Builds a bar rectangle or a path with Chart.js-style rounded corners.
/// `base_side` names the edge touching the bar's base; uniform radii only round the opposite edge.
pub(crate) fn bar_primitive(
    bounds: BarBounds,
    fill: crate::ir::Color,
    radius: Option<BarBorderRadius>,
    base_side: BarSide,
    uniform_enabled: bool,
) -> Prim {
    const KAPPA: f64 = 0.5522847498307936;
    let BarBounds { x, y, w, h } = bounds;

    let (top_left, top_right, bottom_left, bottom_right) = match radius {
        Some(BarBorderRadius::Uniform(value)) if uniform_enabled => match base_side {
            BarSide::Top => (0.0, 0.0, value, value),
            BarSide::Right => (value, 0.0, value, 0.0),
            BarSide::Bottom => (value, value, 0.0, 0.0),
            BarSide::Left => (0.0, value, 0.0, value),
        },
        Some(BarBorderRadius::Uniform(_)) | None => (0.0, 0.0, 0.0, 0.0),
        Some(BarBorderRadius::Corners {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        }) => (
            if matches!(base_side, BarSide::Top | BarSide::Left) {
                0.0
            } else {
                top_left.unwrap_or(0.0)
            },
            if matches!(base_side, BarSide::Top | BarSide::Right) {
                0.0
            } else {
                top_right.unwrap_or(0.0)
            },
            if matches!(base_side, BarSide::Bottom | BarSide::Left) {
                0.0
            } else {
                bottom_left.unwrap_or(0.0)
            },
            if matches!(base_side, BarSide::Bottom | BarSide::Right) {
                0.0
            } else {
                bottom_right.unwrap_or(0.0)
            },
        ),
    };
    let max_radius = w.min(h).max(0.0) / 2.0;
    let clamp_radius = |value: f64| {
        if value.is_finite() {
            value.clamp(0.0, max_radius)
        } else {
            0.0
        }
    };
    let [tl, tr, br, bl] = [
        clamp_radius(top_left),
        clamp_radius(top_right),
        clamp_radius(bottom_right),
        clamp_radius(bottom_left),
    ];
    if tl == 0.0 && tr == 0.0 && br == 0.0 && bl == 0.0 {
        return Prim::Rect { x, y, w, h, fill };
    }

    let mut d = format!(
        "M {} {}",
        crate::num::fmt_num(x + tl),
        crate::num::fmt_num(y)
    );
    write!(
        d,
        " L {} {}",
        crate::num::fmt_num(x + w - tr),
        crate::num::fmt_num(y)
    )
    .unwrap();
    if tr > 0.0 {
        write!(
            d,
            " C {} {} {} {} {} {}",
            crate::num::fmt_num(x + w - tr + KAPPA * tr),
            crate::num::fmt_num(y),
            crate::num::fmt_num(x + w),
            crate::num::fmt_num(y + tr - KAPPA * tr),
            crate::num::fmt_num(x + w),
            crate::num::fmt_num(y + tr)
        )
        .unwrap();
    }
    write!(
        d,
        " L {} {}",
        crate::num::fmt_num(x + w),
        crate::num::fmt_num(y + h - br)
    )
    .unwrap();
    if br > 0.0 {
        write!(
            d,
            " C {} {} {} {} {} {}",
            crate::num::fmt_num(x + w),
            crate::num::fmt_num(y + h - br + KAPPA * br),
            crate::num::fmt_num(x + w - br + KAPPA * br),
            crate::num::fmt_num(y + h),
            crate::num::fmt_num(x + w - br),
            crate::num::fmt_num(y + h)
        )
        .unwrap();
    }
    write!(
        d,
        " L {} {}",
        crate::num::fmt_num(x + bl),
        crate::num::fmt_num(y + h)
    )
    .unwrap();
    if bl > 0.0 {
        write!(
            d,
            " C {} {} {} {} {} {}",
            crate::num::fmt_num(x + bl - KAPPA * bl),
            crate::num::fmt_num(y + h),
            crate::num::fmt_num(x),
            crate::num::fmt_num(y + h - bl + KAPPA * bl),
            crate::num::fmt_num(x),
            crate::num::fmt_num(y + h - bl)
        )
        .unwrap();
    }
    write!(
        d,
        " L {} {}",
        crate::num::fmt_num(x),
        crate::num::fmt_num(y + tl)
    )
    .unwrap();
    if tl > 0.0 {
        write!(
            d,
            " C {} {} {} {} {} {}",
            crate::num::fmt_num(x),
            crate::num::fmt_num(y + tl - KAPPA * tl),
            crate::num::fmt_num(x + tl - KAPPA * tl),
            crate::num::fmt_num(y),
            crate::num::fmt_num(x + tl),
            crate::num::fmt_num(y)
        )
        .unwrap();
    }
    d.push_str(" Z");

    Prim::Path {
        d,
        fill: Some(fill),
        stroke: None,
        stroke_width: 0.0,
    }
}

/// 対数値軸では非正値を描画せず、線形軸では有限値をそのまま描画する。
fn is_renderable_value(value: f64, is_log: bool) -> bool {
    value.is_finite() && (!is_log || value > 0.0)
}

/// 縦棒1本のデータ矩形(ピクセル空間)。`series`=dataset index, `index`=category index。
/// `value` はラベル描画用に元値を保持する(geometry には出力しない)。
#[derive(Debug, Clone, PartialEq)]
pub struct BarBox {
    pub series: usize,
    pub index: usize,
    pub value: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HorizontalBarBox {
    pub series: usize,
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct HorizontalBarLayout {
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
    pub value_ticks: crate::scale::NiceTicks,
    pub value_scale: crate::scale::ValueScale,
    pub bars: Vec<HorizontalBarBox>,
}

/// Computes the pixel bounds for one dataset's category-axis bar slot.
///
/// Numeric `barThickness` defines a fixed slot and ignores both percentage options. `flex` uses
/// the neighboring category interval; category positions in this renderer are evenly spaced, so
/// that interval is the same as `category_size`.
pub(crate) fn category_bar_bounds(
    center: f64,
    category_start: f64,
    category_size: f64,
    slot: usize,
    slot_count: usize,
    options: Option<BarGeometryOptions>,
    legacy_geometry: bool,
) -> (f64, f64) {
    let options = options.unwrap_or_default();
    let slot_count = slot_count.max(1) as f64;
    let category_percentage = options
        .category_percentage
        .filter(|value| value.is_finite())
        .unwrap_or(DEFAULT_CATEGORY_PERCENTAGE)
        .max(0.0);
    let bar_percentage = options
        .bar_percentage
        .filter(|value| value.is_finite())
        .unwrap_or(DEFAULT_BAR_PERCENTAGE)
        .max(0.0);
    let max_bar_thickness = options
        .max_bar_thickness
        .filter(|value| value.is_finite())
        .unwrap_or(f64::INFINITY)
        .max(0.0);

    let (slot_size, fill_ratio) = match options.bar_thickness {
        Some(BarThickness::Pixels(value)) => (
            if value.is_finite() {
                value.clamp(0.0, MAX_BAR_THICKNESS)
            } else {
                0.0
            },
            1.0,
        ),
        Some(BarThickness::Flex) | None => (
            (category_size.max(0.0) * category_percentage).min(MAX_BAR_THICKNESS) / slot_count,
            bar_percentage,
        ),
    };
    let slot_center = center - slot_size * slot_count / 2.0 + slot_size * (slot as f64 + 0.5);
    let natural_width = (slot_size * fill_ratio).min(MAX_BAR_THICKNESS);
    let width = natural_width.min(max_bar_thickness).max(0.0);
    // Keep existing renderer output only when every dataset leaves geometry options unset.
    // Once any dataset opts in, all bars use Chart.js's centered placement so shared stack and
    // dodge slots stay aligned.
    let legacy_default = legacy_geometry && natural_width <= max_bar_thickness;
    let left = if legacy_default {
        category_start + category_size * DEFAULT_CATEGORY_PADDING + slot_size * slot as f64
    } else {
        slot_center - width / 2.0
    };
    (left, width)
}

/// Applies Chart.js `minBarLength` in pixel space while keeping both endpoints inside the value
/// axis span. `base` and `head` are ordered by data direction, including negative stacked bars.
/// `positive_direction` is -1 for a vertical value axis and +1 for a horizontal one.
pub(crate) fn enforce_min_bar_length(
    mut base: f64,
    mut head: f64,
    value: f64,
    minimum: Option<f64>,
    positive_direction: f64,
    pixel_start: f64,
    pixel_end: f64,
) -> (f64, f64) {
    let Some(minimum) = minimum
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, (pixel_end - pixel_start).abs()))
    else {
        return (base, head);
    };
    if (head - base).abs() >= minimum {
        return (base, head);
    }

    let range_min = pixel_start.min(pixel_end);
    let range_max = pixel_start.max(pixel_end);
    let direction = positive_direction * if value >= 0.0 { 1.0 } else { -1.0 };
    if value == 0.0 {
        let length = minimum.min(range_max - range_min);
        let anchor = base.clamp(range_min, range_max);
        base = anchor - direction * length / 2.0;
        head = anchor + direction * length / 2.0;

        // Shift a centered zero bar into the plot at an edge without changing its length.
        if base.min(head) < range_min {
            let shift = range_min - base.min(head);
            base += shift;
            head += shift;
        }
        if base.max(head) > range_max {
            let shift = base.max(head) - range_max;
            base -= shift;
            head -= shift;
        }
        return (base, head);
    }

    base = base.clamp(range_min, range_max);
    let available = if direction > 0.0 {
        range_max - base
    } else {
        base - range_min
    }
    .max(0.0);
    let length = minimum.min(available);
    head = base + direction * length;
    (base, head)
}

pub(crate) fn min_bar_length_for_visible_interval(
    minimum: Option<f64>,
    start: f64,
    end: f64,
    ticks: &crate::scale::NiceTicks,
) -> Option<f64> {
    super::common::axis_interval_intersects_range(start, end, ticks)
        .then_some(minimum)
        .flatten()
}

struct StackedMinBarLength<'a> {
    minimum: Option<f64>,
    positive_direction: f64,
    zero_direction: f64,
    pixel_start: f64,
    pixel_end: f64,
    scale: &'a crate::scale::ValueScale,
}

fn enforce_stacked_min_bar_length(
    base: f64,
    head: f64,
    value: f64,
    context: StackedMinBarLength<'_>,
) -> (f64, f64, f64) {
    let minimum = context
        .minimum
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, (context.pixel_end - context.pixel_start).abs()));
    let minimum_applied = minimum.is_some_and(|minimum| (head - base).abs() < minimum);
    let positive_direction = if value == 0.0 {
        context.zero_direction
    } else {
        context.positive_direction
    };
    let (base, head) = enforce_min_bar_length(
        base,
        head,
        value,
        minimum,
        positive_direction,
        context.pixel_start,
        context.pixel_end,
    );
    // Chart.js replaces the stack's visual value with the post-minimum pixel span converted
    // back through the value scale, so following datasets start at the rendered endpoint.
    let visual_value = if minimum_applied {
        context.scale.unmap(head) - context.scale.unmap(base)
    } else {
        value
    };
    (base, head, visual_value)
}

/// 縦棒の全データ矩形を build_vertical と同一の式で算出する単一の真実源。
/// レンダラ(`build_vertical`)とモデル(`model::Geometry`)の両方がこれを呼ぶ。
/// 非積み上げ (dodge): category 外側 × series 内側で有限値のみ box を生成する。
///   欠損値 (get() None) と非有限値 (NaN / ±∞) は skip され、box は emit されない。
/// 非積み上げ・積み上げともに hard y bound で各端点を clip し、描画と geometry の範囲を
/// 揃える。
pub fn vertical_bar_boxes(spec: &ChartSpec, frame: &super::common::Frame) -> Vec<BarBox> {
    let n = spec.categories.len().max(1);
    let is_log = spec.y_axis.scale_kind == crate::ir::ScaleKind::Logarithmic;
    let (stack_groups, stack_group_count) = super::common::stack_group_indices(&spec.series);
    let legacy_geometry = matches!(spec.x_positions, crate::ir::XPositions::Category)
        && spec.series.iter().all(|series| {
            series
                .bar_geometry
                .is_none_or(|geometry| !geometry.has_geometry_controls())
        });
    let s = spec.series.len().max(1);
    let placement_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            placement_stacked: true,
            ..
        }
    );
    let slot_count = if placement_stacked {
        stack_group_count.max(1)
    } else {
        s
    };
    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let baseline_y = frame.ys.map(base_v);
    let value_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        }
    );

    let mut boxes = Vec::new();
    if placement_stacked && value_stacked {
        // stack ID ごとに並列の列を置き、各列の中で値を正負別に累積する。
        for i in 0..spec.categories.len() {
            let (center, band_left, band_w) = super::common::x_index_band(spec, frame, i, n);
            // Raw sums preserve data stacking. Visual offsets separately carry minBarLength
            // expansion into the starting value of each following segment.
            let mut pos_acc = vec![0.0_f64; stack_group_count];
            let mut neg_acc = vec![0.0_f64; stack_group_count];
            let mut pos_visual_offsets = vec![0.0_f64; stack_group_count];
            let mut neg_visual_offsets = vec![0.0_f64; stack_group_count];
            for (sidx, ser) in spec.series.iter().enumerate() {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !is_renderable_value(v, is_log) {
                    continue;
                }
                let stack_group = stack_groups[sidx];
                let (bx, bar_width) = category_bar_bounds(
                    center,
                    band_left,
                    band_w,
                    stack_group,
                    slot_count,
                    ser.bar_geometry,
                    legacy_geometry,
                );
                let (v0, v1) = if v > 0.0 {
                    let lo = pos_acc[stack_group];
                    pos_acc[stack_group] += v;
                    let sum = pos_acc[stack_group];
                    (lo, sum)
                } else if v < 0.0 {
                    let hi = neg_acc[stack_group];
                    neg_acc[stack_group] += v;
                    (neg_acc[stack_group], hi)
                } else {
                    let total = pos_acc[stack_group]
                        + neg_acc[stack_group]
                        + pos_visual_offsets[stack_group]
                        + neg_visual_offsets[stack_group];
                    (total, total)
                };
                let (visible_start, visible_end) = if v == 0.0 {
                    let raw_total = pos_acc[stack_group] + neg_acc[stack_group];
                    (raw_total, raw_total)
                } else {
                    (v0, v1)
                };
                let (mut base_v, mut head_v) = if v > 0.0 { (v0, v1) } else { (v1, v0) };
                let visual_offset = if v > 0.0 {
                    pos_visual_offsets[stack_group]
                } else if v < 0.0 {
                    neg_visual_offsets[stack_group]
                } else {
                    0.0
                };
                base_v += visual_offset;
                head_v += visual_offset;
                let base = frame
                    .ys
                    .map(super::common::clip_axis_value(base_v, &frame.ticks));
                let head = frame
                    .ys
                    .map(super::common::clip_axis_value(head_v, &frame.ticks));
                let (base, head, visual_value) = enforce_stacked_min_bar_length(
                    base,
                    head,
                    v,
                    StackedMinBarLength {
                        minimum: min_bar_length_for_visible_interval(
                            ser.bar_geometry
                                .and_then(|geometry| geometry.min_bar_length),
                            visible_start,
                            visible_end,
                            &frame.ticks,
                        ),
                        positive_direction: -1.0,
                        zero_direction: if frame.ticks.min >= 0.0 { -1.0 } else { 1.0 },
                        pixel_start: frame.plot_top,
                        pixel_end: frame.plot_bottom,
                        scale: &frame.ys,
                    },
                );
                if v > 0.0 {
                    pos_visual_offsets[stack_group] += visual_value - v;
                } else if v < 0.0 {
                    neg_visual_offsets[stack_group] += visual_value - v;
                } else if visual_value > 0.0 {
                    pos_visual_offsets[stack_group] += visual_value;
                } else if visual_value < 0.0 {
                    neg_visual_offsets[stack_group] += visual_value;
                }
                let y_top = base.min(head);
                let h = (head - base).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: bar_width,
                    h,
                });
            }
        }
    } else if placement_stacked {
        // stack ID ごとのスロットに、各系列を baseline から重ねて描く。
        // 値域は dodge と同じ個別値(value_stacked=false)。
        for i in 0..spec.categories.len() {
            let (center, band_left, band_w) = super::common::x_index_band(spec, frame, i, n);
            for (sidx, ser) in spec.series.iter().enumerate() {
                let stack_group = stack_groups[sidx];
                let (bx, bar_width) = category_bar_bounds(
                    center,
                    band_left,
                    band_w,
                    stack_group,
                    slot_count,
                    ser.bar_geometry,
                    legacy_geometry,
                );
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !is_renderable_value(v, is_log) {
                    continue;
                }
                let vy = frame
                    .ys
                    .map(super::common::clip_axis_value(v, &frame.ticks));
                let (base, head) = enforce_min_bar_length(
                    baseline_y,
                    vy,
                    v,
                    min_bar_length_for_visible_interval(
                        ser.bar_geometry
                            .and_then(|geometry| geometry.min_bar_length),
                        0.0,
                        v,
                        &frame.ticks,
                    ),
                    -1.0,
                    frame.plot_top,
                    frame.plot_bottom,
                );
                let y_top = base.min(head);
                let h = (head - base).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: bar_width,
                    h,
                });
            }
        }
    } else {
        // dodge 配置(従来の stacked=false の挙動)
        // value_stacked=true のとき値域は value_domain が担当するため geometry は変わらない。
        // 非有限値(null→NaN も含む)はギャップとしてスキップ。
        for i in 0..spec.categories.len() {
            let (center, band_left, band_w) = super::common::x_index_band(spec, frame, i, n);
            for (sidx, ser) in spec.series.iter().enumerate() {
                let (bx, bar_width) = category_bar_bounds(
                    center,
                    band_left,
                    band_w,
                    sidx,
                    slot_count,
                    ser.bar_geometry,
                    legacy_geometry,
                );
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !is_renderable_value(v, is_log) {
                    continue;
                }
                let vy = frame
                    .ys
                    .map(super::common::clip_axis_value(v, &frame.ticks));
                let (base, head) = enforce_min_bar_length(
                    baseline_y,
                    vy,
                    v,
                    min_bar_length_for_visible_interval(
                        ser.bar_geometry
                            .and_then(|geometry| geometry.min_bar_length),
                        0.0,
                        v,
                        &frame.ticks,
                    ),
                    -1.0,
                    frame.plot_top,
                    frame.plot_bottom,
                );
                let y_top = base.min(head);
                let h = (head - base).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: bar_width,
                    h,
                });
            }
        }
    }
    boxes
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    match spec.kind {
        crate::ir::ChartKind::Bar {
            horizontal: true, ..
        } => build_horizontal(spec, m),
        _ => build_vertical(spec, m),
    }
}

/// TextMeasurer が受け取れる有限なフォントサイズへ正規化する。
fn finite_measure_font_size(font_size: f64) -> f32 {
    if font_size.is_nan() {
        0.0
    } else if font_size.is_finite() {
        font_size.clamp(0.0, f32::MAX as f64) as f32
    } else if font_size.is_sign_positive() {
        f32::MAX
    } else {
        0.0
    }
}

/// 横棒レイアウト用の文字幅。非有限の計測結果は境界計算へ伝播させない。
fn finite_text_width(m: &TextMeasurer, text: &str, font_size: f64) -> f64 {
    let width = m.width(text, finite_measure_font_size(font_size));
    if width.is_finite() {
        (width as f64).max(0.0)
    } else {
        0.0
    }
}

/// 横棒の左右凡例帯幅。巨大な fontSize でも有限な境界を返す。
fn horizontal_legend_band_width(
    m: &TextMeasurer,
    names: &[String],
    font_size: f64,
    options: &crate::ir::LegendOptions,
) -> f64 {
    crate::layout::common::legend_band_width_vertical_styled(m, names, font_size, options)
}

/// 横棒の値軸端ラベル用のプロット境界を算出する。
///
/// 端のラベルは中央寄せなので、左端と右端を別々に余白化する。右端は最後の
/// tick の幅だけを使い、左端はラベルが canvas の左端を越える場合にだけ補う。
/// 基準境界は canvas 内へ正規化し、余白が大きすぎる有限値では比例縮小する。
/// LinearScale が全値を同一点へ写すのを防ぐため、最低限のプロット幅を残す。
/// 軸の実際のラベル形式で幅を測る。とくに log 軸を `fmt_num`(小数2桁丸め)で
/// 測ると、1e-15 のような端ラベルが実際より短く見積もられ、はみ出す。
struct HorizontalTickLabels<'a> {
    axis: &'a crate::ir::AxisSpec,
    temporal_ticks: Option<&'a [crate::temporal::TemporalTick]>,
}

fn horizontal_plot_bounds(
    base_left: f64,
    base_right: f64,
    canvas_width: f64,
    ticks: &[f64],
    m: &TextMeasurer,
    label_font: f64,
    labels: HorizontalTickLabels<'_>,
) -> (f64, f64) {
    let canvas_width = if canvas_width.is_finite() {
        canvas_width.max(MIN_HORIZONTAL_PLOT_WIDTH)
    } else {
        MIN_HORIZONTAL_PLOT_WIDTH
    };
    let mut base_left = if base_left.is_finite() {
        base_left
    } else if base_left.is_sign_positive() {
        canvas_width
    } else {
        0.0
    };
    let mut base_right = if base_right.is_finite() {
        base_right
    } else if base_right.is_sign_positive() {
        canvas_width
    } else {
        0.0
    };
    base_left = base_left.clamp(0.0, canvas_width - MIN_HORIZONTAL_PLOT_WIDTH);
    base_right = base_right.clamp(0.0, canvas_width);
    if base_right - base_left < MIN_HORIZONTAL_PLOT_WIDTH {
        base_right = base_left + MIN_HORIZONTAL_PLOT_WIDTH;
        if base_right > canvas_width {
            base_right = canvas_width;
            base_left = (base_right - MIN_HORIZONTAL_PLOT_WIDTH).max(0.0);
        }
    }
    let half_tick_width = |index: usize, tick: f64| {
        let label = labels
            .temporal_ticks
            .and_then(|ticks| ticks.get(index))
            .map(|tick| tick.label.clone())
            .unwrap_or_else(|| crate::layout::common::format_axis_tick(labels.axis, tick));
        finite_text_width(m, &label, label_font) / 2.0
    };
    let left_pad = ticks
        .first()
        .map(|&tick| (half_tick_width(0, tick) - base_left).max(0.0))
        .unwrap_or(0.0);
    let right_pad = ticks
        .last()
        .map(|&tick| half_tick_width(ticks.len() - 1, tick))
        .unwrap_or(0.0);
    let available_width = (base_right - base_left).max(0.0);
    let max_edge_padding = (available_width - MIN_HORIZONTAL_PLOT_WIDTH).max(0.0);
    let edge_padding = left_pad + right_pad;
    let scale = if edge_padding > max_edge_padding && edge_padding > 0.0 {
        max_edge_padding / edge_padding
    } else {
        1.0
    };
    let plot_left = base_left + left_pad * scale;
    let plot_right = (base_right - right_pad * scale).max(plot_left);
    (plot_left, plot_right)
}

fn build_vertical(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    use super::common::{LABEL_GAP, value_label};
    use crate::scene::Anchor;

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let frame = super::common::compute(spec, m);

    let mut items: Vec<Prim> = Vec::new();
    super::common::draw_frame(&mut items, spec, &frame, m);

    // bar 本体: 矩形は共有 vertical_bar_boxes(単一真実源)から、値ラベルは box から導出。
    let placement_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            placement_stacked: true,
            ..
        }
    );
    let value_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        }
    );
    let stacked = placement_stacked && value_stacked;
    let is_log = spec.y_axis.scale_kind == crate::ir::ScaleKind::Logarithmic;
    let positive_moves_up = frame.ys.map(frame.ticks.max) < frame.ys.map(frame.ticks.min);
    let (stack_groups, _) = super::common::stack_group_indices(&spec.series);
    let bar_boxes = vertical_bar_boxes(spec, &frame);
    for b in &bar_boxes {
        let ser = &spec.series[b.series];
        let base_side = if (b.value >= 0.0) == positive_moves_up {
            BarSide::Bottom
        } else {
            BarSide::Top
        };
        let has_later_same_sign = stacked
            && bar_boxes.iter().any(|next| {
                next.series > b.series
                    && next.index == b.index
                    && next.h > 0.0
                    && stack_groups[next.series] == stack_groups[b.series]
                    && is_renderable_value(next.value, is_log)
                    && next.value.signum() == b.value.signum()
            });
        items.push(bar_primitive(
            BarBounds {
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
            },
            ser.fill_at(b.index),
            ser.bar_geometry.and_then(|geometry| geometry.border_radius),
            base_side,
            !has_later_same_sign,
        ));
        if !spec.data_labels
            || b.h <= 0.0
            || !super::common::axis_value_in_bounds(b.value, &frame.ticks)
        {
            continue;
        }
        let cx = b.x + b.w / 2.0;
        if stacked {
            // セグメント中央(box 中心)に値ラベル。b.y/b.h は既に ys で写像済みの
            // ピクセル空間なので、ys が線形か対数(非アフィン)かに関わらず、
            // ピクセル空間で平均するこの中点計算は常に正しい(値空間で先に
            // 中点を取ってから map する横棒側の旧実装は対数軸で誤っていた)。
            let mid_y = b.y + b.h / 2.0;
            items.push(value_label(
                cx,
                mid_y + label_font * super::common::TEXT_BASELINE_RATIO,
                label_font,
                Anchor::Middle,
                ink,
                b.value,
                is_log,
            ));
        } else {
            // 正は上端の少し上(- LABEL_GAP)、負は下端の下にラベル。負側は
            // LABEL_GAP ではなく + label_font(≒1行高)を足すのは、SVG の y が
            // ベースラインで字面が上に伸びるため、僅かな隙間だと棒下端に重なるから。
            // この上下非対称(- LABEL_GAP / + label_font)は意図的。
            let label_y = if b.value >= 0.0 {
                b.y - LABEL_GAP
            } else {
                b.y + b.h + label_font
            };
            items.push(value_label(
                cx,
                label_y,
                label_font,
                Anchor::Middle,
                ink,
                b.value,
                is_log,
            ));
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// 横棒(indexAxis:"y"): 値軸=X(左→右非反転)、カテゴリ軸=Y(上→下)。
/// 縦向き前提の common::compute/draw_frame は使わず、転置レイアウトを自前で描く。
fn build_horizontal(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    build_horizontal_with_geometry(spec, m).0
}

pub(crate) fn horizontal_bar_layout(spec: &ChartSpec, m: &TextMeasurer) -> HorizontalBarLayout {
    build_horizontal_with_geometry(spec, m).1
}

pub(crate) fn horizontal_bar_layout_with_temporal_values(
    spec: &ChartSpec,
    m: &TextMeasurer,
    temporal_values: &[i64],
) -> HorizontalBarLayout {
    build_horizontal_with_geometry_using_temporal_values(spec, m, Some(temporal_values)).1
}

pub(crate) fn build_horizontal_with_temporal_values(
    spec: &ChartSpec,
    m: &TextMeasurer,
    temporal_values: &[i64],
) -> Scene {
    build_horizontal_with_geometry_using_temporal_values(spec, m, Some(temporal_values)).0
}

pub(crate) fn horizontal_category_bands(
    spec: &ChartSpec,
    plot_top: f64,
    plot_bottom: f64,
) -> Vec<(f64, f64)> {
    use crate::ir::XPositions;
    use crate::layout::common::{
        temporal_index_domain, temporal_position_band, temporal_position_band_width,
    };
    use crate::temporal::TemporalScale;

    let count = spec.categories.len().max(1);
    let fallback = (plot_bottom - plot_top) / count as f64;
    match &spec.y_positions {
        XPositions::Category => (0..spec.categories.len())
            .map(|index| (plot_top + (index as f64 + 0.5) * fallback, fallback))
            .collect(),
        XPositions::Temporal { unix_millis } => {
            let (min, max) = temporal_index_domain(unix_millis, &spec.y_axis, true);
            let scale = TemporalScale::with_domain(
                spec.y_axis.scale_kind,
                unix_millis,
                min,
                max,
                plot_top,
                plot_bottom,
            );
            let band_width = temporal_position_band_width(
                unix_millis,
                &scale,
                spec.categories.len(),
                plot_top,
                plot_bottom,
            );
            (0..spec.categories.len())
                .map(|index| {
                    let (center, _, height) = temporal_position_band(
                        unix_millis,
                        &scale,
                        index,
                        spec.categories.len(),
                        plot_top,
                        plot_bottom,
                        band_width,
                    );
                    (center, height)
                })
                .collect()
        }
    }
}

fn build_horizontal_with_geometry(
    spec: &ChartSpec,
    m: &TextMeasurer,
) -> (Scene, HorizontalBarLayout) {
    build_horizontal_with_geometry_using_temporal_values(spec, m, None)
}

fn build_horizontal_with_geometry_using_temporal_values(
    spec: &ChartSpec,
    m: &TextMeasurer,
    temporal_values: Option<&[i64]>,
) -> (Scene, HorizontalBarLayout) {
    use crate::ir::{ScaleKind, XPositions};
    use crate::layout::common::*;
    use crate::scale::{LinearScale, NiceTicks, ValueScale};
    use crate::scene::Anchor;
    use crate::temporal::TemporalScale;

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;
    let legacy_geometry = matches!(spec.y_positions, XPositions::Category)
        && spec.series.iter().all(|series| {
            series
                .bar_geometry
                .is_none_or(|geometry| !geometry.has_geometry_controls())
        });

    // 横棒は値軸が x のため x_axis を渡す（begin_at_zero/suggested も x_axis から読む）。
    let (dmin, dmax) = value_domain(spec, &spec.x_axis);
    let is_log = spec.x_axis.scale_kind == ScaleKind::Logarithmic;
    let is_temporal_x = is_temporal_scale(&spec.x_axis);
    let x_temporal_ticks = if is_temporal_x {
        temporal_axis_ticks(&spec.x_axis, dmin as i64, dmax as i64, spec.width)
    } else {
        Vec::new()
    };
    let (ticks, minor_ticks) = if is_temporal_x {
        (
            NiceTicks {
                min: dmin,
                max: dmax,
                step: 0.0,
                ticks: x_temporal_ticks
                    .iter()
                    .map(|tick| tick.unix_millis as f64)
                    .collect(),
            },
            Vec::new(),
        )
    } else if is_log {
        let log = crate::scale::log_ticks_within(dmin, dmax);
        (
            NiceTicks {
                min: log.min,
                max: log.max,
                // 対数軸では decade 間隔が一定でない(1,10,100,...)ため "step" は
                // 意味を持たない。0.0 は Task 9(common.rs::compute())と同じ log 専用の
                // 番兵(nice_ticks は常に step>0 を返す)。
                step: 0.0,
                ticks: log.major,
            },
            log.minor,
        )
    } else {
        (configured_axis_ticks(dmin, dmax, &spec.x_axis), Vec::new())
    };

    let y_temporal_positions = match &spec.y_positions {
        XPositions::Temporal { unix_millis } => Some(unix_millis.as_slice()),
        XPositions::Category => None,
    };
    let y_temporal_domain =
        y_temporal_positions.map(|positions| temporal_index_domain(positions, &spec.y_axis, true));
    let y_temporal_ticks = y_temporal_domain
        .map(|(min, max)| temporal_axis_ticks(&spec.y_axis, min, max, spec.height))
        .unwrap_or_default();

    // インデックス軸ラベル幅(左軸): category なら各ラベル、temporal なら表示 tick の最大幅。
    let mut max_cat_w = 0.0_f64;
    let index_labels = if y_temporal_positions.is_some() {
        y_temporal_ticks
            .iter()
            .map(|tick| tick.label.as_str())
            .collect::<Vec<_>>()
    } else {
        spec.categories.iter().map(String::as_str).collect()
    };
    for c in index_labels {
        let w = finite_text_width(m, c, label_font);
        if w > max_cat_w {
            max_cat_w = w;
        }
    }
    let cat_w = max_cat_w + 10.0;

    let legend_title = crate::layout::common::legend_title(spec);
    // 凡例の有無(Top/Bottom/Left/Right かつ名前付き系列またはタイトルあり)。
    let has_legend = matches!(
        spec.legend,
        crate::ir::LegendPos::Top
            | crate::ir::LegendPos::Bottom
            | crate::ir::LegendPos::Left
            | crate::ir::LegendPos::Right
    ) && (spec.series.iter().any(|s| !s.name.is_empty())
        || legend_title.is_some());

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_font = legend_label_font_size(&spec.legend_options, spec.theme.font_size);
    let legend_height = legend_horizontal_band_height(
        &spec.legend_options,
        spec.theme.font_size,
        legend_title.is_some(),
    );
    let legend_top = if has_legend && spec.legend == crate::ir::LegendPos::Top {
        legend_height
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == crate::ir::LegendPos::Bottom {
        legend_height
    } else {
        0.0
    };
    // Left/Right の凡例帯幅(系列名から算出)。
    let mut series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
    series_names.extend(legend_title.map(str::to_owned));
    let legend_left = if has_legend && spec.legend == crate::ir::LegendPos::Left {
        horizontal_legend_band_width(m, &series_names, legend_font, &spec.legend_options)
    } else {
        0.0
    };
    let legend_right = if has_legend && spec.legend == crate::ir::LegendPos::Right {
        horizontal_legend_band_width(m, &series_names, legend_font, &spec.legend_options)
    } else {
        0.0
    };

    // Y 軸タイトル(回転テキスト)の帯幅 / X 軸タイトルの帯高。title=None(既定)なら 0.0。
    let y_title_w = spec
        .y_axis
        .title
        .as_ref()
        .map(|t| t.font_size.unwrap_or(spec.theme.font_size * 1.1) + 6.0)
        .unwrap_or(0.0);
    let x_title_h = if spec.x_axis.title.is_some() {
        AXIS_TITLE_BAND
    } else {
        0.0
    };
    let base_left = OUTER_PAD + cat_w + y_title_w + legend_left;
    let (plot_left, plot_right) = horizontal_plot_bounds(
        base_left,
        spec.width - OUTER_PAD - legend_right,
        spec.width,
        &ticks.ticks,
        m,
        label_font,
        HorizontalTickLabels {
            axis: &spec.x_axis,
            temporal_ticks: is_temporal_x.then_some(x_temporal_ticks.as_slice()),
        },
    );
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;
    let y_temporal_scale =
        y_temporal_positions
            .zip(y_temporal_domain)
            .map(|(positions, (min, max))| {
                TemporalScale::with_domain(
                    spec.y_axis.scale_kind,
                    positions,
                    min,
                    max,
                    plot_top,
                    plot_bottom,
                )
            });
    let y_temporal_band_width =
        y_temporal_positions
            .zip(y_temporal_scale.as_ref())
            .map(|(positions, scale)| {
                temporal_position_band_width(
                    positions,
                    scale,
                    spec.categories.len(),
                    plot_top,
                    plot_bottom,
                )
            });

    // 値→X(非反転)。対数軸は log10 空間の LinearScale を内側に持つ ValueScale::Log。
    // ticks.min/max は log_ticks_within(dmin, dmax) の戻り値で、渡した tight
    // ドメイン(常に正、dmin < dmax)をそのまま折り返す(decade 境界には丸めない)。
    // chart.js 実機は log 軸のピクセル写像を tight データドメインでそのまま行う
    // (scale.min/max がそれ)ため、これに合わせる(PR #144 の自動レビュー P1 指摘)。
    let xs = if is_temporal_x {
        let values = temporal_values.map_or_else(
            || {
                spec.series
                    .iter()
                    .flat_map(|series| &series.values)
                    .filter(|value| value.is_finite() && value.abs() <= 8.64e15)
                    .map(|value| value.trunc() as i64)
                    .collect::<Vec<_>>()
            },
            <[i64]>::to_vec,
        );
        ValueScale::Temporal(TemporalScale::with_domain(
            spec.x_axis.scale_kind,
            &values,
            dmin as i64,
            dmax as i64,
            plot_left,
            plot_right,
        ))
    } else if is_log {
        ValueScale::Log {
            inner: LinearScale::new(ticks.min.log10(), ticks.max.log10(), plot_left, plot_right),
            floor: ticks.min,
        }
    } else {
        ValueScale::Linear(LinearScale::new(
            ticks.min, ticks.max, plot_left, plot_right,
        ))
    };

    let mut items: Vec<Prim> = Vec::new();
    let mut horizontal_bars = Vec::new();

    // 1. タイトル。
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // 2. 縦グリッド + 値ラベル(下)。x_axis.grid が値軸(=X)のグリッドを支配する。
    // display=false のとき Prim::Line を落とすが、値ラベルは常に残す。
    let x_grid_cfg = &spec.x_axis.grid;
    let x_grid_color = x_grid_cfg.color.unwrap_or(spec.theme.grid_color);
    for (index, &t) in ticks.ticks.iter().enumerate() {
        let x = xs.map(t);
        if x_grid_cfg.display {
            items.push(Prim::Line {
                x1: x,
                y1: plot_top,
                x2: x,
                y2: plot_bottom,
                stroke: x_grid_color,
                stroke_width: x_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
        items.push(Prim::Text {
            x,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: if is_temporal_x {
                axis_temporal_tick_label(&spec.x_axis, &x_temporal_ticks, index, t)
            } else if is_log {
                crate::num::fmt_num_log(t)
            } else {
                crate::num::fmt_axis_tick(t, spec.x_axis.ticks.format.as_ref())
            },
            rotate_deg: None,
        });
    }
    // 2b. 対数軸の minor グリッド(mantissa 2..9、ラベルなし)。線形軸では
    // minor_ticks が常に空なので no-op。major と同じ濃さだと decade 境界が
    // 埋もれるため、common.rs::draw_frame と同様に半透明で薄く描く。
    if x_grid_cfg.display {
        let minor_grid_color = crate::ir::Color {
            a: x_grid_color.a * 0.5,
            ..x_grid_color
        };
        for &t in &minor_ticks {
            let x = xs.map(t);
            items.push(Prim::Line {
                x1: x,
                y1: plot_top,
                x2: x,
                y2: plot_bottom,
                stroke: minor_grid_color,
                stroke_width: x_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }

    // 3. 底辺の値軸線(X のボーダー)。x_axis.border が水平線を支配する。
    let x_border = &spec.x_axis.border;
    if x_border.display {
        let border_color = x_border.color.unwrap_or(ink);
        items.push(Prim::Line {
            x1: plot_left,
            y1: plot_bottom,
            x2: plot_right,
            y2: plot_bottom,
            stroke: border_color,
            stroke_width: x_border.width,
            dash: x_border.dash.clone(),
        });
    }

    // 3a. 左軸線(カテゴリ軸=Y のボーダー)。y_axis.border が縦のカテゴリ軸線を支配する。
    let y_border = &spec.y_axis.border;
    if y_border.display {
        let border_color = y_border.color.unwrap_or(ink);
        items.push(Prim::Line {
            x1: plot_left,
            y1: plot_top,
            x2: plot_left,
            y2: plot_bottom,
            stroke: border_color,
            stroke_width: y_border.width,
            dash: y_border.dash.clone(),
        });
    }

    // 3c. tick 短線(値軸=X)。x_axis.grid.draw_ticks が true のとき plot_bottom から下方向へ。
    // 色は grid.color を継承(既定 ink)。カテゴリ軸(Y)側は Chart.js で通常 tick を描かないためスキップ。
    // 対数軸では minor_ticks(mantissa 2..9)にも同じ短線を描く(2b の minor グリッド線と
    // 1:1 対応させる。Task 9 で common.rs::compute() に施したのと同じ修正)。
    const TICK_LEN: f64 = 4.0;
    if x_grid_cfg.draw_ticks {
        let tick_color = x_grid_cfg.color.unwrap_or(ink);
        for &t in ticks.ticks.iter().chain(minor_ticks.iter()) {
            let x = xs.map(t);
            items.push(Prim::Line {
                x1: x,
                y1: plot_bottom,
                x2: x,
                y2: plot_bottom + TICK_LEN,
                stroke: tick_color,
                stroke_width: x_grid_cfg.line_width,
                dash: Vec::new(),
            });
        }
    }

    if let Some(scale) = &y_temporal_scale {
        let y_grid_cfg = &spec.y_axis.grid;
        let y_grid_color = y_grid_cfg.color.unwrap_or(spec.theme.grid_color);
        for tick in &y_temporal_ticks {
            let y = scale.map_millis(tick.unix_millis);
            if y_grid_cfg.display {
                items.push(Prim::Line {
                    x1: plot_left,
                    y1: y,
                    x2: plot_right,
                    y2: y,
                    stroke: y_grid_color,
                    stroke_width: y_grid_cfg.line_width,
                    dash: Vec::new(),
                });
            }
            items.push(Prim::Text {
                x: plot_left - 6.0,
                y: y + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::End,
                fill: ink,
                content: tick.label.clone(),
                rotate_deg: None,
            });
            if y_grid_cfg.draw_ticks {
                items.push(Prim::Line {
                    x1: plot_left - TICK_LEN,
                    y1: y,
                    x2: plot_left,
                    y2: y,
                    stroke: y_grid_color,
                    stroke_width: y_grid_cfg.line_width,
                    dash: Vec::new(),
                });
            }
        }
    }

    // 4. カテゴリ band と 横棒。
    let n = spec.categories.len().max(1);
    let band_h = (plot_bottom - plot_top) / n as f64;
    let s = spec.series.len().max(1);
    let (stack_groups, stack_group_count) = stack_group_indices(&spec.series);
    let placement_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            placement_stacked: true,
            ..
        }
    );
    let slot_count = if placement_stacked {
        stack_group_count.max(1)
    } else {
        s
    };

    let base_v = 0.0_f64.clamp(ticks.min, ticks.max);
    let baseline_x = xs.map(base_v);

    let value_stacked = matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        }
    );

    for i in 0..spec.categories.len() {
        let (band_top, center_y, band_h) =
            if let (Some(scale), Some(positions)) = (&y_temporal_scale, y_temporal_positions) {
                let (center, top, height) = temporal_position_band(
                    positions,
                    scale,
                    i,
                    n,
                    plot_top,
                    plot_bottom,
                    y_temporal_band_width.unwrap_or(band_h),
                );
                (top, center, height)
            } else {
                let top = plot_top + i as f64 * band_h;
                (top, top + band_h / 2.0, band_h)
            };

        // カテゴリラベル(左)。
        if y_temporal_positions.is_none() && !spec.categories[i].is_empty() {
            items.push(Prim::Text {
                x: plot_left - 6.0,
                y: center_y + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::End,
                fill: ink,
                content: spec.categories[i].clone(),
                rotate_deg: None,
            });
        }

        if placement_stacked && value_stacked {
            // stack ID ごとに平行なレーンを置き、各レーンの中で値を正負別に累積する。
            // Raw sums preserve data stacking; visual offsets carry minBarLength expansion
            // into the starting value of each following segment.
            let mut pos_acc = vec![0.0_f64; stack_group_count];
            let mut neg_acc = vec![0.0_f64; stack_group_count];
            let mut pos_visual_offsets = vec![0.0_f64; stack_group_count];
            let mut neg_visual_offsets = vec![0.0_f64; stack_group_count];
            let mut stacked_segments = Vec::new();
            for (series_index, ser) in spec.series.iter().enumerate() {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !is_renderable_value(v, is_log) {
                    continue;
                }
                let stack_group = stack_groups[series_index];
                let (by, bar_height) = category_bar_bounds(
                    center_y,
                    band_top,
                    band_h,
                    stack_group,
                    slot_count,
                    ser.bar_geometry,
                    legacy_geometry,
                );
                let (v0, v1) = if v > 0.0 {
                    let lo = pos_acc[stack_group];
                    pos_acc[stack_group] += v;
                    (lo, pos_acc[stack_group])
                } else if v < 0.0 {
                    let hi = neg_acc[stack_group];
                    neg_acc[stack_group] += v;
                    (neg_acc[stack_group], hi)
                } else {
                    let total = pos_acc[stack_group]
                        + neg_acc[stack_group]
                        + pos_visual_offsets[stack_group]
                        + neg_visual_offsets[stack_group];
                    (total, total)
                };
                let (visible_start, visible_end) = if v == 0.0 {
                    let raw_total = pos_acc[stack_group] + neg_acc[stack_group];
                    (raw_total, raw_total)
                } else {
                    (v0, v1)
                };
                let (mut base_v, mut head_v) = if v > 0.0 { (v0, v1) } else { (v1, v0) };
                let visual_offset = if v > 0.0 {
                    pos_visual_offsets[stack_group]
                } else if v < 0.0 {
                    neg_visual_offsets[stack_group]
                } else {
                    0.0
                };
                base_v += visual_offset;
                head_v += visual_offset;
                let base = xs.map(super::common::clip_axis_value(base_v, &ticks));
                let head = xs.map(super::common::clip_axis_value(head_v, &ticks));
                let (base, head, visual_value) = enforce_stacked_min_bar_length(
                    base,
                    head,
                    v,
                    StackedMinBarLength {
                        minimum: min_bar_length_for_visible_interval(
                            ser.bar_geometry
                                .and_then(|geometry| geometry.min_bar_length),
                            visible_start,
                            visible_end,
                            &ticks,
                        ),
                        positive_direction: 1.0,
                        zero_direction: if ticks.min >= 0.0 { 1.0 } else { -1.0 },
                        pixel_start: plot_left,
                        pixel_end: plot_right,
                        scale: &xs,
                    },
                );
                if v > 0.0 {
                    pos_visual_offsets[stack_group] += visual_value - v;
                } else if v < 0.0 {
                    neg_visual_offsets[stack_group] += visual_value - v;
                } else if visual_value > 0.0 {
                    pos_visual_offsets[stack_group] += visual_value;
                } else if visual_value < 0.0 {
                    neg_visual_offsets[stack_group] += visual_value;
                }
                let x = base.min(head);
                let w = (head - base).abs();
                stacked_segments.push(StackedHorizontalSegment {
                    series_index,
                    stack_group,
                    value: v,
                    base,
                    head,
                    bounds: BarBounds {
                        x,
                        y: by,
                        w,
                        h: bar_height,
                    },
                });
            }

            for segment in &stacked_segments {
                let ser = &spec.series[segment.series_index];
                horizontal_bars.push(HorizontalBarBox {
                    series: segment.series_index,
                    index: i,
                    x: segment.bounds.x,
                    y: segment.bounds.y,
                    w: segment.bounds.w,
                    h: segment.bounds.h,
                });
                let has_later_same_sign = stacked_segments.iter().any(|next| {
                    next.series_index > segment.series_index
                        && next.stack_group == segment.stack_group
                        && next.bounds.w > 0.0
                        && next.value.signum() == segment.value.signum()
                });
                items.push(bar_primitive(
                    segment.bounds,
                    ser.fill_at(i),
                    ser.bar_geometry.and_then(|geometry| geometry.border_radius),
                    if segment.base <= segment.head {
                        BarSide::Left
                    } else {
                        BarSide::Right
                    },
                    !has_later_same_sign,
                ));
                if spec.data_labels
                    && super::common::axis_value_in_bounds(segment.value, &ticks)
                    && segment.bounds.w > 0.0
                {
                    // セグメント中央(box 中心)に値ラベルを置く。base/head は既に xs で
                    // 写像済みのピクセル空間なので、ここで平均する(ピクセル空間の中点)。
                    // 値空間で (v0+v1)/2.0 を先に計算してから map すると、対数軸では
                    // log10 が非アフィンなためピクセル中点とズレる(線形軸ではアフィン
                    // 写像なので数学的に一致するが、対数軸では誤った位置になる)。
                    let mid_x = (segment.base + segment.head) / 2.0;
                    let label_y = segment.bounds.y
                        + segment.bounds.h / 2.0
                        + label_font * TEXT_BASELINE_RATIO;
                    items.push(value_label(
                        mid_x,
                        label_y,
                        label_font,
                        Anchor::Middle,
                        ink,
                        segment.value,
                        is_log,
                    ));
                }
            }
        } else if placement_stacked {
            // stack ID ごとのレーンへ配置し、各系列を baseline から描画する。
            for (series_index, ser) in spec.series.iter().enumerate() {
                let stack_group = stack_groups[series_index];
                let (by, bar_height) = category_bar_bounds(
                    center_y,
                    band_top,
                    band_h,
                    stack_group,
                    slot_count,
                    ser.bar_geometry,
                    legacy_geometry,
                );
                let cy = by + bar_height / 2.0 + label_font * TEXT_BASELINE_RATIO;
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !is_renderable_value(v, is_log) {
                    continue;
                }
                let vx = xs.map(super::common::clip_axis_value(v, &ticks));
                let (base, head) = enforce_min_bar_length(
                    baseline_x,
                    vx,
                    v,
                    min_bar_length_for_visible_interval(
                        ser.bar_geometry
                            .and_then(|geometry| geometry.min_bar_length),
                        0.0,
                        v,
                        &ticks,
                    ),
                    1.0,
                    plot_left,
                    plot_right,
                );
                let x = base.min(head);
                let w = (head - base).abs();
                let bounds = BarBounds {
                    x,
                    y: by,
                    w,
                    h: bar_height,
                };
                horizontal_bars.push(HorizontalBarBox {
                    series: series_index,
                    index: i,
                    x: bounds.x,
                    y: bounds.y,
                    w: bounds.w,
                    h: bounds.h,
                });
                items.push(bar_primitive(
                    bounds,
                    ser.fill_at(i),
                    ser.bar_geometry.and_then(|geometry| geometry.border_radius),
                    if base <= head {
                        BarSide::Left
                    } else {
                        BarSide::Right
                    },
                    true,
                ));
                if spec.data_labels && super::common::axis_value_in_bounds(v, &ticks) && w > 0.0 {
                    let (cx, anchor) = if v >= 0.0 {
                        (head + LABEL_GAP, Anchor::Start)
                    } else {
                        (head - LABEL_GAP, Anchor::End)
                    };
                    items.push(value_label(cx, cy, label_font, anchor, ink, v, is_log));
                }
            }
        } else {
            // dodge 配置(従来の stacked=false 挙動)
            // 非有限値(null→NaN も含む)はギャップとしてスキップ。
            for (sidx, ser) in spec.series.iter().enumerate() {
                let (by, bar_height) = category_bar_bounds(
                    center_y,
                    band_top,
                    band_h,
                    sidx,
                    slot_count,
                    ser.bar_geometry,
                    legacy_geometry,
                );
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !is_renderable_value(v, is_log) {
                    continue;
                }
                let vx = xs.map(super::common::clip_axis_value(v, &ticks));
                let (base, head) = enforce_min_bar_length(
                    baseline_x,
                    vx,
                    v,
                    min_bar_length_for_visible_interval(
                        ser.bar_geometry
                            .and_then(|geometry| geometry.min_bar_length),
                        0.0,
                        v,
                        &ticks,
                    ),
                    1.0,
                    plot_left,
                    plot_right,
                );
                let x = base.min(head);
                let w = (head - base).abs();
                let bounds = BarBounds {
                    x,
                    y: by,
                    w,
                    h: bar_height,
                };
                horizontal_bars.push(HorizontalBarBox {
                    series: sidx,
                    index: i,
                    x: bounds.x,
                    y: bounds.y,
                    w: bounds.w,
                    h: bounds.h,
                });
                items.push(bar_primitive(
                    bounds,
                    ser.fill_at(i),
                    ser.bar_geometry.and_then(|geometry| geometry.border_radius),
                    if base <= head {
                        BarSide::Left
                    } else {
                        BarSide::Right
                    },
                    true,
                ));
                if spec.data_labels && super::common::axis_value_in_bounds(v, &ticks) {
                    let cy = by + bar_height / 2.0 + label_font * TEXT_BASELINE_RATIO;
                    // 正は棒右端の右(Start)、負は左端の左(End)に LABEL_GAP 分離す。
                    let (lx, anchor) = if v >= 0.0 {
                        (head + LABEL_GAP, Anchor::Start)
                    } else {
                        (head - LABEL_GAP, Anchor::End)
                    };
                    items.push(value_label(lx, cy, label_font, anchor, ink, v, is_log));
                }
            }
        }
    }

    // 5. 凡例(Top/Bottom: common::draw_frame の配置を踏襲)。
    if has_legend
        && matches!(
            spec.legend,
            crate::ir::LegendPos::Top | crate::ir::LegendPos::Bottom
        )
    {
        let entries: Vec<(String, crate::ir::Color)> = spec
            .series
            .iter()
            .map(|series| (series.name.clone(), series.fill_at(0)))
            .collect();
        let legend_title = crate::layout::common::legend_title(spec);
        let legend_cy = if spec.legend == crate::ir::LegendPos::Top {
            OUTER_PAD + title_band + legend_height / 2.0
        } else {
            spec.height - OUTER_PAD - legend_height / 2.0
        };
        draw_horizontal_legend(
            &mut items,
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
    if has_legend
        && matches!(
            spec.legend,
            crate::ir::LegendPos::Left | crate::ir::LegendPos::Right
        )
    {
        let entries: Vec<(String, crate::ir::Color)> = spec
            .series
            .iter()
            .map(|s| (s.name.clone(), s.fill_at(0)))
            .collect();
        let band_x = if spec.legend == crate::ir::LegendPos::Left {
            OUTER_PAD
        } else {
            spec.width - OUTER_PAD - legend_right
        };
        draw_vertical_legend_styled(
            &mut items,
            &entries,
            crate::layout::common::legend_title(spec),
            band_x,
            plot_top,
            plot_bottom,
            ink,
            label_font,
            &spec.legend_options,
        );
    }

    // 6. Y 軸タイトル(-90deg 回転)。common::draw_frame と同じアンカー幾何:
    //   Start + -90deg → cy=plot_bottom(bottom-to-top 読みの起点)
    //   End   + -90deg → cy=plot_top
    if let Some(title) = &spec.y_axis.title {
        let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
        let color = title.color.unwrap_or(ink);
        let cy_center = (plot_top + plot_bottom) / 2.0;
        let (cy, anchor) = match title.align {
            crate::ir::AxisTitleAlign::Start => (plot_bottom, Anchor::Start),
            crate::ir::AxisTitleAlign::End => (plot_top, Anchor::End),
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

    // 7. X 軸タイトル(水平)。x ラベル帯のさらに下側に描く。
    // Chart.js の x 軸は Start=left / End=right。
    if let Some(title) = &spec.x_axis.title {
        let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
        let color = title.color.unwrap_or(ink);
        let (cx, anchor) = match title.align {
            crate::ir::AxisTitleAlign::Start => (plot_left, Anchor::Start),
            crate::ir::AxisTitleAlign::End => (plot_right, Anchor::End),
            crate::ir::AxisTitleAlign::Center => ((plot_left + plot_right) / 2.0, Anchor::Middle),
        };
        let y = plot_bottom + X_LABEL_BAND + font * 0.9;
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

    (
        Scene {
            width: spec.width,
            height: spec.height,
            items,
        },
        HorizontalBarLayout {
            plot_left,
            plot_right,
            plot_top,
            plot_bottom,
            value_ticks: ticks,
            value_scale: xs,
            bars: horizontal_bars,
        },
    )
}

#[cfg(test)]
mod geom_tests {
    use super::*;
    use crate::font::TEST_FONT as DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::text::TextMeasurer;

    fn boxes_for(json: &str) -> Vec<BarBox> {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        vertical_bar_boxes(&spec, &frame)
    }

    fn scene_for(json: &str) -> (ChartSpec, Scene) {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = super::build(&spec, &m);
        (spec, scene)
    }

    fn path_bounds(data: &str) -> (f64, f64, f64, f64) {
        let mut tokens = data.split_ascii_whitespace();
        let mut points = Vec::new();
        while let Some(command) = tokens.next() {
            let count = match command {
                "M" | "L" => 1,
                "C" => 3,
                "Z" => 0,
                _ => panic!("unexpected path command: {command}"),
            };
            for _ in 0..count {
                let x = tokens.next().unwrap().parse::<f64>().unwrap();
                let y = tokens.next().unwrap().parse::<f64>().unwrap();
                points.push((x, y));
            }
        }
        let min_x = points.iter().map(|(x, _)| *x).fold(f64::INFINITY, f64::min);
        let max_x = points
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = points.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
        let max_y = points
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max);
        (min_x, min_y, max_x, max_y)
    }

    #[test]
    fn temporal_vertical_bars_follow_irregular_timestamp_spacing() {
        let bars = boxes_for(
            r#"{"type":"bar","data":{"labels":["1970-01-01","1970-01-02","1970-01-05"],
            "datasets":[{"data":[1,2,3]}]},"options":{"scales":{"x":{"type":"time"}}}}"#,
        );

        assert_eq!(bars.len(), 3);
        let first_gap = bars[1].x + bars[1].w / 2.0 - (bars[0].x + bars[0].w / 2.0);
        let second_gap = bars[2].x + bars[2].w / 2.0 - (bars[1].x + bars[1].w / 2.0);
        assert!((second_gap / first_gap - 3.0).abs() < 1e-9);
        assert!((bars[1].w / bars[0].w - 1.0).abs() < 1e-9);
        assert!((bars[2].w / bars[0].w - 1.0).abs() < 1e-9);
    }

    #[test]
    fn vertical_bars_support_temporal_value_axis() {
        let json = r#"{"type":"bar","data":{"labels":["A","B","C"],
          "datasets":[{"data":["1970-01-02","1970-01-03","1970-01-05"]}]},
          "options":{"scales":{"y":{"type":"time","min":0,"max":345600000,
            "time":{"unit":"day","displayFormats":{"day":"%Y-%m-%d"}}}}}}"#;
        let (spec, scene) = scene_for(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let bars = vertical_bar_boxes(&spec, &frame);

        assert_eq!(bars.len(), 3);
        assert!((bars[1].h / bars[0].h - 2.0).abs() < 1e-9);
        assert!((bars[2].h / bars[0].h - 4.0).abs() < 1e-9);
        assert!(scene.items.iter().any(|item| matches!(item,
            crate::scene::Prim::Text { content, .. } if content == "1970-01-05")));
    }

    #[test]
    fn horizontal_bars_support_temporal_index_and_value_axes() {
        let json = r#"{"type":"bar","data":{"labels":["1970-01-01","1970-01-02","1970-01-05"],
          "datasets":[{"data":["1970-01-02","1970-01-03","1970-01-05"]}]},
          "options":{"indexAxis":"y","scales":{
            "x":{"type":"time","min":0,"max":345600000,"time":{"unit":"day","displayFormats":{"day":"%Y-%m-%d"}}},
            "y":{"type":"time","time":{"unit":"day","displayFormats":{"day":"%Y-%m-%d"}}}}}}"#;
        let (spec, scene) = scene_for(json);
        let bars = scene
            .items
            .iter()
            .filter_map(|item| match item {
                crate::scene::Prim::Rect { x, y, w, h, .. } => Some((*x, *y, *x + *w, *y + *h)),
                crate::scene::Prim::Path { d, .. } => Some(path_bounds(d)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(bars.len(), 3);
        let centers = bars
            .iter()
            .map(|(_, top, _, bottom)| (top + bottom) / 2.0)
            .collect::<Vec<_>>();
        let first_gap = centers[1] - centers[0];
        let second_gap = centers[2] - centers[1];
        assert!((second_gap / first_gap - 3.0).abs() < 1e-9);
        let first_height = bars[0].3 - bars[0].1;
        assert!(((bars[1].3 - bars[1].1) / first_height - 1.0).abs() < 1e-9);
        assert!(((bars[2].3 - bars[2].1) / first_height - 1.0).abs() < 1e-9);
        assert!(matches!(
            spec.y_positions,
            crate::ir::XPositions::Temporal { .. }
        ));
        assert!(scene.items.iter().any(|item| matches!(item,
            crate::scene::Prim::Text { content, .. } if content == "1970-01-02")));
        assert!(scene.items.iter().any(|item| matches!(item,
            crate::scene::Prim::Text { content, .. } if content == "1970-01-05")));
    }

    #[test]
    fn bar_primitive_rounds_only_value_end_corners() {
        for (value, expected_commands) in [
            (4, vec!["M", "L", "C", "L", "L", "L", "C", "Z"]),
            (-4, vec!["M", "L", "L", "C", "L", "C", "L", "Z"]),
        ] {
            let json = format!(
                r#"{{"type":"bar","data":{{"labels":["A"],"datasets":[{{"data":[{value}],"borderRadius":3}}]}}}}"#
            );
            let (spec, scene) = scene_for(&json);
            let fill = spec.series[0].fill_at(0);
            let path = scene
                .items
                .iter()
                .find_map(|prim| match prim {
                    Prim::Path {
                        d,
                        fill: Some(path_fill),
                        ..
                    } if *path_fill == fill => Some(d),
                    _ => None,
                })
                .expect("bar radius should produce a path");
            let commands: Vec<_> = path
                .split_ascii_whitespace()
                .filter(|token| matches!(*token, "M" | "L" | "C" | "Z"))
                .collect();
            assert_eq!(commands, expected_commands, "value={value}: {path}");
        }

        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[4],"borderRadius":{"topLeft":3}}]}}"#,
        );
        let fill = spec.series[0].fill_at(0);
        let path = scene
            .items
            .iter()
            .find_map(|prim| match prim {
                Prim::Path {
                    d,
                    fill: Some(path_fill),
                    ..
                } if *path_fill == fill => Some(d),
                _ => None,
            })
            .expect("one rounded corner should produce a path");
        assert_eq!(
            path.split_ascii_whitespace()
                .filter(|token| *token == "C")
                .count(),
            1,
            "only topLeft is rounded: {path}"
        );
    }

    #[test]
    fn bar_primitive_clamps_radii_and_keeps_zero_square() {
        for radius in ["0", "-5"] {
            let json = format!(
                r#"{{"type":"bar","data":{{"labels":["A"],"datasets":[{{"data":[4],"borderRadius":{radius}}}]}}}}"#
            );
            let (spec, scene) = scene_for(&json);
            let fill = spec.series[0].fill_at(0);
            assert!(scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Rect { fill: rect_fill, .. } if *rect_fill == fill
            )));
            assert!(!scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Path { fill: Some(path_fill), .. } if *path_fill == fill
            )));
        }

        let oversized = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[4],"borderRadius":10000}]}}"#;
        let (spec, scene) = scene_for(oversized);
        let fill = spec.series[0].fill_at(0);
        let path = scene
            .items
            .iter()
            .find_map(|prim| match prim {
                Prim::Path {
                    d,
                    fill: Some(path_fill),
                    ..
                } if *path_fill == fill => Some(d),
                _ => None,
            })
            .expect("oversized radius should be clamped into a path");
        let bar = boxes_for(oversized).pop().unwrap();
        let coordinates: Vec<_> = path
            .split_ascii_whitespace()
            .filter_map(|token| token.parse::<f64>().ok())
            .collect();
        assert_eq!(coordinates.len() % 2, 0, "path coordinates must be pairs");
        for (index, coordinate) in coordinates.into_iter().enumerate() {
            assert!(coordinate.is_finite(), "{path}");
            let (min, max) = if index % 2 == 0 {
                (bar.x, bar.x + bar.w)
            } else {
                (bar.y, bar.y + bar.h)
            };
            assert!(
                coordinate >= min - 0.01 && coordinate <= max + 0.01,
                "{path}"
            );
        }
    }

    #[test]
    fn bar_primitive_radius_only_preserves_vertical_bar_geometry() {
        let square =
            boxes_for(r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[4]}]}}"#);
        let rounded = boxes_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[4],"borderRadius":3}]}}"#,
        );
        assert_eq!(rounded, square);
    }

    #[test]
    fn vertical_bar_border_radius_rounds_stack_ends_and_mixed_bars() {
        let stack_json = r#"{
          "type":"bar",
          "data":{"labels":["A"],"datasets":[
            {"stack":"s","data":[2],"borderRadius":4},
            {"stack":"s","data":[3],"borderRadius":4},
            {"stack":"s","data":[-2],"borderRadius":4},
            {"stack":"s","data":[-3],"borderRadius":4}
          ]},
          "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}
        }"#;
        let (spec, scene) = scene_for(stack_json);
        let has_primitive = |series_index: usize, path: bool| {
            scene.items.iter().any(|prim| match prim {
                Prim::Path {
                    fill: Some(fill), ..
                } if path => *fill == spec.series[series_index].fill_at(0),
                Prim::Rect { fill, .. } if !path => *fill == spec.series[series_index].fill_at(0),
                _ => false,
            })
        };
        assert!(
            has_primitive(0, false),
            "first positive segment is the base"
        );
        assert!(
            has_primitive(1, true),
            "last positive segment is the stack end"
        );
        assert!(
            has_primitive(2, false),
            "first negative segment is the base"
        );
        assert!(
            has_primitive(3, true),
            "last negative segment is the stack end"
        );

        let square_json = stack_json.replace(",\"borderRadius\":4", "");
        let (square_spec, square_scene) = scene_for(&square_json);
        for series in &square_spec.series {
            let fill = series.fill_at(0);
            assert!(square_scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Rect { fill: rect_fill, .. } if *rect_fill == fill
            )));
            assert!(!square_scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Path { fill: Some(path_fill), .. } if *path_fill == fill
            )));
        }

        let (mixed_spec, mixed_scene) = scene_for(
            r#"{"type":"line","data":{"labels":["A"],"datasets":[{"type":"bar","data":[2],"borderRadius":4},{"data":[3]}]}}"#,
        );
        let bar_fill = mixed_spec.series[0].fill_at(0);
        assert!(mixed_scene.items.iter().any(|prim| matches!(
            prim,
            Prim::Path { fill: Some(fill), .. } if *fill == bar_fill
        )));
    }

    #[test]
    fn vertical_bar_border_radius_value_axis_only_stacked_keeps_independent_bars_rounded() {
        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[2],"borderRadius":4},{"data":[3],"borderRadius":4}]},"options":{"scales":{"x":{"stacked":false},"y":{"stacked":true}}}}"#,
        );
        for series in &spec.series {
            let fill = series.fill_at(0);
            assert!(
                scene.items.iter().any(|prim| matches!(
                    prim,
                    Prim::Path { fill: Some(path_fill), .. } if *path_fill == fill
                )),
                "value-axis stacking must not suppress rounding for an unstacked bar"
            );
        }
    }

    #[test]
    fn vertical_zero_stack_value_does_not_hide_rounded_endpoint() {
        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"stack":"s","data":[5],"borderRadius":4},{"stack":"s","data":[0],"borderRadius":4}]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}}"#,
        );
        let first_fill = spec.series[0].fill_at(0);
        assert!(
            scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Path { fill: Some(fill), .. } if *fill == first_fill
            )),
            "zero-length later segment must not hide the visible stack endpoint"
        );
    }

    #[test]
    fn vertical_clipped_stack_segment_does_not_hide_visible_endpoint() {
        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"stack":"s","data":[2],"borderRadius":4},{"stack":"s","data":[3],"borderRadius":4}]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true,"min":0,"max":2}}}}"#,
        );
        let first_fill = spec.series[0].fill_at(0);
        assert!(
            scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Path { fill: Some(fill), .. } if *fill == first_fill
            )),
            "fully clipped later stack segment must not hide the visible endpoint"
        );
    }

    #[test]
    fn horizontal_zero_stack_value_does_not_hide_rounded_endpoint() {
        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"stack":"s","data":[5],"borderRadius":4},{"stack":"s","data":[0],"borderRadius":4}]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true},"y":{"stacked":true}}}}"#,
        );
        let first_fill = spec.series[0].fill_at(0);
        assert!(
            scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Path { fill: Some(fill), .. } if *fill == first_fill
            )),
            "zero-length later segment must not hide the visible stack endpoint"
        );
    }

    #[test]
    fn horizontal_clipped_stack_segment_does_not_hide_visible_endpoint() {
        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"stack":"s","data":[2],"borderRadius":4},{"stack":"s","data":[3],"borderRadius":4}]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true,"min":0,"max":2},"y":{"stacked":true}}}}"#,
        );
        let first_fill = spec.series[0].fill_at(0);
        assert!(
            scene.items.iter().any(|prim| matches!(
                prim,
                Prim::Path { fill: Some(fill), .. } if *fill == first_fill
            )),
            "fully clipped later stack segment must not hide the visible endpoint"
        );
    }

    #[test]
    fn vertical_clipped_negative_stack_rounds_the_value_end() {
        let (spec, scene) = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"stack":"s","data":[-3],"borderRadius":4},{"stack":"s","data":[-4],"borderRadius":4}]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true,"min":-10,"max":-5}}}}"#,
        );
        let second_fill = spec.series[1].fill_at(0);
        let path = scene
            .items
            .iter()
            .find_map(|prim| match prim {
                Prim::Path {
                    d,
                    fill: Some(fill),
                    ..
                } if *fill == second_fill => Some(d),
                _ => None,
            })
            .expect("visible clipped stack segment should be rounded at its value end");
        let commands: Vec<_> = path
            .split_ascii_whitespace()
            .filter(|token| matches!(*token, "M" | "L" | "C" | "Z"))
            .collect();
        assert_eq!(commands, ["M", "L", "L", "C", "L", "C", "L", "Z"]);
    }

    #[test]
    fn one_box_per_category_series_grouped() {
        // 3 カテゴリ × 2 系列 = 6 矩形。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B","C"],
              "datasets":[{"data":[10,20,30]},{"data":[5,15,25]}]}}"#,
        );
        assert_eq!(bs.len(), 6);
        // (series,index) が全組み合わせ網羅。
        for s in 0..2 {
            for i in 0..3 {
                assert!(bs.iter().any(|b| b.series == s && b.index == i));
            }
        }
    }

    #[test]
    fn boxes_left_to_right_by_category() {
        // 単系列: カテゴリ順に x が増加する。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#,
        );
        assert!(bs[0].x < bs[1].x && bs[1].x < bs[2].x);
        // 幅は正。
        assert!(bs.iter().all(|b| b.w > 0.0));
    }

    #[test]
    fn default_bar_geometry_preserves_legacy_width_and_offset() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],
              "datasets":[{"data":[10,20,30]}]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let band = super::super::common::band_width(&frame, spec.categories.len());
        let center = super::super::common::category_center(&frame, 0, spec.categories.len());
        let category_start = center - band / 2.0;

        assert!((boxes[0].w - band * 0.8 * 0.9).abs() < 1e-9);
        assert!((boxes[0].x - (category_start + band * 0.1)).abs() < 1e-9);
    }

    #[test]
    fn box_height_tracks_value_magnitude() {
        // 値が大きいほど高い矩形(baseline=0)。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,100]}]}}"#,
        );
        assert!(bs[1].h > bs[0].h);
    }

    #[test]
    fn per_dataset_thickness_overrides_percentages_and_honors_maximum() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1],"barThickness":20,"barPercentage":0.1,"categoryPercentage":0.1,"maxBarThickness":12},
              {"data":[1],"barPercentage":0.5,"categoryPercentage":0.6}
            ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        assert_eq!(boxes.len(), 2);
        assert!((boxes[0].w - 12.0).abs() < 1e-9);
        let band = super::super::common::band_width(&frame, 1);
        assert!((boxes[1].w - band * 0.6 / 2.0 * 0.5).abs() < 1e-9);
    }

    #[test]
    fn min_bar_length_applies_to_positive_negative_and_zero_values() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["positive","zero","negative"],"datasets":[
              {"data":[1,0,-1],"minBarLength":12}
            ]},"options":{"scales":{"y":{"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        assert_eq!(boxes.len(), 3);
        assert!(boxes.iter().all(|bar| (bar.h - 12.0).abs() < 1e-9));
        let baseline = frame.ys.map(0.0);
        let zero = boxes.iter().find(|bar| bar.index == 1).unwrap();
        assert!((zero.y + zero.h / 2.0 - baseline).abs() < 1e-9);
    }

    #[test]
    fn min_bar_length_skips_vertical_intervals_outside_hard_bounds() {
        let assert_heights = |json: &str| {
            let boxes = boxes_for(json);
            assert_eq!(boxes.len(), 2);
            assert_eq!(boxes[0].h, 0.0, "fully out-of-range value: {boxes:?}");
            assert_eq!(boxes[1].h, 20.0, "boundary-intersecting value: {boxes:?}");
        };

        assert_heights(
            r#"{"type":"bar","data":{"labels":["below","inside"],"datasets":[
              {"data":[5,11],"minBarLength":20}
            ]},"options":{"scales":{"y":{"min":10,"max":100}}}}"#,
        );
        assert_heights(
            r#"{"type":"bar","data":{"labels":["below","inside"],"datasets":[
              {"data":[5,11],"minBarLength":20}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":10,"max":100}}}}"#,
        );
    }

    #[test]
    fn stacked_min_bar_visibility_uses_raw_value_intervals() {
        let boxes = boxes_for(
            r#"{"type":"bar","data":{"labels":["stacked"],"datasets":[
              {"data":[11],"minBarLength":20},
              {"data":[1],"minBarLength":20}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":10,"max":100}}}}"#,
        );

        assert_eq!(boxes.len(), 2);
        assert!(
            boxes.iter().all(|bar| (bar.h - 20.0).abs() < 1e-9),
            "{boxes:?}"
        );
    }

    #[test]
    fn stacked_datasets_keep_geometry_inside_their_stack_slot() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
            {"stack":"first","data":[1],"barThickness":10,"minBarLength":8},
            {"stack":"first","data":[1],"barThickness":10,"minBarLength":8},
            {"stack":"second","data":[1],"barThickness":6,"minBarLength":8}
            ]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true,"min":0,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        assert_eq!(boxes.len(), 3);
        assert!((boxes[0].w - 10.0).abs() < 1e-9);
        assert!((boxes[1].w - 10.0).abs() < 1e-9);
        assert!((boxes[2].w - 6.0).abs() < 1e-9);
        assert!((boxes[0].x - boxes[1].x).abs() < 1e-9);
        assert_ne!(boxes[0].x, boxes[2].x);
        assert!(boxes.iter().all(|bar| (bar.h - 8.0).abs() < 1e-9));
    }

    #[test]
    fn legacy_stacked_slot_is_consistent_when_only_one_dataset_has_geometry_options() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"stack":"same","data":[1]},
              {"stack":"same","data":[1],"minBarLength":8}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);

        assert_eq!(boxes.len(), 2);
        assert!(
            (boxes[0].x - boxes[1].x).abs() < 1e-9,
            "stacked segments must share one x slot: {boxes:?}"
        );
    }

    #[test]
    fn legacy_dodge_slots_are_consistent_when_only_one_dataset_has_geometry_options() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1]},
              {"data":[2],"barPercentage":0.9}
            ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let band = super::super::common::band_width(&frame, 1);
        let expected_center_distance = band * DEFAULT_CATEGORY_PERCENTAGE / 2.0;

        assert_eq!(boxes.len(), 2);
        let center_distance = (boxes[1].x + boxes[1].w / 2.0) - (boxes[0].x + boxes[0].w / 2.0);
        assert!(
            (center_distance - expected_center_distance).abs() < 1e-9,
            "dodge slot centers should be evenly spaced: {boxes:?}"
        );
    }

    #[test]
    fn flex_thickness_uses_category_intervals_and_dataset_percentages() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[
              {"data":[1,2,3],"barThickness":"flex","categoryPercentage":0.5,"barPercentage":0.4}
            ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let expected = super::super::common::band_width(&frame, 3) * 0.5 * 0.4;
        assert_eq!(boxes.len(), 3);
        assert!(boxes.iter().all(|bar| (bar.w - expected).abs() < 1e-9));
    }

    #[test]
    fn explicitly_configured_flex_bars_are_centered_in_their_slots() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1],"barThickness":"flex"},
              {"data":[1],"categoryPercentage":0.8,"barPercentage":0.9}
            ]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let center = super::super::common::category_center(&frame, 0, 1);
        let band = super::super::common::band_width(&frame, 1);
        let slot_size = band * DEFAULT_CATEGORY_PERCENTAGE / 2.0;

        assert_eq!(boxes.len(), 2);
        for (slot, bar) in boxes.iter().enumerate() {
            let slot_center = center - slot_size + slot_size * (slot as f64 + 0.5);
            assert!(
                (bar.x + bar.w / 2.0 - slot_center).abs() < 1e-9,
                "explicit geometry should be centered in its slot: {bar:?}, slot_center={slot_center}"
            );
        }
    }

    #[test]
    fn min_bar_length_keeps_negative_vertical_stack_base() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[-40],"minBarLength":15},
              {"data":[-1],"minBarLength":15}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let second = boxes.iter().find(|bar| bar.series == 1).unwrap();
        let stack_base = frame.ys.map(-40.0);

        assert!((second.y - stack_base).abs() < 1e-9);
        assert!((second.h - 15.0).abs() < 1e-9);
    }

    #[test]
    fn min_bar_length_does_not_extend_vertical_stacks_past_hard_bounds() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[99],"minBarLength":20},
              {"data":[1],"minBarLength":20},
              {"data":[-99],"minBarLength":20},
              {"data":[-1],"minBarLength":20}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);

        for bar in boxes {
            assert!(bar.y >= frame.plot_top, "bar top escaped plot: {bar:?}");
            assert!(
                bar.y + bar.h <= frame.plot_bottom,
                "bar bottom escaped plot: {bar:?}"
            );
        }
    }

    #[test]
    fn min_bar_length_stacked_vertical_segments_follow_visual_endpoints() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1],"minBarLength":20},
              {"data":[1],"minBarLength":20},
              {"data":[-1],"minBarLength":20},
              {"data":[-1],"minBarLength":20}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let first_positive = boxes.iter().find(|bar| bar.series == 0).unwrap();
        let second_positive = boxes.iter().find(|bar| bar.series == 1).unwrap();
        let first_negative = boxes.iter().find(|bar| bar.series == 2).unwrap();
        let second_negative = boxes.iter().find(|bar| bar.series == 3).unwrap();

        assert!(
            (second_positive.y + second_positive.h - first_positive.y).abs() < 1e-9,
            "the upper positive segment should begin where the extended lower segment ends: {boxes:?}"
        );
        assert!(
            (second_negative.y - (first_negative.y + first_negative.h)).abs() < 1e-9,
            "the lower negative segment should begin where the extended upper segment ends: {boxes:?}"
        );
    }

    #[test]
    fn min_bar_length_stacked_vertical_zero_uses_prior_negative_visual_stack() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[-1],"minBarLength":20},
              {"data":[0],"minBarLength":20},
              {"data":[-1],"minBarLength":20}
            ]},"options":{"scales":{"x":{"stacked":true},
              "y":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        let first_negative = boxes.iter().find(|bar| bar.series == 0).unwrap();
        let zero = boxes.iter().find(|bar| bar.series == 1).unwrap();
        let last_negative = boxes.iter().find(|bar| bar.series == 2).unwrap();
        let zero_center = zero.y + zero.h / 2.0;

        assert!(
            (zero_center - (first_negative.y + first_negative.h)).abs() < 1e-9,
            "zero should be centered at the endpoint of the prior negative stack: {boxes:?}"
        );
        assert!(
            (last_negative.y - (zero_center + 20.0)).abs() < 1e-9,
            "the later negative bar should include zero's negative visual extent: {boxes:?}"
        );
    }

    #[test]
    fn horizontal_min_bar_length_preserves_direction_and_centers_zero() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["positive","zero","negative"],"datasets":[
              {"data":[1,0,-1],"minBarLength":12}
            ]},"options":{"indexAxis":"y","scales":{"x":{"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let bars: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, w, fill, .. } if *fill == spec.series[0].fill_at(0) => {
                    Some((*x, *w))
                }
                _ => None,
            })
            .collect();
        assert_eq!(bars.len(), 3);
        assert!(bars.iter().all(|(_, width)| (*width - 12.0).abs() < 1e-9));
        let baseline = bars[0].0;
        assert!((bars[1].0 + bars[1].1 / 2.0 - baseline).abs() < 1e-9);
        assert!((bars[2].0 + bars[2].1 - baseline).abs() < 1e-9);
    }

    #[test]
    fn min_bar_length_skips_horizontal_intervals_outside_hard_bounds() {
        let assert_widths = |json: &str| {
            let spec = chartjs::parse(json, false).unwrap();
            let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
            let scene = build(&spec, &m);
            let fills = [spec.series[0].fill_at(0), spec.series[0].fill_at(1)];
            let widths: Vec<_> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Prim::Rect { w, fill, .. } if fills.contains(fill) => Some(*w),
                    _ => None,
                })
                .collect();
            assert_eq!(widths.len(), 2);
            assert_eq!(widths[0], 0.0, "fully out-of-range value: {widths:?}");
            assert_eq!(widths[1], 20.0, "boundary-intersecting value: {widths:?}");
        };

        assert_widths(
            r#"{"type":"bar","data":{"labels":["below","inside"],"datasets":[
              {"data":[5,11],"minBarLength":20}
            ]},"options":{"indexAxis":"y","scales":{"x":{"min":10,"max":100}}}}"#,
        );
        assert_widths(
            r#"{"type":"bar","data":{"labels":["below","inside"],"datasets":[
              {"data":[5,11],"minBarLength":20}
            ]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true,"min":10,"max":100},
              "y":{"stacked":true}}}}"#,
        );
    }

    #[test]
    fn horizontal_stacked_min_bar_visibility_uses_raw_value_intervals() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["stacked"],"datasets":[
              {"data":[11],"minBarLength":20},
              {"data":[1],"minBarLength":20}
            ]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true,
              "min":10,"max":100},"y":{"stacked":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let fills = [spec.series[0].fill_at(0), spec.series[1].fill_at(0)];
        let scene = build(&spec, &m);
        let widths: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { w, fill, .. } if fills.contains(fill) => Some(*w),
                _ => None,
            })
            .collect();

        assert_eq!(widths, [20.0, 20.0]);
    }

    #[test]
    fn horizontal_dodge_slots_are_consistent_when_only_one_dataset_has_geometry_options() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1]},
              {"data":[2],"barPercentage":0.9}
            ]},"options":{"indexAxis":"y","scales":{"x":{"min":0,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rect_for = |series_index: usize| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Rect { y, h, fill, .. }
                        if *fill == spec.series[series_index].fill_at(0) =>
                    {
                        Some((*y, *h))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (first_y, first_h) = rect_for(0);
        let (second_y, second_h) = rect_for(1);
        let center_distance = (second_y + second_h / 2.0) - (first_y + first_h / 2.0);
        let expected_center_distance = first_h / DEFAULT_BAR_PERCENTAGE;

        assert!((first_h - second_h).abs() < 1e-9);
        assert!(
            (center_distance - expected_center_distance).abs() < 1e-9,
            "horizontal dodge lane centers should be evenly spaced"
        );
    }

    #[test]
    fn horizontal_min_bar_length_datalabels_follow_adjusted_bar_endpoints() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["positive","negative"],"datasets":[
              {"data":[0.1,-0.1],"minBarLength":15}
            ]},"options":{"indexAxis":"y","scales":{"x":{"min":-100,"max":100}},
              "plugins":{"datalabels":{"display":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rects: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, w, fill, .. } if *fill == spec.series[0].fill_at(0) => {
                    Some((*x, *w))
                }
                _ => None,
            })
            .collect();
        let label_x = |value: &str| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Text { x, content, .. } if content == value => Some(*x),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing data label {value}"))
        };

        assert_eq!(rects.len(), 2);
        assert!((label_x("0.1") - (rects[0].0 + rects[0].1 + 4.0)).abs() < 1e-9);
        assert!((label_x("-0.1") - (rects[1].0 - 4.0)).abs() < 1e-9);
    }

    #[test]
    fn vertical_min_bar_length_label_follows_negative_bar_at_axis_edge() {
        let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[-1],"minBarLength":15}
            ]},"options":{"scales":{"y":{"min":-100,"max":-1}},
              "plugins":{"datalabels":{"display":true}}}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let bar = &vertical_bar_boxes(&spec, &frame)[0];
        let scene = build(&spec, &m);
        let label_y = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Text {
                    x,
                    y,
                    content,
                    anchor: crate::scene::Anchor::Middle,
                    ..
                } if content == "-1" && (x - (bar.x + bar.w / 2.0)).abs() < 1e-9 => Some(*y),
                _ => None,
            })
            .expect("missing data label for -1");

        let expected_y = bar.y + bar.h + spec.theme.font_size;
        assert!(
            (label_y - expected_y).abs() < 1e-9,
            "negative bar label should follow its adjusted lower endpoint: label={label_y}, expected={expected_y}"
        );
    }

    #[test]
    fn horizontal_min_bar_length_label_follows_negative_bar_at_axis_edge() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[-1],"minBarLength":15}
            ]},"options":{"indexAxis":"y","scales":{"x":{"min":-100,"max":-1}},
              "plugins":{"datalabels":{"display":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let bar_x = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Rect { x, .. } => Some(*x),
                _ => None,
            })
            .expect("missing bar rectangle");
        let label_x = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Text {
                    x,
                    content,
                    anchor: crate::scene::Anchor::End,
                    ..
                } if content == "-1" => Some(*x),
                _ => None,
            })
            .expect("negative bar label should use the left-side anchor");

        assert!((label_x - (bar_x - 4.0)).abs() < 1e-9);
    }

    #[test]
    fn horizontal_placement_stacked_min_bar_length_datalabels_follow_adjusted_endpoints() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["positive","negative"],"datasets":[
              {"data":[0.1,-0.1],"minBarLength":15},
              {"data":[0.2,-0.2],"minBarLength":15}
            ]},"options":{"indexAxis":"y","scales":{"x":{"min":-100,"max":100},
              "y":{"stacked":true}},"plugins":{"datalabels":{"display":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        for (series_index, values) in [(0, ["0.1", "-0.1"]), (1, ["0.2", "-0.2"])] {
            let rects: Vec<(f64, f64)> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Prim::Rect { x, w, fill, .. }
                        if *fill == spec.series[series_index].fill_at(0) =>
                    {
                        Some((*x, *w))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(rects.len(), 2);

            for (index, value) in values.into_iter().enumerate() {
                let label_x = scene
                    .items
                    .iter()
                    .find_map(|item| match item {
                        Prim::Text { x, content, .. } if content == value => Some(*x),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing data label {value}"));
                let expected = if value.starts_with('-') {
                    rects[index].0 - 4.0
                } else {
                    rects[index].0 + rects[index].1 + 4.0
                };
                assert!(
                    (label_x - expected).abs() < 1e-9,
                    "label {value} at {label_x}, expected {expected}"
                );
            }
        }
    }

    #[test]
    fn min_bar_length_keeps_negative_horizontal_stack_base() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[-40],"minBarLength":15},
              {"data":[-1],"minBarLength":15}
            ]},"options":{"indexAxis":"y","scales":{"y":{"stacked":true},
              "x":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rects: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, w, fill, .. } if *fill == spec.series[1].fill_at(0) => {
                    Some((*x, *w))
                }
                _ => None,
            })
            .collect();
        let previous = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Rect { x, fill, .. } if *fill == spec.series[0].fill_at(0) => Some(*x),
                _ => None,
            })
            .unwrap();

        assert_eq!(rects.len(), 1);
        assert!((rects[0].0 + rects[0].1 - previous).abs() < 1e-9);
        assert!((rects[0].1 - 15.0).abs() < 1e-9);
    }

    #[test]
    fn min_bar_length_does_not_extend_horizontal_stacks_past_hard_bounds() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[99],"minBarLength":20},
              {"data":[1],"minBarLength":20},
              {"data":[-99],"minBarLength":20},
              {"data":[-1],"minBarLength":20}
            ]},"options":{"indexAxis":"y","scales":{"y":{"stacked":true},
              "x":{"stacked":true,"min":-100,"max":100}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let left = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Text { x, content, .. } if content == "-100" => Some(*x),
                _ => None,
            })
            .unwrap();
        let right = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Text { x, content, .. } if content == "100" => Some(*x),
                _ => None,
            })
            .unwrap();
        let rects: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, w, fill, .. }
                    if spec.series.iter().any(|series| series.fill_at(0) == *fill) =>
                {
                    Some((*x, *w))
                }
                _ => None,
            })
            .collect();

        assert_eq!(rects.len(), 4);
        for (x, width) in rects {
            assert!(x >= left, "bar left escaped plot: x={x}, left={left}");
            assert!(
                x + width <= right,
                "bar right escaped plot: x={x}, width={width}, right={right}"
            );
        }
    }

    #[test]
    fn min_bar_length_stacked_horizontal_segments_follow_visual_endpoints() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[1],"minBarLength":20},
              {"data":[1],"minBarLength":20},
              {"data":[-1],"minBarLength":20},
              {"data":[-1],"minBarLength":20}
            ]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true,"min":-100,"max":100},
              "y":{"stacked":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rect_for = |series_index: usize| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Rect { x, w, fill, .. }
                        if *fill == spec.series[series_index].fill_at(0) =>
                    {
                        Some((*x, *w))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (first_x, first_w) = rect_for(0);
        let (second_x, _) = rect_for(1);
        let (first_negative_x, _) = rect_for(2);
        let (second_negative_x, second_negative_w) = rect_for(3);

        assert!(
            (second_x - (first_x + first_w)).abs() < 1e-9,
            "the later segment should begin where the visually extended earlier segment ends"
        );
        assert!(
            (second_negative_x + second_negative_w - first_negative_x).abs() < 1e-9,
            "the later negative segment should begin where the extended earlier segment ends"
        );
    }

    #[test]
    fn min_bar_length_stacked_horizontal_zero_uses_prior_negative_visual_stack() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[-1],"minBarLength":20},
              {"data":[0],"minBarLength":20},
              {"data":[-1],"minBarLength":20}
            ]},"options":{"indexAxis":"y","scales":{"x":{"stacked":true,"min":-100,"max":100},
              "y":{"stacked":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rect_for = |series_index: usize| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Rect { x, w, fill, .. }
                        if *fill == spec.series[series_index].fill_at(0) =>
                    {
                        Some((*x, *w))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (first_x, _) = rect_for(0);
        let (zero_x, zero_w) = rect_for(1);
        let (last_x, last_w) = rect_for(2);
        let zero_center = zero_x + zero_w / 2.0;

        assert!(
            (zero_center - first_x).abs() < 1e-9,
            "zero should be centered at the endpoint of the prior negative stack"
        );
        assert!(
            ((last_x + last_w) - (zero_center - 20.0)).abs() < 1e-9,
            "the later negative bar should include zero's negative visual extent"
        );
    }

    #[test]
    fn vertical_bar_boxes_clip_out_of_range_values_to_hard_bounds() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[1,50,100]}]},
               "options":{"scales":{"y":{"min":13,"max":87}}}}"#,
            false,
        )
        .unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &measurer);

        let boxes = vertical_bar_boxes(&spec, &frame);

        assert_eq!(boxes.len(), 3);
        for bar in boxes {
            assert!(bar.y >= frame.plot_top, "bar top escaped plot: {bar:?}");
            assert!(
                bar.y + bar.h <= frame.plot_bottom,
                "bar bottom escaped plot: {bar:?}"
            );
        }
    }

    #[test]
    fn vertical_stacked_log_bars_fit_the_sum_domain() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[10]},{"data":[10]}]},
               "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true,"type":"logarithmic"}}}}"#,
            false,
        )
        .unwrap();
        let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &measurer);

        assert_eq!(
            frame.ticks.max, 20.0,
            "domain must include the 10 + 10 stack"
        );
        let boxes = vertical_bar_boxes(&spec, &frame);
        assert_eq!(boxes.len(), 2);
        for bar in boxes {
            assert!(bar.y >= frame.plot_top, "bar top escaped plot: {bar:?}");
            assert!(
                bar.y + bar.h <= frame.plot_bottom,
                "bar bottom escaped plot: {bar:?}"
            );
        }
    }

    #[test]
    fn stacked_collapses_to_one_column_per_category() {
        // 積み上げ: 2 カテゴリ × 2 系列、各カテゴリの 2 矩形は同じ x・同じ幅(縦に積む)。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B"],
              "datasets":[{"data":[10,20]},{"data":[30,40]}]},
              "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}}"#,
        );
        assert_eq!(bs.len(), 4);
        let cat0: Vec<&BarBox> = bs.iter().filter(|b| b.index == 0).collect();
        assert_eq!(cat0.len(), 2);
        assert_eq!(cat0[0].x, cat0[1].x);
        assert_eq!(cat0[0].w, cat0[1].w);
    }

    #[test]
    fn stacked_bar_stack_ids_create_parallel_columns_and_independent_totals() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[2]},
              {"data":[3],"stack":"bar"},
              {"data":[7],"stack":"fruit"}
            ]},"options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);

        let boxes = vertical_bar_boxes(&spec, &frame);
        assert_eq!(boxes.len(), 3);
        assert_eq!(
            boxes[0].x, boxes[1].x,
            "implicit bar and explicit bar share a stack"
        );
        assert_ne!(
            boxes[0].x, boxes[2].x,
            "a different stack gets a parallel column"
        );
        let baseline_y = frame.ys.map(0.0);
        assert!((boxes[0].y + boxes[0].h - baseline_y).abs() < 1e-9);
        assert!((boxes[2].y + boxes[2].h - baseline_y).abs() < 1e-9);
    }

    #[test]
    fn index_axis_stacking_uses_parallel_slots_without_value_stacking() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[2]},{"data":[2],"stack":"fruit"}
            ]},"options":{"scales":{"x":{"stacked":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);

        let boxes = vertical_bar_boxes(&spec, &frame);
        assert_eq!(boxes.len(), 2);
        assert_ne!(boxes[0].x, boxes[1].x);
        assert_eq!(boxes[0].y, boxes[1].y);
        assert_eq!(boxes[0].h, boxes[1].h);
    }

    #[test]
    fn vertical_dodge_skips_nan_value() {
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10, null, 30]}]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        let boxes = vertical_bar_boxes(&spec, &frame);
        assert!(
            !boxes.iter().any(|b| b.index == 1),
            "NaN category should have no BarBox: {:?}",
            boxes
        );
        assert!(boxes.iter().any(|b| b.index == 0));
        assert!(boxes.iter().any(|b| b.index == 2));
    }

    #[test]
    fn vertical_log_dodge_skips_non_positive_values() {
        let boxes = boxes_for(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[{"data":[-5,0,10]}]},
               "options":{"scales":{"y":{"type":"logarithmic"}}}}"#,
        );
        assert_eq!(boxes.len(), 1, "対数軸の非正値は BarBox を生成しない");
        assert_eq!(boxes[0].index, 2);
    }

    #[test]
    fn horizontal_dodge_skips_nan_value() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10, null, 30]}]},
               "options":{"indexAxis":"y"}}"#,
            false,
        )
        .unwrap();
        let scene = super::build(&spec, &m);
        let rects: Vec<_> = scene
            .items
            .iter()
            .filter(|p| matches!(p, crate::scene::Prim::Rect { .. }))
            .collect();
        let spec_no_null = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10, 20, 30]}]},
               "options":{"indexAxis":"y"}}"#,
            false,
        )
        .unwrap();
        let scene_full = super::build(&spec_no_null, &m);
        let rects_full: Vec<_> = scene_full
            .items
            .iter()
            .filter(|p| matches!(p, crate::scene::Prim::Rect { .. }))
            .collect();
        assert_eq!(
            rects_full.len() - rects.len(),
            1,
            "NaN カテゴリで rect が 1 個減るはず"
        );
    }

    #[test]
    fn horizontal_log_dodge_skips_non_positive_values() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let spec = chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a","b","c"],
               "datasets":[{"data":[-5,0,10]}]},
               "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}}"#,
            false,
        )
        .unwrap();
        let scene = super::build(&spec, &m);
        let rects: Vec<_> = scene
            .items
            .iter()
            .filter(|p| matches!(p, crate::scene::Prim::Rect { .. }))
            .collect();
        assert_eq!(rects.len(), 1, "対数軸の非正値は横棒を生成しない");
    }
}

#[cfg(test)]
mod horizontal_axis_style_tests {
    //! 横棒(indexAxis:"y") のグリッド/ボーダー/軸タイトル反映テスト。
    //! ChartJS フロントエンドを経由して spec を組む(scales.x/y と options.plugins.title を直に指定できる)。

    use super::{
        HorizontalTickLabels, MIN_HORIZONTAL_PLOT_WIDTH, build, finite_text_width,
        horizontal_legend_band_width, horizontal_plot_bounds,
    };
    use crate::font::TEST_FONT as DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::ir::ChartSpec;
    use crate::layout::common::{OUTER_PAD, X_LABEL_BAND, value_domain};
    use crate::num::fmt_num;
    use crate::scale::nice_ticks;
    use crate::scene::{Anchor, Prim, Scene};
    use crate::text::TextMeasurer;

    fn parse(json: &str) -> ChartSpec {
        chartjs::parse(json, false).expect("parse")
    }

    fn scene_for(json: &str) -> Scene {
        let spec = parse(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        build(&spec, &m)
    }

    #[test]
    fn horizontal_stacked_bar_stack_ids_create_parallel_lanes() {
        let scene = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[
              {"data":[2]},
              {"data":[3],"stack":"bar"},
              {"data":[7],"stack":"fruit"}
            ]},"options":{"indexAxis":"y","scales":{"y":{"stacked":true},"x":{"stacked":true}}}}"#,
        );
        let rects: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .collect();

        assert_eq!(rects.len(), 3);
        assert_eq!(
            rects[0].1, rects[1].1,
            "implicit bar and explicit bar share a lane"
        );
        assert_ne!(
            rects[0].1, rects[2].1,
            "a different stack gets a parallel lane"
        );
        assert_eq!(
            rects[0].0, rects[2].0,
            "each stack starts its own accumulation"
        );
    }

    fn horizontal_plot_right(spec: &ChartSpec, m: &TextMeasurer<'_>) -> f64 {
        let (dmin, dmax) = value_domain(spec, &spec.x_axis);
        let ticks = crate::layout::common::configured_axis_ticks(dmin, dmax, &spec.x_axis);
        let max_cat_w = spec
            .categories
            .iter()
            .map(|category| finite_text_width(m, category, spec.theme.font_size))
            .fold(0.0, f64::max);
        let cat_w = max_cat_w + 10.0;
        let y_title_w = spec
            .y_axis
            .title
            .as_ref()
            .map(|title| title.font_size.unwrap_or(spec.theme.font_size * 1.1) + 6.0)
            .unwrap_or(0.0);
        let has_legend = spec.series.iter().any(|series| !series.name.is_empty());
        let series_names: Vec<String> = spec
            .series
            .iter()
            .map(|series| series.name.clone())
            .collect();
        let legend_left = if has_legend && spec.legend == crate::ir::LegendPos::Left {
            horizontal_legend_band_width(
                m,
                &series_names,
                spec.theme.font_size,
                &spec.legend_options,
            )
        } else {
            0.0
        };
        let legend_right = if has_legend && spec.legend == crate::ir::LegendPos::Right {
            horizontal_legend_band_width(
                m,
                &series_names,
                spec.theme.font_size,
                &spec.legend_options,
            )
        } else {
            0.0
        };
        let base_left = OUTER_PAD + cat_w + y_title_w + legend_left;
        let base_right = spec.width - OUTER_PAD - legend_right;
        horizontal_plot_bounds(
            base_left,
            base_right,
            spec.width,
            &ticks.ticks,
            m,
            spec.theme.font_size,
            HorizontalTickLabels {
                axis: &spec.x_axis,
                temporal_ticks: None,
            },
        )
        .1
    }

    /// 実機バグ回帰テスト: `horizontal_plot_bounds` は端ラベル幅を測って
    /// プロット境界の余白を決めるが、以前は対数軸でも常に `fmt_num`(小数2桁丸め)
    /// を使っていた。1e-15 のような極端な桁の tick は `fmt_num` だと "0" に潰れて
    /// ほぼ幅ゼロと見積もられ、実際に `fmt_num_log` で描画されるラベル
    /// ("1e-15" 相当)がプロット外へはみ出す(自動レビュー指摘)。
    /// log 軸 spec を渡すと `fmt_num_log` の(より長い)ラベル幅を反映し、
    /// 同じ tick・同じ base_right でも右端の余白がより広く確保される
    /// (=plot_right がより小さくなる)ことを固定する。
    #[test]
    fn horizontal_plot_bounds_reserves_more_space_for_extreme_log_labels() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let ticks = [1e-15];
        let linear = parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"options":{"indexAxis":"y"}}"#,
        );
        let logarithmic = parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}}"#,
        );
        let (_, plot_right_linear) = horizontal_plot_bounds(
            50.0,
            700.0,
            800.0,
            &ticks,
            &m,
            12.0,
            HorizontalTickLabels {
                axis: &linear.x_axis,
                temporal_ticks: None,
            },
        );
        let (_, plot_right_log) = horizontal_plot_bounds(
            50.0,
            700.0,
            800.0,
            &ticks,
            &m,
            12.0,
            HorizontalTickLabels {
                axis: &logarithmic.x_axis,
                temporal_ticks: None,
            },
        );
        assert!(
            plot_right_log < plot_right_linear,
            "log 軸では fmt_num_log の長いラベル分だけ右余白が広く \
             (plot_right が小さく)なるはず: log={plot_right_log} linear={plot_right_linear}"
        );
    }

    /// 値軸(=X)のグリッド線を検出: y1!=y2 かつ x1==x2(垂直線)で grid_color。
    fn count_vertical_gridlines(scene: &Scene, spec: &ChartSpec) -> usize {
        scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (x1 - x2).abs() < 0.01
                            && (y1 - y2).abs() > 1.0
                            && stroke.r == spec.theme.grid_color.r
                            && stroke.g == spec.theme.grid_color.g
                            && stroke.b == spec.theme.grid_color.b
                )
            })
            .count()
    }

    #[test]
    fn horizontal_x_grid_display_false_drops_vertical_gridlines() {
        // grid.display=false → 縦グリッド 0 本。カテゴリラベル(左)は残る。
        let scene = scene_for(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]},
                "options":{"indexAxis":"y","scales":{"x":{"grid":{"display":false}}}}}"#,
        );
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]},
                "options":{"indexAxis":"y","scales":{"x":{"grid":{"display":false}}}}}"#,
        );
        assert_eq!(
            count_vertical_gridlines(&scene, &spec),
            0,
            "x_axis.grid.display=false → 縦グリッド 0 本"
        );
        // カテゴリラベル(A/B/C, anchor=End)は残る。
        let labels = scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Text { content, anchor: Anchor::End, .. }
                        if content == "A" || content == "B" || content == "C"
                )
            })
            .count();
        assert_eq!(labels, 3, "カテゴリラベルは grid を消しても残る");
    }

    #[test]
    fn horizontal_y_border_display_false_drops_left_baseline() {
        // 既定では左のカテゴリ軸線を描く。border.display=false で消える。
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","scales":{"y":{"border":{"display":false}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let ink = spec.theme.text_color;
        // 左辺垂直ベースライン: x1==x2, ink 色, y は plot_top..plot_bottom を張る。
        // grid の垂直線は色が grid_color なので識別可能。
        let baseline = scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (x1 - x2).abs() < 0.01
                            && (y1 - y2).abs() > 1.0
                            && stroke.r == ink.r && stroke.g == ink.g && stroke.b == ink.b
                )
            })
            .count();
        assert_eq!(
            baseline, 0,
            "y_axis.border.display=false → 左辺ベースライン無し"
        );
    }

    #[test]
    fn horizontal_x_border_style_reaches_bottom_baseline() {
        let spec = parse(
            r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","theme":{"textColor":"#123456"},"scales":{"x":{"border":{
                    "width":3,"dash":[5,2]
                }}}}}"##,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let plot_left = OUTER_PAD
            + spec
                .categories
                .iter()
                .map(|category| m.width(category, spec.theme.font_size as f32))
                .fold(0.0_f32, f32::max) as f64
            + 10.0;
        let plot_right = horizontal_plot_right(&spec, &m);
        let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND;
        let scene = build(&spec, &m);
        let baseline = scene.items.iter().find_map(|p| match p {
            Prim::Line {
                x1,
                x2,
                y1,
                y2,
                stroke,
                stroke_width,
                dash,
            } if (*x1 - plot_left).abs() < 0.01
                && (*x2 - plot_right).abs() < 0.01
                && (*y1 - plot_bottom).abs() < 0.01
                && (*y2 - plot_bottom).abs() < 0.01 =>
            {
                Some((*stroke, *stroke_width, dash.as_slice()))
            }
            _ => None,
        });
        let (stroke, width, dash) =
            baseline.expect("x_axis.border should span the bottom plot baseline");
        assert_eq!(stroke, spec.theme.text_color);
        assert!((width - 3.0).abs() < 1e-9);
        assert_eq!(dash, &[5.0, 2.0]);
    }

    #[test]
    fn horizontal_x_border_display_controls_bottom_baseline() {
        fn count_bottom_baseline(
            scene: &Scene,
            plot_left: f64,
            plot_right: f64,
            plot_bottom: f64,
        ) -> usize {
            scene
                .items
                .iter()
                .filter(|p| {
                    matches!(p,
                        Prim::Line { x1, x2, y1, y2, .. }
                            if (*x1 - plot_left).abs() < 0.01
                                && (*x2 - plot_right).abs() < 0.01
                                && (*y1 - plot_bottom).abs() < 0.01
                                && (*y2 - plot_bottom).abs() < 0.01
                    )
                })
                .count()
        }

        let visible_spec = parse(
            r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","scales":{"x":{"border":{
                    "display":true
                }}}}}"##,
        );
        let hidden_spec = parse(
            r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","scales":{"x":{"border":{
                    "display":false
                }}}}}"##,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let plot_left = OUTER_PAD
            + visible_spec
                .categories
                .iter()
                .map(|category| m.width(category, visible_spec.theme.font_size as f32))
                .fold(0.0_f32, f32::max) as f64
            + 10.0;
        let plot_right = horizontal_plot_right(&visible_spec, &m);
        let plot_bottom = visible_spec.height - OUTER_PAD - X_LABEL_BAND;
        let visible = build(&visible_spec, &m);
        let hidden = build(&hidden_spec, &m);

        assert_eq!(
            count_bottom_baseline(&visible, plot_left, plot_right, plot_bottom),
            1
        );
        assert_eq!(
            count_bottom_baseline(&hidden, plot_left, plot_right, plot_bottom),
            0
        );
    }

    #[test]
    fn horizontal_x_grid_draw_ticks_true_adds_bottom_tick_marks() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","scales":{"x":{"grid":{"drawTicks":true}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        // tick 短線: x1==x2, y2-y1==4.0 (プロット下側 plot_bottom→plot_bottom+4)。
        let ticks = scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (x1 - x2).abs() < 0.01 && ((*y2 - *y1) - 4.0).abs() < 1e-9
                )
            })
            .count();
        assert!(
            ticks > 0,
            "x_axis.grid.draw_ticks=true → 値軸 tick 短線が出る: 実際 {ticks}"
        );
    }

    #[test]
    fn horizontal_y_axis_title_renders_rotated() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","scales":{"y":{"title":{"display":true,"text":"地域"}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rotated = scene.items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, rotate_deg: Some(deg), .. }
                    if content == "地域" && (deg.abs() - 90.0).abs() < 0.1
            )
        });
        assert!(rotated, "y_axis.title は -90deg 回転で描画");
    }

    #[test]
    fn horizontal_x_axis_title_renders_horizontal() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
                "options":{"indexAxis":"y","scales":{"x":{"title":{"display":true,"text":"売上"}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let has_x_title = scene.items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, rotate_deg: None, .. }
                    if content == "売上"
            )
        });
        assert!(has_x_title, "x_axis.title は水平テキストで描画");
    }

    #[test]
    fn horizontal_rightmost_tick_label_fits_inside_canvas() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[5,500,95000]}]},
                 "options":{"indexAxis":"y"}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (x, size) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Text {
                    x,
                    size,
                    anchor: Anchor::Middle,
                    content,
                    ..
                } if content == "100000" => Some((*x, *size)),
                _ => None,
            })
            .expect("最大 x 軸目盛 100000 が描画される");
        let half_width = m.width("100000", size as f32) as f64 / 2.0;
        assert!(
            x + half_width <= scene.width + 1e-9,
            "右端目盛ラベルが canvas 外へ出ている: x={x}, half_width={half_width}, width={}",
            scene.width
        );
    }

    #[test]
    fn horizontal_right_edge_padding_uses_terminal_tick_width() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[-1000,-500]}]},
                 "options":{"indexAxis":"y"}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (_, plot_right) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (*x2 - *x1) > 1.0
                    && (*y1 - *y2).abs() < 1e-9
                    && *stroke == spec.theme.text_color =>
                {
                    Some((*x1, *x2))
                }
                _ => None,
            })
            .expect("x 軸の下辺が描画される");
        let ticks = {
            let (dmin, dmax) = value_domain(&spec, &spec.x_axis);
            nice_ticks(dmin, dmax, 10)
        };
        let first_width = m.width(&fmt_num(ticks.ticks[0]), spec.theme.font_size as f32);
        let last_width = m.width(
            &fmt_num(*ticks.ticks.last().expect("目盛がある")),
            spec.theme.font_size as f32,
        );
        assert!(
            first_width > last_width,
            "左端の負値目盛が右端の 0 より幅広い入力であること"
        );
        let expected = spec.width - OUTER_PAD - last_width as f64 / 2.0;
        assert!(
            (plot_right - expected).abs() < 1e-9,
            "右端の余白は終端目盛幅だけで決める: actual={plot_right}, expected={expected}"
        );
    }

    #[test]
    fn horizontal_extreme_tick_labels_keep_nonzero_plot_width() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1e308,5e307]}]},
                 "options":{"indexAxis":"y"}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (plot_left, plot_right) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (*y1 - *y2).abs() < 1e-9 && *stroke == spec.theme.text_color => {
                    Some((*x1, *x2))
                }
                _ => None,
            })
            .expect("x 軸の下辺が描画される");
        assert!(
            plot_right > plot_left,
            "極端に幅広い目盛ラベルでもプロット領域を潰さない: left={plot_left}, right={plot_right}"
        );
    }

    #[test]
    fn horizontal_plot_right_includes_right_legend_width() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"label":"売上","data":[10,20]}]},
                 "options":{"indexAxis":"y","plugins":{"legend":{"position":"right"}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (_, plot_right) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (*x2 - *x1) > 1.0
                    && (*y1 - *y2).abs() < 1e-9
                    && *stroke == spec.theme.text_color =>
                {
                    Some((*x1, *x2))
                }
                _ => None,
            })
            .expect("x 軸の下辺が描画される");
        let expected = horizontal_plot_right(&spec, &m);
        assert!(
            (plot_right - expected).abs() < 1e-9,
            "テスト用 plot_right は右凡例帯を本体と同じく考慮する: actual={plot_right}, expected={expected}"
        );
    }

    #[test]
    fn horizontal_narrow_canvas_preserves_minimum_plot_width() {
        let spec = parse(
            r#"{"type":"bar","width":30,"data":{"labels":["長いカテゴリラベル"],"datasets":[{"label":"右凡例","data":[10]}]},
                 "options":{"indexAxis":"y","plugins":{"legend":{"position":"right"}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (plot_left, plot_right) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (*y1 - *y2).abs() < 1e-9 && *stroke == spec.theme.text_color => {
                    Some((*x1, *x2))
                }
                _ => None,
            })
            .expect("x 軸の下辺が描画される");
        assert!(plot_left.is_finite() && plot_right.is_finite());
        assert!(
            plot_right - plot_left >= MIN_HORIZONTAL_PLOT_WIDTH - 1e-9,
            "狭い canvas でも最小プロット幅を確保する: left={plot_left}, right={plot_right}"
        );
        assert!(
            plot_left >= 0.0 && plot_right <= scene.width + 1e-9,
            "最小プロット幅を canvas 内に収める: left={plot_left}, right={plot_right}, width={}",
            scene.width
        );
    }

    #[test]
    fn horizontal_extreme_y_axis_title_preserves_minimum_plot_width() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[10]}]},
                 "options":{"indexAxis":"y","scales":{"y":{"title":{"display":true,"text":"カテゴリ","font":{"size":1e308}}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (plot_left, plot_right) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (*y1 - *y2).abs() < 1e-9 && *stroke == spec.theme.text_color => {
                    Some((*x1, *x2))
                }
                _ => None,
            })
            .expect("x 軸の下辺が描画される");
        assert!(plot_left.is_finite() && plot_right.is_finite());
        assert!(
            plot_right - plot_left >= MIN_HORIZONTAL_PLOT_WIDTH - 1e-9,
            "巨大な y 軸タイトルでも最小プロット幅を確保する: left={plot_left}, right={plot_right}"
        );
        assert!(plot_left >= 0.0 && plot_right <= scene.width + 1e-9);
    }

    #[test]
    fn horizontal_font_size_above_f32_range_keeps_layout_finite() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":[""],"datasets":[{"data":[10]}]},
                 "options":{"indexAxis":"y","theme":{"fontSize":1e40}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let (plot_left, plot_right) = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (*y1 - *y2).abs() < 1e-9 && *stroke == spec.theme.text_color => {
                    Some((*x1, *x2))
                }
                _ => None,
            })
            .expect("x 軸の下辺が描画される");
        assert!(plot_left.is_finite() && plot_right.is_finite());
        assert!(
            plot_right - plot_left >= MIN_HORIZONTAL_PLOT_WIDTH - 1e-9,
            "巨大 fontSize でもプロット境界を有限かつ非縮退にする: left={plot_left}, right={plot_right}"
        );
    }

    #[test]
    fn nonfinite_text_measurements_fall_back_to_zero() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let long_text = "A".repeat(1024);
        assert!(
            !m.width(&long_text, f32::MAX).is_finite(),
            "極端な有限フォントサイズでも計測結果が非有限になり得ること"
        );
        assert!(
            m.width("A", f32::NAN).is_nan(),
            "NaN のフォントサイズは計測結果を NaN にすること"
        );
        assert_eq!(finite_text_width(&m, &long_text, f64::INFINITY), 0.0);
        assert_eq!(finite_text_width(&m, "A", f64::NAN), 0.0);
    }
}

#[cfg(test)]
mod horizontal_log_scale_tests {
    //! 横棒(indexAxis:"y")の対数 X 軸: major/minor grid, log-aware ラベル, tick 刻み,
    //! baseline(bar が軸下端から生える)を検証する。Task 9(common.rs::compute()/draw_frame(),
    //! 縦軸)と対になる、build_horizontal 専用の対数分岐テスト。

    use super::build;
    use crate::font::TEST_FONT as DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::ir::{ChartSpec, ScaleKind};
    use crate::num::fmt_num_log;
    use crate::scene::{Anchor, Prim, Scene};
    use crate::text::TextMeasurer;

    fn parse(json: &str) -> ChartSpec {
        chartjs::parse(json, false).expect("parse")
    }

    fn scene_for(json: &str) -> Scene {
        let spec = parse(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        build(&spec, &m)
    }

    /// 3カテゴリ、値は各 decade の中央(mantissa=5)を跨ぐ: 5(1..10圏)/500(100..1000圏)/50000(10000..100000圏)。
    const LOG_JSON: &str = r#"{"type":"bar","data":{"labels":["A","B","C"],
        "datasets":[{"data":[5, 500, 50000]}]},
        "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}}"#;

    /// 値軸(=X)のグリッド線を検出: y1!=y2 かつ x1==x2(垂直線)で grid_color。
    /// (horizontal_axis_style_tests::count_vertical_gridlines と同じ判定。テストモジュールを跨いで
    /// private fn を共有できないため複製する。)
    fn count_vertical_gridlines(scene: &Scene, spec: &ChartSpec) -> usize {
        scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (x1 - x2).abs() < 0.01
                            && (y1 - y2).abs() > 1.0
                            && stroke.r == spec.theme.grid_color.r
                            && stroke.g == spec.theme.grid_color.g
                            && stroke.b == spec.theme.grid_color.b
                )
            })
            .count()
    }

    #[test]
    fn scale_kind_is_logarithmic_and_scoped_to_x_axis_only() {
        let spec = parse(LOG_JSON);
        assert!(matches!(spec.x_axis.scale_kind, ScaleKind::Logarithmic));
        // カテゴリ軸(=Y)は値軸ではないので Linear のまま(scale_kind に意味を持たないが、
        // 誤って y_axis 側を対数化していないことを確認する)。
        assert!(matches!(spec.y_axis.scale_kind, ScaleKind::Linear));
    }

    #[test]
    fn horizontal_linear_axis_renders_hard_min_max_ticks() {
        let scene = scene_for(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[20,80]}]},
               "options":{"indexAxis":"y","scales":{"x":{"min":13,"max":87}}}}"#,
        );
        let labels: Vec<&str> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            labels.contains(&"13"),
            "hard x-axis min should be labeled: {labels:?}"
        );
        assert!(
            labels.contains(&"87"),
            "hard x-axis max should be labeled: {labels:?}"
        );
        assert!(!labels.contains(&"10") && !labels.contains(&"90"));
    }

    #[test]
    fn horizontal_bars_stay_within_hard_bounds_and_skip_out_of_range_labels() {
        let scene = scene_for(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[1,50,100]}]},
               "options":{"indexAxis":"y","scales":{"x":{"min":13,"max":87}},
               "plugins":{"datalabels":{"display":true}}}}"#,
        );
        let tick_x = |label: &str| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Text { content, x, .. } if content == label => Some(*x),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing hard-bound tick label {label}"))
        };
        let left = tick_x("13");
        let right = tick_x("87");
        let bars: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Rect { x, w, .. } => Some((*x, *w)),
                _ => None,
            })
            .collect();

        assert_eq!(bars.len(), 3);
        for (x, width) in bars {
            assert!(x >= left, "bar left escaped plot: x={x}, left={left}");
            assert!(
                x + width <= right,
                "bar right escaped plot: x={x}, width={width}, right={right}"
            );
        }
        assert!(
            !scene.items.iter().any(|item| matches!(item,
                Prim::Text { content, .. } if content == "1" || content == "100"
            )),
            "out-of-range values should not get labels"
        );
    }

    #[test]
    fn horizontal_stacked_bars_skip_labels_for_out_of_range_segments() {
        let scene = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[100]}]},
               "options":{"indexAxis":"y","scales":{"x":{"stacked":true,"min":13,"max":87},
               "y":{"stacked":true}},"plugins":{"datalabels":{"display":true}}}}"#,
        );

        assert!(
            !scene.items.iter().any(|item| matches!(item,
                Prim::Text { content, .. } if content == "100"
            )),
            "a clipped stacked segment outside the hard x-axis bounds should not get a data label"
        );
    }

    #[test]
    fn major_labels_use_fmt_num_log_and_cover_every_decade_boundary() {
        let scene = scene_for(LOG_JSON);
        // 値ラベルは Anchor::Middle で描かれる(カテゴリラベルは Anchor::End、
        // 凡例/タイトルはこの spec に存在しない)。
        let mut labels: Vec<String> = scene
            .items
            .iter()
            .filter_map(|p| match p {
                Prim::Text {
                    content,
                    anchor: Anchor::Middle,
                    ..
                } => Some(content.clone()),
                _ => None,
            })
            .collect();
        labels.sort();
        // データ 5..50000、横棒 x軸は begin_at_zero:true が既定。min_positive=5 は
        // decade 境界ではないため decade floor(10^floor(log10(5))=1)へ切り下げ、
        // domain_max は tight(50000 のまま、100000 へは外側丸めしない — P1 修正)。
        // よって major は domain [1, 50000] に収まる 1..10000 の5本
        // (100000 は domain_max=50000 を超えるので出ない)。
        let expected: Vec<String> = ["1", "10", "100", "1000", "10000"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(labels, expected);
        // fmt_num_log の丸めなし表現であることの直接確認(fmt_num との違いが出るケースで検証)。
        let sub_one_scene = scene_for(
            r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[0.0003]}]},
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}}"#,
        );
        let has_full_precision_label = sub_one_scene.items.iter().any(|p| {
            matches!(p,
                Prim::Text { content, anchor: Anchor::Middle, .. }
                    if content == &fmt_num_log(0.0001)
            )
        });
        assert!(
            has_full_precision_label,
            "sub-1 の対数ラベルは fmt_num_log の全桁表現を使う"
        );
    }

    #[test]
    fn grid_lines_count_covers_major_and_minor_ticks() {
        // 2 decade ドメイン [1,100] → major=[1,10,100](3本)、
        // minor=mantissa 2..9 × 2 decades(16本) = 縦グリッド計 19 本。
        // beginAtZero:false を明示: 横棒の値軸は既定 beginAtZero:true で、
        // 最小値 1 はちょうど decade 境界(10^0)なので既定のままだと
        // log_value_domain の beginAtZero 特例でドメインが [0.1,100] に広がり
        // (major/minor 本数が変わる)、このテストの主眼(グリッド本数の集計)から
        // 逸れてしまう。beginAtZero の対数軸特例自体は layout/common.rs 側の
        // log_value_domain_begin_at_zero_widens_by_one_decade_when_min_is_exact_boundary
        // で個別に検証済み。
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,100]}]},
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic","beginAtZero":false}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(
            count_vertical_gridlines(&scene, &spec),
            3 + 16,
            "major(3) + minor(16) の縦グリッド線"
        );
    }

    #[test]
    fn grid_display_false_drops_both_major_and_minor_gridlines() {
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,100]}]},
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic","beginAtZero":false,"grid":{"display":false}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(
            count_vertical_gridlines(&scene, &spec),
            0,
            "grid.display=false → major/minor とも縦グリッド 0 本"
        );
        // ラベルは display とは独立に残る(既存の線形パスと同じ挙動)。
        let label_count = scene
            .items
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
        assert_eq!(
            label_count, 3,
            "major tick ラベル(1,10,100)は grid.display と無関係に残る"
        );
    }

    #[test]
    fn draw_ticks_true_covers_major_and_minor_tick_marks() {
        // gridline は major+minor 両方に描く一方、tick 刻みが major だけだと
        // 「グリッド線はあるのに対応する軸の刻みが無い」という見た目の不整合が生じる
        // (Task 9 で common.rs::compute()/draw_frame() に施したのと同じ修正を横棒にも適用)。
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,100]}]},
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic","beginAtZero":false,"grid":{"drawTicks":true}}}}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        // tick 短線: x1==x2, y2-y1==4.0 (プロット下側 plot_bottom→plot_bottom+4)。
        let tick_count = scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, .. }
                        if (x1 - x2).abs() < 0.01 && ((*y2 - *y1) - 4.0).abs() < 1e-9
                )
            })
            .count();
        assert_eq!(
            tick_count,
            3 + 16,
            "log 軸の tick 刻み数は major(3)+minor(16) の本数と一致すべき"
        );
    }

    #[test]
    fn bars_grow_from_axis_floor_not_zero() {
        // base_v = 0.0.clamp(ticks.min, ticks.max) は対数軸でも ticks.min(常に正の
        // tight ドメイン下端)に評価される(0.0 は決して正のドメインに含まれないため)。
        // よって全ての bar は左端(plot_left = xs.map(ticks.min))から生える。
        // この行は変更していないので、その挙動を実測で確認する。
        //
        // 実装内部の ValueScale を直接使わず、描画済みの major ラベル("1"/"10")の
        // x 座標だけから期待値を導出する(log10 補間)。これにより「対数写像そのもの」を
        // 独立に検証できる(単に3本の bar の x が互いに一致するだけの弱い保証ではない)。
        let spec = parse(LOG_JSON);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);

        let label_x = |wanted: &str| -> f64 {
            scene
                .items
                .iter()
                .find_map(|p| match p {
                    Prim::Text {
                        x,
                        content,
                        anchor: Anchor::Middle,
                        ..
                    } if content == wanted => Some(*x),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing major tick label {wanted:?}"))
        };
        let x_at_1 = label_x("1");
        let x_at_10 = label_x("10");

        let rects: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|p| match p {
                Prim::Rect { x, w, .. } => Some((*x, *w)),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3, "3 カテゴリ分の bar (A=5, B=500, C=50000)");

        // 0. 独立した基準点: Y 軸(カテゴリ軸)ボーダー線(build_horizontal の "3a" で
        //    plot_left 変数を直接使って描く、xs/ValueScale::Log を一切経由しない線)。
        //    x_at_1 も bar の左端も xs.map() 経由で計算されるため、xs の構築自体
        //    (例えば plot_left/plot_right に誤ったオフセットを混入させるバグ)が
        //    壊れていても、それらは「お互いに」自己無矛盾のままズレて test をすり抜け
        //    得る(mutation testing で実証済み: xs 構築時の plot_left/plot_right への
        //    オフセット注入も、base_v の .clamp(...) 削除も、旧テストは検出できなかった)。
        //    border_x は xs を経由しない独立した描画経路なので、これを ground truth
        //    にすることで「x_at_1 や bar 左端が"本当に"正しい plot_left にあるか」を
        //    内部的な自己無矛盾ではなく検証できる。
        let border_x = scene
            .items
            .iter()
            .find_map(|p| match p {
                Prim::Line {
                    x1,
                    x2,
                    y1,
                    y2,
                    stroke,
                    ..
                } if (x1 - x2).abs() < 1e-9
                    && (y2 - y1).abs() > 10.0
                    && stroke.r == spec.theme.text_color.r
                    && stroke.g == spec.theme.text_color.g
                    && stroke.b == spec.theme.text_color.b =>
                {
                    Some(*x1)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing y-axis (category axis) border line"));
        assert!(
            (x_at_1 - border_x).abs() < 0.5,
            "major tick \"1\" の x={x_at_1} は Y 軸ボーダー線(独立した plot_left 基準)の \
             x={border_x} と一致すべき"
        );

        // 1. 全 bar の左端(baseline)は decade 境界 "1" の x、かつ独立基準の border_x
        //    にも一致する(0 ではない)。
        for &(x, _) in &rects {
            assert!(
                (x - x_at_1).abs() < 0.5,
                "bar の左端 {x} は major tick \"1\" の x={x_at_1} に一致すべき"
            );
            assert!(
                (x - border_x).abs() < 0.5,
                "bar の左端 {x} は Y 軸ボーダー線(独立基準)の x={border_x} に一致すべき"
            );
        }

        // 2. bar A(値=5)の右端は、"1"/"10" ラベル間を log10(5)≈0.69897 で内分した
        //    位置(= mantissa=5 の minor gridline)に一致する。対数写像自体のピン留め。
        let expected_x_at_5 = x_at_1 + (x_at_10 - x_at_1) * 5.0_f64.log10();
        let (bar_a_x, bar_a_w) = rects[0];
        assert!(
            (bar_a_x + bar_a_w - expected_x_at_5).abs() < 0.5,
            "bar A の右端 {} should land on log10-interpolated x={expected_x_at_5}",
            bar_a_x + bar_a_w
        );

        assert!(rects.iter().all(|&(_, w)| w > 0.0 && w.is_finite()));
    }

    #[test]
    fn stacked_data_label_midpoint_uses_pixel_space_not_value_space_under_log_scale() {
        // 積み上げ横棒 + 対数 x 軸、2系列 [10, 90](単一カテゴリ)。
        // 系列2 のセグメントは値空間で [10, 100]。対数軸では log10 が非アフィンなため、
        // 「値空間の中点 (10+100)/2=55 を map したピクセル位置」(旧実装のバグ)と
        // 「セグメント両端を先に map してからピクセル空間で平均する中点」(正しい)は
        // 一致しない。コードレビューで実測: 800px canvas 上で ~184px の誤差。
        let json = r#"{"type":"bar","data":{"labels":["A"],
            "datasets":[{"data":[10]},{"data":[90]}]},
            "options":{"indexAxis":"y",
                "scales":{"x":{"stacked":true,"type":"logarithmic"},"y":{"stacked":true}},
                "plugins":{"datalabels":{"display":true}}}}"#;
        let spec = parse(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);

        assert!(
            scene.items.iter().any(|item| matches!(item,
                Prim::Text { content, .. } if content == "100"
            )),
            "the horizontal log domain should include the stacked total"
        );

        // 2 系列 × 1 カテゴリ → Rect は 2 本。x 昇順に並べると
        // [0]=系列1(値空間 [0,10])、[1]=系列2(値空間 [10,100])。
        let mut rects: Vec<(f64, f64, f64, f64)> = scene
            .items
            .iter()
            .filter_map(|p| match p {
                Prim::Rect { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        rects.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(rects.len(), 2, "1 カテゴリ × 2 系列で 2 本の Rect");
        let (seg2_x, seg2_y, seg2_w, seg2_h) = rects[1];
        let pixel_mid = seg2_x + seg2_w / 2.0; // 修正後の正しい中点(ピクセル空間で平均)

        // 旧実装(バグ)が出す位置を独立に再現する: map(10)/map(100) は既に描画済みの
        // Rect 端から読み取り、その2点間を log10(55) で内分する(map 自体は
        // bars_grow_from_axis_floor_not_zero と同じ log10-補間手法で独立に検証済み)。
        let map10 = seg2_x;
        let map100 = seg2_x + seg2_w;
        let t = (55.0_f64.log10() - 10.0_f64.log10()) / (100.0_f64.log10() - 10.0_f64.log10());
        let buggy_x = map10 + t * (map100 - map10);

        // 実際に描画されたデータラベル("90")の x 座標。x 軸の major tick ラベルにも
        // 偶然 "90" が現れうる(線形軸の nice_ticks 次第)ため、セグメント2の行の
        // 縦範囲 [seg2_y, seg2_y+seg2_h] 内にあるものだけをデータラベルとみなす
        // (軸目盛ラベルは常にプロット領域の外側・下端の固定 y に描かれるため区別できる)。
        let label_x = scene
            .items
            .iter()
            .find_map(|p| match p {
                Prim::Text {
                    x,
                    y,
                    content,
                    anchor: Anchor::Middle,
                    ..
                } if content == "90" && *y >= seg2_y && *y <= seg2_y + seg2_h => Some(*x),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing data label for value 90"));

        // このシナリオでは buggy_x と pixel_mid が有意に(50px 以上)乖離する
        // ことをまず確認する(対数軸で非アフィンなズレが実際に起きる設定であることの担保)。
        assert!(
            (buggy_x - pixel_mid).abs() > 50.0,
            "test scenario should reproduce a large value-space-vs-pixel-space gap: \
             buggy_x={buggy_x} pixel_mid={pixel_mid}"
        );

        // 修正後の実装はピクセル空間中点に一致し、旧バグの位置には一致しない。
        assert!(
            (label_x - pixel_mid).abs() < 0.5,
            "label x={label_x} should match pixel-space segment midpoint={pixel_mid}"
        );
        assert!(
            (label_x - buggy_x).abs() > 50.0,
            "label x={label_x} should NOT match the old value-space-then-map midpoint={buggy_x}"
        );
    }

    #[test]
    fn stacked_data_label_midpoint_unaffected_by_fix_under_linear_scale() {
        // Issue 1 の修正(値空間の中点を map → 先に map してからピクセル空間で平均)は、
        // 線形軸では数学的に無演算(LinearScale.map はアフィン写像なので
        // map((v0+v1)/2) == (map(v0)+map(v1))/2 が常に成立)。
        // 対数軸版のテストと全く同じ構造(同じ値 [10,90]、同じ判定手法)で、
        // 「新実装(pixel_mid)」と「旧実装が出していたはずの位置(buggy_x)」が
        // 線形軸では一致することを直接示す(= 修正が線形パスの挙動を変えていない証明)。
        let json = r#"{"type":"bar","data":{"labels":["A"],
            "datasets":[{"data":[10]},{"data":[90]}]},
            "options":{"indexAxis":"y",
                "scales":{"x":{"stacked":true},"y":{"stacked":true}},
                "plugins":{"datalabels":{"display":true}}}}"#;
        let spec = parse(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);

        let mut rects: Vec<(f64, f64, f64, f64)> = scene
            .items
            .iter()
            .filter_map(|p| match p {
                Prim::Rect { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        rects.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(rects.len(), 2, "1 カテゴリ × 2 系列で 2 本の Rect");
        let (seg2_x, seg2_y, seg2_w, seg2_h) = rects[1];
        let pixel_mid = seg2_x + seg2_w / 2.0;

        // 線形軸での「値空間の中点を map した位置」(旧実装相当)。map10/map100 は
        // 線形なので単純な線形補間で独立に再現できる。
        let map10 = seg2_x;
        let map100 = seg2_x + seg2_w;
        let t = (55.0 - 10.0) / (100.0 - 10.0);
        let buggy_x = map10 + t * (map100 - map10);

        // "90" は線形軸の nice_ticks 目盛りラベルとしても現れうる(このケースで実際に
        // 衝突する)ため、セグメント2の行の縦範囲で絞り込んでデータラベルだけを拾う。
        let label_x = scene
            .items
            .iter()
            .find_map(|p| match p {
                Prim::Text {
                    x,
                    y,
                    content,
                    anchor: Anchor::Middle,
                    ..
                } if content == "90" && *y >= seg2_y && *y <= seg2_y + seg2_h => Some(*x),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing data label for value 90"));

        // 線形軸では pixel_mid と buggy_x が(浮動小数点誤差の範囲で)完全に一致する。
        assert!(
            (buggy_x - pixel_mid).abs() < 1e-6,
            "linear scale: value-space-then-map should equal pixel-space midpoint exactly \
             (affine map): buggy_x={buggy_x} pixel_mid={pixel_mid}"
        );
        assert!(
            (label_x - pixel_mid).abs() < 0.5,
            "label x={label_x} should match pixel-space segment midpoint={pixel_mid}"
        );
    }

    /// 実機バグ回帰テスト: データラベルは `common::value_label` を経由するが、
    /// 対数軸フラグを渡していなかったため常に `fmt_num`(小数2桁丸め)で
    /// フォーマットされ、0.0003 のような対数軸上の正当な小さい値が "0" という
    /// 誤ったラベルになっていた(PR #144 の自動レビューで指摘)。
    #[test]
    fn data_label_uses_fmt_num_log_precision_on_horizontal_log_axis() {
        let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[0.0003]}]},
            "options":{"indexAxis":"y",
                "scales":{"x":{"type":"logarithmic"}},
                "plugins":{"datalabels":{"display":true}}}}"#;
        let spec = parse(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);

        // 横棒(dodge, 非stacked)のデータラベルは正値なら Anchor::Start(bar.rs の
        // `if v >= base_v { (vx + LABEL_GAP, Anchor::Start) }` 参照)。軸目盛ラベルは
        // Anchor::Middle/End なので、値そのもので判定すれば十分区別できる。
        let has_full_precision_label = scene
            .items
            .iter()
            .any(|p| matches!(p, Prim::Text { content, .. } if content == &fmt_num_log(0.0003)));
        assert!(
            has_full_precision_label,
            "対数軸のデータラベルは fmt_num_log の全桁表現(\"0.0003\")を使うべき、\
             fmt_num(丸めで \"0\")ではない"
        );
        assert!(
            !scene
                .items
                .iter()
                .any(|p| matches!(p, Prim::Text { content, .. } if content == "0")),
            "0.0003 のデータラベルが \"0\" に丸められて描画されてはならない"
        );
    }

    #[test]
    fn linear_x_axis_has_no_minor_gridlines_regression() {
        // type 未指定(既定 Linear)では従来通り minor グリッドは出ない(is_log 分岐が
        // 誤って常時発火していないことの回帰確認)。
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,100]}]},
                "options":{"indexAxis":"y"}}"#,
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        // nice_ticks(1,100,10) は 10 刻み程度の major のみで、log の 19 本には遠く及ばない。
        assert!(count_vertical_gridlines(&scene, &spec) < 19);
    }
}
