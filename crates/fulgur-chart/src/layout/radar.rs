//! radar(レーダー)チャート。極座標でカテゴリをスポークに割り当て、系列ごとに
//! 多角形を重ねる。軸・グリッドは多角形状(chart.js 既定)。`common::compute` は
//! 直交カテゴリ x を前提とするため使わず、pie.rs と同様に自前でタイトル・凡例・
//! 極座標ジオメトリを構築する。

use super::common;
use crate::ir::{ChartSpec, Color, LegendPos};
use crate::num::fmt_num;
use crate::scale::nice_ticks;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;
use std::fmt::Write;

/// 系列塗りの不透明度（系列 fill の alpha に乗ずる）。
const SERIES_FILL_ALPHA: f32 = 0.2;
/// 頂点マーカー(点)の半径。
const MARKER_R: f64 = 3.0;
/// 円の外接半径に対する使用率(軸ラベルの余白を残す)。
const RADIUS_RATIO: f64 = 0.8;
/// カテゴリラベルを外周からどれだけ離すか(px)。
const LABEL_OFFSET: f64 = 12.0;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut items: Vec<Prim> = Vec::new();

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 凡例の有無(系列ベース: Top/Bottom/Left/Right かつ名前付き系列が1つ以上)。
    // common::has_legend は private なのでここで判定する。
    let has_legend = matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && spec.series.iter().any(|s| !s.name.is_empty());

    // 1. タイトル。
    let title_band = if spec.title.is_some() {
        common::TITLE_BAND
    } else {
        0.0
    };
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: common::OUTER_PAD + common::TITLE_FONT,
            size: common::TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }

    // 2. 凡例の帯を確保(系列ベース)。
    let legend_top = if has_legend && spec.legend == LegendPos::Top {
        common::LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == LegendPos::Bottom {
        common::LEGEND_BAND
    } else {
        0.0
    };
    let series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
    let legend_left = if has_legend && spec.legend == LegendPos::Left {
        common::legend_band_width_vertical(m, &series_names, label_font)
    } else {
        0.0
    };
    let legend_right = if has_legend && spec.legend == LegendPos::Right {
        common::legend_band_width_vertical(m, &series_names, label_font)
    } else {
        0.0
    };

    // 2a. 凡例(Top/Bottom: 横並び、系列別)。draw_frame の横並び実装に倣う。
    if has_legend && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let mut total = 0.0_f64;
        let n = spec.series.len();
        for (k, ser) in spec.series.iter().enumerate() {
            total += common::legend_entry_width(m, &ser.name, label_font);
            if k == n - 1 {
                total -= 16.0;
            }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == LegendPos::Top {
            common::OUTER_PAD + title_band + common::LEGEND_BAND / 2.0
        } else {
            spec.height - common::OUTER_PAD - common::LEGEND_BAND / 2.0
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
                y: legend_cy + label_font * common::TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Start,
                fill: ink,
                content: ser.name.clone(),
            });
            cursor += common::legend_entry_width(m, &ser.name, label_font);
        }
    }

    // 2b. 凡例(Left/Right: 縦並び、系列別)。
    if has_legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, Color)> = spec
            .series
            .iter()
            .map(|s| (s.name.clone(), s.fill_at(0)))
            .collect();
        let band_w = if spec.legend == LegendPos::Left {
            legend_left
        } else {
            legend_right
        };
        let band_x = if spec.legend == LegendPos::Left {
            common::OUTER_PAD
        } else {
            spec.width - common::OUTER_PAD - band_w
        };
        let area_top = common::OUTER_PAD + title_band + legend_top;
        let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
        common::draw_vertical_legend(
            &mut items,
            &entries,
            band_x,
            area_top,
            area_bottom,
            ink,
            label_font,
        );
    }

    // 3. プロット(円)領域。タイトル・凡例を除いた残り。
    let area_top = common::OUTER_PAD + title_band + legend_top;
    let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
    let area_left = common::OUTER_PAD + legend_left;
    let area_right = spec.width - common::OUTER_PAD - legend_right;
    let cx = (area_left + area_right) / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let radius =
        ((area_right - area_left).min(area_bottom - area_top) / 2.0 * RADIUS_RATIO).max(0.0);

    let n = spec.categories.len();
    if n == 0 || radius <= 0.0 {
        // スポークが無い/領域が無い → タイトル・凡例のみの有効 SVG を返す。
        return Scene {
            width: spec.width,
            height: spec.height,
            items,
        };
    }

    // 各スポークの角度(上始点・時計回り。SVG は y 下向きなので sin が下方向に効き時計回り)。
    let angle = |i: usize| -PI / 2.0 + i as f64 * (2.0 * PI / n as f64);

    // 4. 値スケール。全系列の有限・非負値の最大から nice_ticks(0..max)。
    let mut max_val = 0.0_f64;
    for ser in &spec.series {
        for &v in &ser.values {
            if v.is_finite() && v >= 0.0 && v > max_val {
                max_val = v;
            }
        }
    }
    let nice = nice_ticks(0.0, max_val, 10);
    // 値→半径。nice.max<=0 の縮退は中心へ落とす。
    let rr = |v: f64| -> f64 {
        if nice.max > 0.0 {
            (v / nice.max) * radius
        } else {
            0.0
        }
    };

    // 5. グリッド(多角形状)。tick レベルごとに n 頂点を結ぶ閉多角形を描く。
    for &t in &nice.ticks {
        if t <= 0.0 {
            continue;
        }
        let r = rr(t);
        let mut d = String::new();
        for i in 0..n {
            let a = angle(i);
            let x = cx + r * a.cos();
            let y = cy + r * a.sin();
            let cmd = if i == 0 { 'M' } else { 'L' };
            write!(d, "{} {} {} ", cmd, fmt_num(x), fmt_num(y)).unwrap();
        }
        d.push('Z');
        items.push(Prim::Path {
            d,
            fill: None,
            stroke: Some(spec.theme.grid_color),
            stroke_width: 1.0,
        });
    }

    // スポーク線(中心→各外周頂点)。
    for i in 0..n {
        let a = angle(i);
        items.push(Prim::Line {
            x1: cx,
            y1: cy,
            x2: cx + radius * a.cos(),
            y2: cy + radius * a.sin(),
            stroke: spec.theme.grid_color,
            stroke_width: 1.0,
        });
    }

    // 6. 系列(重ねる多角形)。入力順で描く。
    for ser in &spec.series {
        let verts: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let v = ser.values.get(i).copied().unwrap_or(0.0);
                let v = if v.is_finite() { v } else { 0.0 };
                let a = angle(i);
                let r = rr(v);
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();

        // 多角形(閉path)。半透明塗り + 系列ストローク。
        let mut d = String::new();
        for (i, (x, y)) in verts.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            write!(d, "{} {} {} ", cmd, fmt_num(*x), fmt_num(*y)).unwrap();
        }
        d.push('Z');
        let f = ser.fill_at(0);
        let area_fill = Color {
            a: f.a * SERIES_FILL_ALPHA,
            ..f
        };
        items.push(Prim::Path {
            d,
            fill: Some(area_fill),
            stroke: Some(ser.stroke_at(0)),
            stroke_width: ser.stroke_width,
        });

        // 頂点マーカー。
        for (x, y) in &verts {
            items.push(Prim::Circle {
                cx: *x,
                cy: *y,
                r: MARKER_R,
                fill: ser.stroke_at(0),
            });
        }
    }

    // 7. カテゴリ(軸)ラベル。各スポーク外端の少し外側に描く。
    let label_r = radius + LABEL_OFFSET;
    for (i, cat) in spec.categories.iter().enumerate() {
        if cat.is_empty() {
            continue;
        }
        let a = angle(i);
        let lx = cx + label_r * a.cos();
        let ly = cy + label_r * a.sin();
        // アンカーは角度の cos で決める(右半=Start, 左半=End, 上下付近=Middle)。
        let c = a.cos();
        let anchor = if c > 0.1 {
            Anchor::Start
        } else if c < -0.1 {
            Anchor::End
        } else {
            Anchor::Middle
        };
        items.push(Prim::Text {
            x: lx,
            y: ly + label_font * common::TEXT_BASELINE_RATIO,
            size: label_font,
            anchor,
            fill: ink,
            content: cat.clone(),
        });
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
