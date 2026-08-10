//! bar チャートのレイアウト: ChartSpec → Scene。
//! 縦棒・横棒に対応。決定的に組み立て、NaN/Inf/panic を出さない。

use crate::ir::ChartSpec;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

/// band 内のグループ幅比。
const GROUP_RATIO: f64 = 0.8;
/// band 左右パディング比。
const BAND_PAD_RATIO: f64 = 0.1;
/// bar 幅の塗り比。
const BAR_FILL_RATIO: f64 = 0.9;
/// 極端に長い目盛ラベルでも LinearScale のプロット幅を 0 にしない下限。
const MIN_HORIZONTAL_PLOT_WIDTH: f64 = 1.0;

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

/// 縦棒の全データ矩形を build_vertical と同一の式で算出する単一の真実源。
/// レンダラ(`build_vertical`)とモデル(`model::Geometry`)の両方がこれを呼ぶ。
/// 非積み上げ (dodge): category 外側 × series 内側で有限値のみ box を生成する。
///   欠損値 (get() None) と非有限値 (NaN / ±∞) は skip され、box は emit されない。
/// 積み上げ: category 外側 × series 内側で有限値のみ値空間に積む。
pub fn vertical_bar_boxes(spec: &ChartSpec, frame: &super::common::Frame) -> Vec<BarBox> {
    let n = spec.categories.len().max(1);
    let band_w = super::common::band_width(frame, n);
    let s = spec.series.len().max(1);
    let group_w = band_w * GROUP_RATIO;
    let bar_w = group_w / s as f64;
    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let baseline_y = frame.ys.map(base_v);
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

    let mut boxes = Vec::new();
    if placement_stacked && value_stacked {
        // 同スロット + 値累積(従来の stacked=true の挙動)
        let stack_w = (group_w * BAR_FILL_RATIO).max(0.0);
        for i in 0..spec.categories.len() {
            let band_left = super::common::category_center(frame, i, n) - band_w / 2.0;
            let bx = band_left + band_w * BAND_PAD_RATIO;
            let mut pos_acc = 0.0_f64;
            let mut neg_acc = 0.0_f64;
            for (sidx, ser) in spec.series.iter().enumerate() {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let (v0, v1) = if v >= 0.0 {
                    let lo = pos_acc;
                    pos_acc += v;
                    (lo, pos_acc)
                } else {
                    let hi = neg_acc;
                    neg_acc += v;
                    (neg_acc, hi)
                };
                let y0 = frame.ys.map(v0);
                let y1 = frame.ys.map(v1);
                let y_top = y0.min(y1);
                let h = (y1 - y0).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: stack_w,
                    h,
                });
            }
        }
    } else if placement_stacked {
        // 同スロット + 各系列を baseline から描画(chart.js の index-only stacked 挙動)
        // 系列は重なる。値域は dodge と同じ個別値(value_stacked=false)。
        let stack_w = (group_w * BAR_FILL_RATIO).max(0.0);
        for i in 0..spec.categories.len() {
            let band_left = super::common::category_center(frame, i, n) - band_w / 2.0;
            let bx = band_left + band_w * BAND_PAD_RATIO;
            for (sidx, ser) in spec.series.iter().enumerate() {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let vy = frame.ys.map(v);
                let y_top = vy.min(baseline_y);
                let h = (vy - baseline_y).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: stack_w,
                    h,
                });
            }
        }
    } else {
        // dodge 配置(従来の stacked=false の挙動)
        // value_stacked=true のとき値域は value_domain が担当するため geometry は変わらない。
        // 非有限値(null→NaN も含む)はギャップとしてスキップ。
        for i in 0..spec.categories.len() {
            let band_left = super::common::category_center(frame, i, n) - band_w / 2.0;
            for (sidx, ser) in spec.series.iter().enumerate() {
                let bx = band_left + band_w * BAND_PAD_RATIO + sidx as f64 * bar_w;
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let vy = frame.ys.map(v);
                let y_top = vy.min(baseline_y);
                let h = (vy - baseline_y).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: (bar_w * BAR_FILL_RATIO).max(0.0),
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
fn horizontal_legend_band_width(m: &TextMeasurer, names: &[String], font_size: f64) -> f64 {
    let max_width = names
        .iter()
        .map(|name| finite_text_width(m, name, font_size))
        .fold(0.0, f64::max);
    12.0 + 4.0 + max_width + 16.0
}

/// 横棒の値軸端ラベル用のプロット境界を算出する。
///
/// 端のラベルは中央寄せなので、左端と右端を別々に余白化する。右端は最後の
/// tick の幅だけを使い、左端はラベルが canvas の左端を越える場合にだけ補う。
/// 基準境界は canvas 内へ正規化し、余白が大きすぎる有限値では比例縮小する。
/// LinearScale が全値を同一点へ写すのを防ぐため、最低限のプロット幅を残す。
fn horizontal_plot_bounds(
    base_left: f64,
    base_right: f64,
    canvas_width: f64,
    ticks: &[f64],
    m: &TextMeasurer,
    label_font: f64,
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
    let half_tick_width = |tick: f64| {
        let label = crate::num::fmt_num(tick);
        finite_text_width(m, &label, label_font) / 2.0
    };
    let left_pad = ticks
        .first()
        .map(|&tick| (half_tick_width(tick) - base_left).max(0.0))
        .unwrap_or(0.0);
    let right_pad = ticks
        .last()
        .map(|&tick| half_tick_width(tick))
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
    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
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
    for b in vertical_bar_boxes(spec, &frame) {
        let ser = &spec.series[b.series];
        items.push(Prim::Rect {
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
            fill: ser.fill_at(b.index),
        });
        if !spec.data_labels {
            continue;
        }
        let cx = b.x + b.w / 2.0;
        if stacked {
            // セグメント中央(box 中心 = 値中点; ys は線形なので一致)に値ラベル。
            let mid_y = b.y + b.h / 2.0;
            items.push(value_label(
                cx,
                mid_y + label_font * super::common::TEXT_BASELINE_RATIO,
                label_font,
                Anchor::Middle,
                ink,
                b.value,
            ));
        } else {
            // 正は上端の少し上(- LABEL_GAP)、負は下端の下にラベル。負側は
            // LABEL_GAP ではなく + label_font(≒1行高)を足すのは、SVG の y が
            // ベースラインで字面が上に伸びるため、僅かな隙間だと棒下端に重なるから。
            // この上下非対称(- LABEL_GAP / + label_font)は意図的。
            let label_y = if b.value >= base_v {
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
    use crate::layout::common::*;
    use crate::num::fmt_num;
    use crate::scale::{LinearScale, nice_ticks};
    use crate::scene::Anchor;

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 横棒は値軸が x のため x_axis を渡す（begin_at_zero/suggested も x_axis から読む）。
    let (dmin, dmax) = value_domain(spec, &spec.x_axis);
    let ticks = nice_ticks(dmin, dmax, 10);
    // カテゴリラベル幅(左軸): 各 categories の最大幅 + 10。空なら最低でも 10。
    let mut max_cat_w = 0.0_f64;
    for c in &spec.categories {
        let w = finite_text_width(m, c, label_font);
        if w > max_cat_w {
            max_cat_w = w;
        }
    }
    let cat_w = max_cat_w + 10.0;

    // 凡例の有無(縦棒と同じ判定: Top/Bottom/Left/Right かつ名前付き系列あり)。
    let has_legend = matches!(
        spec.legend,
        crate::ir::LegendPos::Top
            | crate::ir::LegendPos::Bottom
            | crate::ir::LegendPos::Left
            | crate::ir::LegendPos::Right
    ) && spec.series.iter().any(|s| !s.name.is_empty());

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_top = if has_legend && spec.legend == crate::ir::LegendPos::Top {
        LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == crate::ir::LegendPos::Bottom {
        LEGEND_BAND
    } else {
        0.0
    };
    // Left/Right の凡例帯幅(系列名から算出)。
    let series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
    let legend_left = if has_legend && spec.legend == crate::ir::LegendPos::Left {
        horizontal_legend_band_width(m, &series_names, label_font)
    } else {
        0.0
    };
    let legend_right = if has_legend && spec.legend == crate::ir::LegendPos::Right {
        horizontal_legend_band_width(m, &series_names, label_font)
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
    );
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;

    // 値→X(非反転)。
    let xs = LinearScale::new(ticks.min, ticks.max, plot_left, plot_right);

    let mut items: Vec<Prim> = Vec::new();

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
    for &t in &ticks.ticks {
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
            content: fmt_num(t),
            rotate_deg: None,
        });
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
    const TICK_LEN: f64 = 4.0;
    if x_grid_cfg.draw_ticks {
        let tick_color = x_grid_cfg.color.unwrap_or(ink);
        for &t in &ticks.ticks {
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

    // 4. カテゴリ band と 横棒。
    let n = spec.categories.len().max(1);
    let band_h = (plot_bottom - plot_top) / n as f64;
    let s = spec.series.len().max(1);
    let group_h = band_h * GROUP_RATIO;
    let bar_h = group_h / s as f64;

    let base_v = 0.0_f64.clamp(ticks.min, ticks.max);
    let baseline_x = xs.map(base_v);

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

    for i in 0..spec.categories.len() {
        let band_top = plot_top + i as f64 * band_h;
        let center_y = band_top + band_h / 2.0;

        // カテゴリラベル(左)。
        if !spec.categories[i].is_empty() {
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
            // 同スロット + 値累積(従来の横棒 stacked 挙動)
            let stack_h = (group_h * BAR_FILL_RATIO).max(0.0);
            let by = band_top + band_h * BAND_PAD_RATIO;
            let cy = by + stack_h / 2.0 + label_font * TEXT_BASELINE_RATIO;
            let mut pos_acc = 0.0_f64;
            let mut neg_acc = 0.0_f64;
            for ser in &spec.series {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let (v0, v1) = if v >= 0.0 {
                    let lo = pos_acc;
                    pos_acc += v;
                    (lo, pos_acc)
                } else {
                    let hi = neg_acc;
                    neg_acc += v;
                    (neg_acc, hi)
                };
                let x0 = xs.map(v0);
                let x1 = xs.map(v1);
                let x = x0.min(x1);
                let w = (x1 - x0).abs();
                items.push(Prim::Rect {
                    x,
                    y: by,
                    w,
                    h: stack_h,
                    fill: ser.fill_at(i),
                });
                if spec.data_labels {
                    // セグメント中央(値中点)に値ラベルを置く。
                    let mid_x = xs.map((v0 + v1) / 2.0);
                    items.push(value_label(mid_x, cy, label_font, Anchor::Middle, ink, v));
                }
            }
        } else if placement_stacked {
            // 同スロット + 各 baseline から描画(横棒 index-only stacked)
            let stack_h = (group_h * BAR_FILL_RATIO).max(0.0);
            let by = band_top + band_h * BAND_PAD_RATIO;
            for ser in &spec.series {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let vx = xs.map(v);
                let x = vx.min(baseline_x);
                let w = (vx - baseline_x).abs();
                items.push(Prim::Rect {
                    x,
                    y: by,
                    w,
                    h: stack_h,
                    fill: ser.fill_at(i),
                });
                if spec.data_labels {
                    let cy = by + stack_h / 2.0 + label_font * TEXT_BASELINE_RATIO;
                    let (cx, anchor) = if v >= base_v {
                        (vx + LABEL_GAP, Anchor::Start)
                    } else {
                        (vx - LABEL_GAP, Anchor::End)
                    };
                    items.push(value_label(cx, cy, label_font, anchor, ink, v));
                }
            }
        } else {
            // dodge 配置(従来の stacked=false 挙動)
            // 非有限値(null→NaN も含む)はギャップとしてスキップ。
            for (sidx, ser) in spec.series.iter().enumerate() {
                let by = band_top + band_h * BAND_PAD_RATIO + sidx as f64 * bar_h;
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let vx = xs.map(v);
                let x = vx.min(baseline_x);
                let w = (vx - baseline_x).abs();
                items.push(Prim::Rect {
                    x,
                    y: by,
                    w,
                    h: (bar_h * BAR_FILL_RATIO).max(0.0),
                    fill: ser.fill_at(i),
                });
                if spec.data_labels {
                    let cy = by + (bar_h * BAR_FILL_RATIO) / 2.0 + label_font * TEXT_BASELINE_RATIO;
                    // 正は棒右端の右(Start)、負は左端の左(End)に LABEL_GAP 分離す。
                    let (lx, anchor) = if v >= base_v {
                        (vx + LABEL_GAP, Anchor::Start)
                    } else {
                        (vx - LABEL_GAP, Anchor::End)
                    };
                    items.push(value_label(lx, cy, label_font, anchor, ink, v));
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
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = legend_entry_width(m, &ser.name, label_font);
            total += ew;
            if k == spec.series.len() - 1 {
                total -= 16.0;
            }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == crate::ir::LegendPos::Top {
            OUTER_PAD + title_band + LEGEND_BAND / 2.0
        } else {
            spec.height - OUTER_PAD - LEGEND_BAND / 2.0
        };
        let mut cursor = start_x;
        for ser in &spec.series {
            items.push(Prim::Rect {
                x: cursor,
                y: legend_cy - 6.0,
                w: 12.0,
                h: 12.0,
                fill: ser.fill_at(0),
            });
            items.push(Prim::Text {
                x: cursor + 16.0,
                y: legend_cy + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Start,
                fill: ink,
                content: ser.name.clone(),
                rotate_deg: None,
            });
            cursor += legend_entry_width(m, &ser.name, label_font);
        }
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
        draw_vertical_legend(
            &mut items,
            &entries,
            None,
            band_x,
            plot_top,
            plot_bottom,
            ink,
            label_font,
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

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

#[cfg(test)]
mod geom_tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::text::TextMeasurer;

    fn boxes_for(json: &str) -> Vec<BarBox> {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        vertical_bar_boxes(&spec, &frame)
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
    fn box_height_tracks_value_magnitude() {
        // 値が大きいほど高い矩形(baseline=0)。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,100]}]}}"#,
        );
        assert!(bs[1].h > bs[0].h);
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
}

#[cfg(test)]
mod horizontal_axis_style_tests {
    //! 横棒(indexAxis:"y") のグリッド/ボーダー/軸タイトル反映テスト。
    //! ChartJS フロントエンドを経由して spec を組む(scales.x/y と options.plugins.title を直に指定できる)。

    use super::{
        MIN_HORIZONTAL_PLOT_WIDTH, build, finite_text_width, horizontal_legend_band_width,
        horizontal_plot_bounds,
    };
    use crate::font::DEFAULT_FONT;
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

    fn horizontal_plot_right(spec: &ChartSpec, m: &TextMeasurer<'_>) -> f64 {
        let (dmin, dmax) = value_domain(spec, &spec.x_axis);
        let ticks = nice_ticks(dmin, dmax, 10);
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
            horizontal_legend_band_width(m, &series_names, spec.theme.font_size)
        } else {
            0.0
        };
        let legend_right = if has_legend && spec.legend == crate::ir::LegendPos::Right {
            horizontal_legend_band_width(m, &series_names, spec.theme.font_size)
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
        )
        .1
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
