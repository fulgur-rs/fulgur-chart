//! bar/line が共有するプロット領域・軸・グリッド・凡例の構築。

use crate::ir::{ChartSpec, Color, LegendPos};
use crate::num::fmt_num;
use crate::scale::{LinearScale, NiceTicks, nice_ticks};
use crate::scene::{Anchor, Prim};
use crate::text::TextMeasurer;

pub const OUTER_PAD: f64 = 8.0;
pub const TITLE_FONT: f64 = 16.0;
pub const LABEL_FONT: f64 = 12.0;
pub const TITLE_BAND: f64 = 28.0;
pub const LEGEND_BAND: f64 = 26.0;
/// 縦置き凡例(Left/Right)の 1 行の高さ(px)。
pub const LEGEND_ROW_H: f64 = 18.0;
pub const X_LABEL_BAND: f64 = 22.0;
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
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
    pub ticks: NiceTicks,
    pub ys: LinearScale,
}

/// 凡例の有無を判定する（Top/Bottom/Left/Right かつ名前付き系列が 1 つ以上）。
fn has_legend(spec: &ChartSpec) -> bool {
    matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && spec.series.iter().any(|s| !s.name.is_empty())
}

/// 値ドメイン(begin_at_zero尊重・空データ→0..1・縮退補正)を算出する。
/// 縦棒(compute)と横棒(build_horizontal)が同一の値域計算を共有する。
pub fn value_domain(spec: &ChartSpec) -> (f64, f64) {
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
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
    if !data_min.is_finite() || !data_max.is_finite() {
        data_min = 0.0;
        data_max = 1.0;
    }
    let (domain_min, mut domain_max) = if spec.y_axis.begin_at_zero {
        (data_min.min(0.0), data_max.max(0.0))
    } else {
        (data_min, data_max)
    };
    // 上限>下限を保証（縮退時の保険）。
    if domain_max <= domain_min {
        domain_max = domain_min + 1.0;
    }
    (domain_min, domain_max)
}

/// spec から y ドメイン(begin_at_zero尊重)・nice_ticks・y軸ラベル幅・プロット領域・凡例帯を計算。
pub fn compute(spec: &ChartSpec, m: &TextMeasurer) -> Frame {
    // y ドメイン。
    let (domain_min, domain_max) = value_domain(spec);
    let ticks = nice_ticks(domain_min, domain_max, 5);

    // y 軸ラベル幅。
    let mut max_w = 0.0_f32;
    for &t in &ticks.ticks {
        let s = fmt_num(t);
        let w = m.width(&s, LABEL_FONT as f32);
        if w > max_w {
            max_w = w;
        }
    }
    let y_axis_w = max_w as f64 + 10.0;

    // 凡例の有無。
    let legend = has_legend(spec);

    // プロット領域。
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let legend_top = if legend && spec.legend == LegendPos::Top {
        LEGEND_BAND
    } else {
        0.0
    };
    let legend_bottom = if legend && spec.legend == LegendPos::Bottom {
        LEGEND_BAND
    } else {
        0.0
    };
    // Left/Right の凡例帯幅(系列名から算出)。Top/Bottom 時は 0。
    let series_names: Vec<String> = spec.series.iter().map(|s| s.name.clone()).collect();
    let legend_left = if legend && spec.legend == LegendPos::Left {
        legend_band_width_vertical(m, &series_names)
    } else {
        0.0
    };
    let legend_right = if legend && spec.legend == LegendPos::Right {
        legend_band_width_vertical(m, &series_names)
    } else {
        0.0
    };
    let plot_left = OUTER_PAD + y_axis_w + legend_left;
    let plot_right = spec.width - OUTER_PAD - legend_right;
    let plot_top = OUTER_PAD + title_band + legend_top;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom;

    // y スケール（上下反転）。
    let ys = LinearScale::new(ticks.min, ticks.max, plot_bottom, plot_top);

    Frame {
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
        ticks,
        ys,
    }
}

/// n カテゴリ中 i 番目の x 中心。band_w=(plot_right-plot_left)/n。
pub fn category_center(frame: &Frame, i: usize, n: usize) -> f64 {
    let band_w = (frame.plot_right - frame.plot_left) / n.max(1) as f64;
    frame.plot_left + (i as f64 + 0.5) * band_w
}

pub fn band_width(frame: &Frame, n: usize) -> f64 {
    (frame.plot_right - frame.plot_left) / n.max(1) as f64
}

