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
/// 非積み上げ: category 外側 × series 内側で全 (i,sidx) を生成(欠損値は `unwrap_or(0.0)`、
///   present な非有限値は build_vertical と同様 NaN 矩形になる)。
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
        for i in 0..spec.categories.len() {
            let band_left = super::common::category_center(frame, i, n) - band_w / 2.0;
            for (sidx, ser) in spec.series.iter().enumerate() {
                let bx = band_left + band_w * BAND_PAD_RATIO + sidx as f64 * bar_w;
                let v = ser.values.get(i).copied().unwrap_or(0.0);
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
        } else if ser.values.get(b.index).is_some() && b.value.is_finite() {
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

    let plot_left = OUTER_PAD + cat_w + legend_left;
    let plot_right = spec.width - OUTER_PAD - legend_right;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom;

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

    // 2. 縦グリッド + 値ラベル(下)。
    for &t in &ticks.ticks {
        let x = xs.map(t);
        items.push(Prim::Line {
            x1: x,
            y1: plot_top,
            x2: x,
            y2: plot_bottom,
            stroke: spec.theme.grid_color,
            stroke_width: 1.0,
            dash: Vec::new(),
        });
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

    // 3. 左軸線(カテゴリ軸)。
    items.push(Prim::Line {
        x1: plot_left,
        y1: plot_top,
        x2: plot_left,
        y2: plot_bottom,
        stroke: ink,
        stroke_width: 1.0,
        dash: Vec::new(),
    });

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
            for (sidx, ser) in spec.series.iter().enumerate() {
                let by = band_top + band_h * BAND_PAD_RATIO + sidx as f64 * bar_h;
                let v = ser.values.get(i).copied().unwrap_or(0.0);
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
                if spec.data_labels && ser.values.get(i).is_some() && v.is_finite() {
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
            band_x,
            plot_top,
            plot_bottom,
            ink,
            label_font,
        );
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
}
