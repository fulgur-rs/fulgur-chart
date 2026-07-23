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
    // X 軸タイトルがあれば、x カテゴリラベル帯の下側にさらにタイトル帯を確保して plot_bottom を上へ押し上げる。
    let x_title_h = if spec.x_axis.title.is_some() {
        AXIS_TITLE_BAND
    } else {
        0.0
    };
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;

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
            content: fmt_num(t),
            rotate_deg: None,
        });
    }

    // 3. x ベースライン。border.display / border.color / border.width / border.dash を反映。
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
    const TICK_LEN: f64 = 4.0;
    let ticks_cfg = &spec.y_axis.grid;
    if ticks_cfg.draw_ticks {
        let tick_color = ticks_cfg.color.unwrap_or(ink);
        for &t in &frame.ticks.ticks {
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
    use crate::ir::{
        AxisBorder, AxisGrid, AxisSpec, AxisTitle, AxisTitleAlign, ChartKind, ChartSpec, LegendPos,
        Point, Series, SeriesType,
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
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
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
            },
            legend: LegendPos::None,
            title: None,
            width,
            height: 400.0,
            data_labels: false,
            theme: crate::ir::Theme::default(),
            decimation: crate::ir::Decimation::default(),
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
