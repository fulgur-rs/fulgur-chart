//! bar/line が共有するプロット領域・軸・グリッド・凡例の構築。

use crate::ir::{AxisSpec, ChartKind, ChartSpec, Color, LegendPos};
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
pub fn value_domain(spec: &ChartSpec, axis: &AxisSpec) -> (f64, f64) {
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
    if matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        }
    ) {
        // 積み上げ: カテゴリごとに正値の和(上限)・負値の和(下限)をとる。
        // chart.js 互換: beginAtZero=false のとき 0 ではなく実データの個別値を境界にする。
        // 全正値ケース(neg_sum が常に 0)では min_individual を下限として使う。
        // 全負値ケース(pos_sum が常に 0)では max_individual を上限として使う。
        let mut has_positive = false;
        let mut has_negative = false;
        let mut min_individual = f64::INFINITY;
        let mut max_individual = f64::NEG_INFINITY;
        for i in 0..spec.categories.len() {
            let mut pos_sum = 0.0_f64;
            let mut neg_sum = 0.0_f64;
            for ser in &spec.series {
                if let Some(&v) = ser.values.get(i)
                    && v.is_finite()
                {
                    if v < min_individual {
                        min_individual = v;
                    }
                    if v > max_individual {
                        max_individual = v;
                    }
                    if v >= 0.0 {
                        pos_sum += v;
                        has_positive = true;
                    } else {
                        neg_sum += v;
                        has_negative = true;
                    }
                }
            }
            if pos_sum > data_max {
                data_max = pos_sum;
            }
            if neg_sum < data_min {
                data_min = neg_sum;
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
    // データなし: suggested を初期シードとして使う(chart.js 互換)。suggested もなければ 0..1。
    if !data_min.is_finite() || !data_max.is_finite() {
        let lo = axis.suggested_min.filter(|s| s.is_finite()).unwrap_or(0.0);
        let hi = axis
            .suggested_max
            .filter(|s| s.is_finite())
            .unwrap_or(if lo == 0.0 { 1.0 } else { lo + 1.0 });
        let lo = if axis.begin_at_zero { lo.min(0.0) } else { lo };
        let hi = if axis.begin_at_zero { hi.max(0.0) } else { hi };
        return (lo, if hi > lo { hi } else { lo + 1.0 });
    }
    let (mut domain_min, mut domain_max) = if axis.begin_at_zero {
        (data_min.min(0.0), data_max.max(0.0))
    } else {
        (data_min, data_max)
    };
    // suggestedMin/suggestedMax: データが優先、suggested はドメインを広げるだけ。
    // 非有限値（Infinity/NaN）は nice_ticks で無限 range を生じさせるため無視する。
    if let Some(s) = axis.suggested_min
        && s.is_finite()
        && s < domain_min
    {
        domain_min = s;
    }
    if let Some(s) = axis.suggested_max
        && s.is_finite()
        && s > domain_max
    {
        domain_max = s;
    }
    // ハード制約の y_axis.min / y_axis.max は現状 wire されていない（未実装）。
    // 実装する際は: hard min/max が suggested より優先、かつドメインを縮小できる点に注意。
    // 上限>下限を保証（縮退時の保険）。
    if domain_max <= domain_min {
        domain_max = domain_min + 1.0;
    }
    (domain_min, domain_max)
}

/// spec から y ドメイン(begin_at_zero尊重)・nice_ticks・y軸ラベル幅・プロット領域・凡例帯を計算。
pub fn compute(spec: &ChartSpec, m: &TextMeasurer) -> Frame {
    // y ドメイン。
    let (domain_min, domain_max) = value_domain(spec, &spec.y_axis);
    let ticks = nice_ticks(domain_min, domain_max, 10);

    // y 軸ラベル幅。
    let mut max_w = 0.0_f32;
    for &t in &ticks.ticks {
        let s = fmt_num(t);
        let w = m.width(&s, spec.theme.font_size as f32);
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
        legend_band_width_vertical(m, &series_names, spec.theme.font_size)
    } else {
        0.0
    };
    let legend_right = if legend && spec.legend == LegendPos::Right {
        legend_band_width_vertical(m, &series_names, spec.theme.font_size)
    } else {
        0.0
    };
    // line(edge-to-edge)では先頭/末尾の点が plot_left/plot_right に乗り、中央寄せの
    // x ラベルが点の外側へ半幅はみ出してキャンバス端でクリップされる。chart.js が
    // chartArea を edge ラベル半幅ぶん内側へ取るのと同様に edge 余白を確保する。
    // 末尾は常に内側化し、先頭は y 軸ラベル幅で足りなければ拡張する。
    // offset:true の line は bar 同様 band 中心配置でラベルがプロット内に収まるため、
    // 端余白は取らない(bar と同じ chartArea を使う)。
    let (edge_pad_left, edge_pad_right) =
        if matches!(spec.kind, ChartKind::Line) && spec.categories.len() > 1 && !spec.x_axis.offset
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

/// line/area の x 座標。chart.js の category スケール offset:false(edge-to-edge)に合わせ、
/// n 個のカテゴリを [plot_left, plot_right] へ i/(n-1) で等間隔配置する(先頭=左端・末尾=右端)。
/// bar の band 中心(category_center)とは異なる。n<=1 は (n-1)=0 で NaN になるため
/// プロット中央へフォールバックする(縮退ケース; 単一カテゴリの line fixture は存在しない)。
pub fn line_x(frame: &Frame, i: usize, n: usize) -> f64 {
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
        line_x(frame, i, n)
    }
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

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

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

    // 2. 横グリッド + y 軸ラベル。
    for &t in &frame.ticks.ticks {
        let y = frame.ys.map(t);
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: y,
            x2: frame.plot_right,
            y2: y,
            stroke: spec.theme.grid_color,
            stroke_width: 1.0,
        });
        items.push(Prim::Text {
            x: frame.plot_left - 6.0,
            y: y + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: fmt_num(t),
            rotate_deg: None,
        });
    }

    // 3. x ベースライン。
    items.push(Prim::Line {
        x1: frame.plot_left,
        y1: frame.plot_bottom,
        x2: frame.plot_right,
        y2: frame.plot_bottom,
        stroke: ink,
        stroke_width: 1.0,
    });

    // 4. x カテゴリラベル（auto-skip: ラベル幅 > スロット幅なら間引く）。
    let n = spec.categories.len().max(1);
    let slot_w = (frame.plot_right - frame.plot_left) / n as f64;
    // 代表ラベル幅（最初の非空ラベル）＋ 4px ギャップを使って step を決める。
    let step = spec
        .categories
        .iter()
        .find(|c| !c.is_empty())
        .map(|lbl| {
            let lw = m.width(lbl, label_font as f32) as f64 + 4.0;
            if slot_w > 0.0 && lw > slot_w {
                ((lw / slot_w).ceil() as usize).max(1)
            } else {
                1
            }
        })
        .unwrap_or(1);
    for (i, cat) in spec.categories.iter().enumerate() {
        if !cat.is_empty() && i % step == 0 {
            // line は点と同じ配置(offset:false=edge-to-edge / offset:true=band 中心)で
            // ラベルを点の真下に置く。bar/その他はバンド中心。mixed は bar を含むため band 中心のまま。
            let label_x = if matches!(spec.kind, ChartKind::Line) {
                line_category_x(spec, frame, i, n)
            } else {
                category_center(frame, i, n)
            };
            items.push(Prim::Text {
                x: label_x,
                y: frame.plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
                size: label_font,
                anchor: Anchor::Middle,
                fill: ink,
                content: cat.clone(),
                rotate_deg: None,
            });
        }
    }

    // 5. 凡例(Top/Bottom: 横並び)。
    if has_legend(spec) && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        // 各エントリ幅と合計（末尾間隔 16 を最後だけ除く）。
        let mut total = 0.0_f64;
        for (k, ser) in spec.series.iter().enumerate() {
            let ew = legend_entry_width(m, &ser.name, label_font);
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
                y: legend_cy + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Start,
                fill: ink,
                content: ser.name.clone(),
                rotate_deg: None,
            });
            let ew = legend_entry_width(m, &ser.name, label_font);
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
        let band_w = legend_band_width_vertical(m, &names, label_font);
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
            ink,
            label_font,
        );
    }
}

