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
    let is_log = spec.y_axis.scale_kind == crate::ir::ScaleKind::Logarithmic;
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
    use crate::ir::ScaleKind;
    use crate::layout::common::*;
    use crate::num::fmt_num;
    use crate::scale::{LinearScale, NiceTicks, ValueScale, nice_ticks};
    use crate::scene::Anchor;

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 横棒は値軸が x のため x_axis を渡す（begin_at_zero/suggested も x_axis から読む）。
    let (dmin, dmax) = value_domain(spec, &spec.x_axis);
    let is_log = spec.x_axis.scale_kind == ScaleKind::Logarithmic;
    let (ticks, minor_ticks) = if is_log {
        let log = crate::scale::log_ticks(dmin, dmax);
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
        (nice_ticks(dmin, dmax, 10), Vec::new())
    };

    // カテゴリラベル幅(左軸): 各 categories の最大幅 + 10。空なら最低でも 10。
    let mut max_cat_w = 0.0_f32;
    for c in &spec.categories {
        let w = m.width(c, label_font as f32);
        if w > max_cat_w {
            max_cat_w = w;
        }
    }
    let cat_w = max_cat_w as f64 + 10.0;

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
        legend_band_width_vertical(m, &series_names, label_font)
    } else {
        0.0
    };
    let legend_right = if has_legend && spec.legend == crate::ir::LegendPos::Right {
        legend_band_width_vertical(m, &series_names, label_font)
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
    let plot_left = OUTER_PAD + cat_w + y_title_w + legend_left;
    let plot_right = spec.width - OUTER_PAD - legend_right;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;

    // 値→X(非反転)。対数軸は log10 空間の LinearScale を内側に持つ ValueScale::Log。
    // ticks.min/max は log_ticks が返す 10^n の decade 境界(常に正)なので、
    // その log10() は有限かつ ticks.min < ticks.max(log_ticks は hi_exp > lo_exp を保証)。
    let xs = if is_log {
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
            content: if is_log {
                crate::num::fmt_num_log(t)
            } else {
                fmt_num(t)
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
                    // セグメント中央(box 中心)に値ラベルを置く。x0/x1 は既に xs で
                    // 写像済みのピクセル空間なので、ここで平均する(ピクセル空間の中点)。
                    // 値空間で (v0+v1)/2.0 を先に計算してから map すると、対数軸では
                    // log10 が非アフィンなためピクセル中点とズレる(線形軸ではアフィン
                    // 写像なので数学的に一致するが、対数軸では誤った位置になる)。
                    let mid_x = (x0 + x1) / 2.0;
                    items.push(value_label(
                        mid_x,
                        cy,
                        label_font,
                        Anchor::Middle,
                        ink,
                        v,
                        is_log,
                    ));
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
                    items.push(value_label(cx, cy, label_font, anchor, ink, v, is_log));
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

    use super::build;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::ir::ChartSpec;
    use crate::layout::common::{OUTER_PAD, X_LABEL_BAND};
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
        let plot_right = spec.width - OUTER_PAD;
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
        let plot_right = visible_spec.width - OUTER_PAD;
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
}

#[cfg(test)]
mod horizontal_log_scale_tests {
    //! 横棒(indexAxis:"y")の対数 X 軸: major/minor grid, log-aware ラベル, tick 刻み,
    //! baseline(bar が軸下端から生える)を検証する。Task 9(common.rs::compute()/draw_frame(),
    //! 縦軸)と対になる、build_horizontal 専用の対数分岐テスト。

    use super::build;
    use crate::font::DEFAULT_FONT;
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
        // データ 5..50000 → decade 境界は 1..100000(6 major tick)。
        let expected: Vec<String> = ["1", "10", "100", "1000", "10000", "100000"]
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
        let spec = parse(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,100]}]},
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic"}}}}"#,
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
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic","grid":{"display":false}}}}}"#,
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
                "options":{"indexAxis":"y","scales":{"x":{"type":"logarithmic","grid":{"drawTicks":true}}}}}"#,
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
        // decade 境界)に評価される(0.0 は決して正のドメインに含まれないため)。
        // よって全ての bar は左端(plot_left = xs.map(ticks.min))から生える。
        // Task 11 Step 3: この行は変更していないので、その挙動を実測で確認する。
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
        //
        // 「対数軸の値軸 × value_stacked」の組み合わせは frontend::chartjs::parse が
        // 現在は明示エラーで拒否する(log_value_domain がスタック合計を計算しないため
        // ドメインが過小になる別バグ、fulgur-chart-bap 参照)。この layout レベルの
        // テストが検証したいのは build_horizontal 自体のピクセル空間中点計算の正しさ
        // であり、frontend の禁止とは独立した性質(bindings 等で ChartSpec を直接
        // 組み立てた場合にも成り立つべき)なので、まず対数軸なしで parse させてから
        // scale_kind だけを直接差し替えて対数軸 + stacked の ChartSpec を作る。
        let json = r#"{"type":"bar","data":{"labels":["A"],
            "datasets":[{"data":[10]},{"data":[90]}]},
            "options":{"indexAxis":"y",
                "scales":{"x":{"stacked":true},"y":{"stacked":true}},
                "plugins":{"datalabels":{"display":true}}}}"#;
        let mut spec = parse(json);
        spec.x_axis.scale_kind = ScaleKind::Logarithmic;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);

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