/// 共有フレーム描画: タイトル→横グリッド+yラベル→xベースライン→xカテゴリラベル→凡例。
/// チャート本体(bar/line)はこの後に重ねて描く。
pub fn draw_frame(items: &mut Vec<Prim>, spec: &ChartSpec, frame: &Frame, m: &TextMeasurer) {
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

    // 1. タイトル。
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: INK,
            content: title.clone(),
        });
    }

    // 2. 横グリッド + y 軸ラベル。
    for &t in &frame.ticks.ticks {
        let y = frame.ys.map(t);
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: y,
            x2: frame.plot_right,
            y2: y,
            stroke: GRID,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x: frame.plot_left - 6.0,
            y: y + LABEL_FONT * TEXT_BASELINE_RATIO,
            size: LABEL_FONT,
            anchor: Anchor::End,
            fill: INK,
            content: fmt_num(t),
        });
    }

    // 3. x ベースライン。
    items.push(Prim::Line {
        x1: frame.plot_left,
        y1: frame.plot_bottom,
        x2: frame.plot_right,
        y2: frame.plot_bottom,
        stroke: INK,
        stroke_width: 1.0,
    });

    // 4. x カテゴリラベル。
    let n = spec.categories.len().max(1);
    for (i, cat) in spec.categories.iter().enumerate() {
        if !cat.is_empty() {
            items.push(Prim::Text {
                x: category_center(frame, i, n),
                y: frame.plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
                size: LABEL_FONT,
                anchor: Anchor::Middle,
                fill: INK,
                content: cat.clone(),
            });
        }
    }

    // 5. 凡例(Top/Bottom: 横並び)。
    if has_legend(spec) && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        // 各エントリ幅と合計（末尾間隔 16 を最後だけ除く）。
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = legend_entry_width(m, &ser.name);
            total += ew;
            if k == spec.series.len() - 1 {
                total -= 16.0;
            }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == LegendPos::Top {
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
                y: legend_cy + LABEL_FONT * TEXT_BASELINE_RATIO,
                size: LABEL_FONT,
                anchor: Anchor::Start,
                fill: INK,
                content: ser.name.clone(),
            });
            let ew = legend_entry_width(m, &ser.name);
            cursor += ew;
        }
    }

    // 5b. 凡例(Left/Right: 縦並び)。
    if has_legend(spec) && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, Color)> = spec
            .series
            .iter()
            .map(|s| (s.name.clone(), s.fill_at(0)))
            .collect();
        let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
        let band_w = legend_band_width_vertical(m, &names);
        let band_x = if spec.legend == LegendPos::Left {
            OUTER_PAD
        } else {
            spec.width - OUTER_PAD - band_w
        };
        draw_vertical_legend(
            items,
            &entries,
            band_x,
            frame.plot_top,
            frame.plot_bottom,
            m,
        );
    }
}

/// 縦置き凡例(Left/Right)の帯幅: swatch(12) + gap(4) + 最大ラベル幅 + パディング(16)。
/// 名前が空の系列も含めて算出する(レイアウトの確保量を呼び出し側と一致させるため)。
pub fn legend_band_width_vertical(m: &TextMeasurer, names: &[String]) -> f64 {
    let mut max_w = 0.0_f64;
    for name in names {
        let w = m.width(name, LABEL_FONT as f32) as f64;
        if w > max_w {
            max_w = w;
        }
    }
    12.0 + 4.0 + max_w + 16.0
}

/// 縦置き凡例(Left/Right)を描く。entries は (名前, swatch色) の解決済みペア。
/// プロットの縦スパン中央にエントリ群を配置する。
pub fn draw_vertical_legend(
    items: &mut Vec<Prim>,
    entries: &[(String, Color)],
    band_x: f64,
    plot_top: f64,
    plot_bottom: f64,
    _m: &TextMeasurer,
) {
    let n = entries.len();
    let group_h = n as f64 * LEGEND_ROW_H;
    let start_y = (plot_top + plot_bottom) / 2.0 - group_h / 2.0;
    for (i, (name, color)) in entries.iter().enumerate() {
        let row_top = start_y + i as f64 * LEGEND_ROW_H;
        let row_center = row_top + LEGEND_ROW_H / 2.0;
        items.push(Prim::Rect {
            x: band_x,
            y: row_center - 6.0,
            w: 12.0,
            h: 12.0,
            fill: *color,
        });
        items.push(Prim::Text {
            x: band_x + 16.0,
            y: row_center + LABEL_FONT * TEXT_BASELINE_RATIO,
            size: LABEL_FONT,
            anchor: Anchor::Start,
            fill: INK,
            content: name.clone(),
        });
    }
}

/// 凡例 1 エントリの占有幅: swatch幅(12) + gap(4) + ラベル幅 + trailing間隔(16)。
pub fn legend_entry_width(m: &TextMeasurer, name: &str) -> f64 {
    12.0 + 4.0 + m.width(name, LABEL_FONT as f32) as f64 + 16.0
}

/// 値ラベルの Prim::Text を生成する(フォント=LABEL_FONT、内容=fmt_num(v))。
/// 全チャート種でデータラベル生成を一元化する。x/y/anchor/fill は呼び出し側が決める。
pub fn value_label(x: f64, y: f64, anchor: Anchor, fill: Color, v: f64) -> Prim {
    Prim::Text {
        x,
        y,
        size: LABEL_FONT,
        anchor,
        fill,
        content: fmt_num(v),
    }
}