/// 縦置き凡例(Left/Right)の帯幅: swatch(12) + gap(4) + 最大ラベル幅 + パディング(16)。
/// 名前が空の系列も含めて算出する(レイアウトの確保量を呼び出し側と一致させるため)。
/// `font_size` はラベル測定に使う基準フォント(テーマ)。
pub fn legend_band_width_vertical(m: &TextMeasurer, names: &[String], font_size: f64) -> f64 {
    let mut max_w = 0.0_f64;
    for name in names {
        let w = m.width(name, font_size as f32) as f64;
        if w > max_w {
            max_w = w;
        }
    }
    12.0 + 4.0 + max_w + 16.0
}

/// 縦置き凡例(Left/Right)を描く。entries は (名前, swatch色) の解決済みペア。
/// プロットの縦スパン中央にエントリ群を配置する。
/// `ink`/`font_size` はラベルの色とフォント(テーマ)。
pub fn draw_vertical_legend(
    items: &mut Vec<Prim>,
    entries: &[(String, Color)],
    band_x: f64,
    plot_top: f64,
    plot_bottom: f64,
    ink: Color,
    font_size: f64,
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
            y: row_center + font_size * TEXT_BASELINE_RATIO,
            size: font_size,
            anchor: Anchor::Start,
            fill: ink,
            content: name.clone(),
            rotate_deg: None,
        });
    }
}

/// 凡例 1 エントリの占有幅: swatch幅(12) + gap(4) + ラベル幅 + trailing間隔(16)。
/// `font_size` はラベル測定に使う基準フォント(テーマ)。
pub fn legend_entry_width(m: &TextMeasurer, name: &str, font_size: f64) -> f64 {
    12.0 + 4.0 + m.width(name, font_size as f32) as f64 + 16.0
}

/// 値ラベルの Prim::Text を生成する(フォント=size、内容=fmt_num(v))。
/// 全チャート種でデータラベル生成を一元化する。x/y/anchor/fill/size は呼び出し側が決める。
pub fn value_label(x: f64, y: f64, size: f64, anchor: Anchor, fill: Color, v: f64) -> Prim {
    Prim::Text {
        x,
        y,
        size,
        anchor,
        fill,
        content: fmt_num(v),
        rotate_deg: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::ir::{AxisSpec, ChartKind, ChartSpec, LegendPos, Point, Series, SeriesType};
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
            series: vec![Series {
                name: String::new(),
                values: vec![1.0; n],
                points: Vec::<Point>::new(),
                fill: vec![palette[0]],
                stroke: vec![],
                stroke_width: 1.0,
                area: false,
                tension: 0.0,
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
                grid: true,
            },
            y_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: true,
                offset: false,
                grid: true,
            },
            legend: LegendPos::None,
            title: None,
            width,
            height: 400.0,
            data_labels: false,
            theme: crate::ir::Theme::default(),
            decimation: crate::ir::Decimation::default(),
            radial_axis: None,
        }
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
}
