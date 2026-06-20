//! チャート意味モデル: chart.js と数値照合するための、解決済み色・軸目盛り・
//! counts を持つシリアライズ可能な中間表現。描画はせず IR + layout から構築する。

use serde::Serialize;

use crate::ir::{ChartKind, ChartSpec, Color};
use crate::text::TextMeasurer;

/// 解決済み色を正規化 rgba 文字列にする(plan の正規化規約に従う)。
pub fn rgba_string(c: &Color) -> String {
    format!("rgba({},{},{},{})", c.r, c.g, c.b, fmt_alpha(c.a))
}

/// alpha を正規化整形する(>=1→"1", <=0→"0", それ以外は 3 桁丸め・末尾ゼロ除去)。
fn fmt_alpha(a: f32) -> String {
    if a >= 1.0 {
        return "1".to_string();
    }
    if a <= 0.0 {
        return "0".to_string();
    }
    let r = (a as f64 * 1000.0).round() / 1000.0;
    // f64 の Display は最短往復表現を出すため n/1000 に末尾ゼロは付かない。
    format!("{r}")
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ChartModel {
    pub meta: Meta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axes: Option<Axes>,
    pub series: Vec<SeriesModel>,
    pub counts: Counts,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Meta {
    pub r#type: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Axes {
    pub x: AxisModel,
    pub y: AxisModel,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AxisModel {
    pub kind: String, // "linear" | "category"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<Vec<f64>>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SeriesModel {
    pub label: String,
    pub fill: Vec<String>,
    pub stroke: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Counts {
    pub datasets: usize,
    /// ラベル非空のデータセット数。描画される凡例エントリ数ではない
    /// (pie/doughnut はスライスごとに 1 エントリを描画する)。
    pub legend_items: usize,
    pub x_ticks: usize,
    pub y_ticks: usize,
}

/// 描画要素数(scatter/bubble は points、その他は values)。
fn element_count(s: &crate::ir::Series) -> usize {
    if s.points.is_empty() {
        s.values.len()
    } else {
        s.points.len()
    }
}

/// 色ベクタを「要素ごと rgba」に展開しつつ、全要素同色なら長さ1へ畳む。
/// 色解決はレンダラと共有する `ir::color_at` を使い、モデルと描画の差異を防ぐ。
/// 要素数 0(空データセット)では描画マークが無いため空ベクタを返す
/// (chart.js 抽出器も `meta.data.length`=0 で空配列を返すため、これに揃える)。
fn colors_to_strings(colors: &[Color], n: usize) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let all: Vec<String> = (0..n)
        .map(|i| rgba_string(&crate::ir::color_at(colors, i)))
        .collect();
    if all.iter().all(|x| x == &all[0]) {
        vec![all[0].clone()]
    } else {
        all
    }
}

fn chart_type_name(kind: &ChartKind) -> &'static str {
    match kind {
        ChartKind::Bar {
            horizontal: true, ..
        } => "bar-horizontal",
        ChartKind::Bar { .. } => "bar",
        ChartKind::Line => "line",
        ChartKind::Pie { donut_ratio } if *donut_ratio > 0.0 => "doughnut",
        ChartKind::Pie { .. } => "pie",
        ChartKind::Scatter => "scatter",
        ChartKind::Bubble => "bubble",
        ChartKind::Radar => "radar",
        ChartKind::Mixed => "mixed",
        ChartKind::Matrix { .. } => "matrix",
        ChartKind::Progress => "progress",
    }
}

/// 軸抜き(meta/series/counts のみ)のコアモデル。Task 3 で軸を載せる。
pub fn build_model_core(spec: &ChartSpec) -> ChartModel {
    // pie/doughnut のスライス境界は renderer が白(pie::SLICE_STROKE)で固定描画し、
    // 解析済み borderColor を使わない。モデルも実描画に合わせて白を主張する
    // (spec が borderColor を指定しても fulgur はそれを無視して白を描く点を、
    // chart.js との diff で正しく顕在化させるため)。
    let is_pie = matches!(spec.kind, ChartKind::Pie { .. });
    let series: Vec<SeriesModel> = spec
        .series
        .iter()
        .map(|s| {
            let n = element_count(s);
            let stroke = if is_pie {
                colors_to_strings(&[crate::layout::pie::SLICE_STROKE], n)
            } else {
                colors_to_strings(&s.stroke, n)
            };
            SeriesModel {
                label: s.name.clone(),
                fill: colors_to_strings(&s.fill, n),
                stroke,
                values: s.values.clone(),
            }
        })
        .collect();
    let legend_items = spec.series.iter().filter(|s| !s.name.is_empty()).count();
    ChartModel {
        meta: Meta {
            r#type: chart_type_name(&spec.kind).to_string(),
            width: spec.width,
            height: spec.height,
        },
        axes: None,
        series,
        counts: Counts {
            datasets: spec.series.len(),
            legend_items,
            x_ticks: spec.categories.len(),
            y_ticks: 0,
        },
    }
}

/// NiceTicks を線形軸モデルへ変換する。
fn linear_axis(t: &crate::scale::NiceTicks) -> AxisModel {
    AxisModel {
        kind: "linear".to_string(),
        labels: None,
        min: Some(t.min),
        max: Some(t.max),
        step: Some(t.step),
        ticks: Some(t.ticks.clone()),
    }
}

/// カテゴリ軸モデル(ラベルのみ)。
fn category_axis(labels: &[String]) -> AxisModel {
    AxisModel {
        kind: "category".to_string(),
        labels: Some(labels.to_vec()),
        min: None,
        max: None,
        step: None,
        ticks: None,
    }
}

/// 直交チャートの (x 軸, y 軸, y 目盛り数) を計算する。値(線形)軸は描画上の向きに
/// 関わらず常に `y` に載せ、カテゴリ軸を `x` に載せる — JS 抽出器の正規化規約
/// (線形値軸→y・カテゴリ→x)と揃え、apples-to-apples 照合を可能にするため。
/// 値域・nice_ticks は renderer の各 layout と同じ関数を共有し、描画との乖離を防ぐ。
/// 軸を持たないチャート(pie/radar/matrix/progress)は None を返す。
fn compute_axes(spec: &ChartSpec, m: &TextMeasurer) -> Option<(AxisModel, AxisModel, usize)> {
    use crate::scale::nice_ticks;
    match &spec.kind {
        // 縦棒・線・mixed: 値軸=y(layout::common::compute と共有)、カテゴリ=x。
        ChartKind::Bar {
            horizontal: false, ..
        }
        | ChartKind::Line
        | ChartKind::Mixed => {
            let t = crate::layout::common::compute(spec, m).ticks;
            Some((
                category_axis(&spec.categories),
                linear_axis(&t),
                t.ticks.len(),
            ))
        }
        // 横棒: 値軸は描画上 x だが照合のため y に載せる。値域は build_horizontal と
        // 同じく x_axis から読む。カテゴリ=x。
        ChartKind::Bar {
            horizontal: true, ..
        } => {
            let (lo, hi) = crate::layout::common::value_domain(spec, &spec.x_axis);
            let t = nice_ticks(lo, hi, 10);
            Some((
                category_axis(&spec.categories),
                linear_axis(&t),
                t.ticks.len(),
            ))
        }
        // scatter/bubble: x・y とも線形。renderer (scatter::build) と同じ axis_domain を共有。
        ChartKind::Scatter | ChartKind::Bubble => {
            let (xlo, xhi) = crate::layout::scatter::axis_domain(spec, &spec.x_axis, |p| p.x);
            let (ylo, yhi) = crate::layout::scatter::axis_domain(spec, &spec.y_axis, |p| p.y);
            let xt = nice_ticks(xlo, xhi, 10);
            let yt = nice_ticks(ylo, yhi, 10);
            Some((linear_axis(&xt), linear_axis(&yt), yt.ticks.len()))
        }
        _ => None,
    }
}

/// IR + layout から完全な意味モデルを構築する。直交チャート(縦棒・横棒・線・
/// mixed・scatter・bubble)に軸を載せる。
pub fn build_model(spec: &ChartSpec, m: &TextMeasurer) -> ChartModel {
    let mut model = build_model_core(spec);
    if let Some((x, y, y_ticks)) = compute_axes(spec, m) {
        model.counts.y_ticks = y_ticks;
        model.axes = Some(Axes { x, y });
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::ir::Color;
    use crate::text::TextMeasurer;

    #[test]
    fn rgba_opaque_uses_1() {
        let c = Color {
            r: 54,
            g: 162,
            b: 235,
            a: 1.0,
        };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,1)");
    }

    #[test]
    fn rgba_half_alpha() {
        let c = Color {
            r: 54,
            g: 162,
            b: 235,
            a: 0.5,
        };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,0.5)");
    }

    #[test]
    fn rgba_transparent_uses_0() {
        let c = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        };
        assert_eq!(rgba_string(&c), "rgba(0,0,0,0)");
    }

    #[test]
    fn rgba_trims_trailing_zeros() {
        let c = Color {
            r: 1,
            g: 2,
            b: 3,
            a: 0.25,
        };
        assert_eq!(rgba_string(&c), "rgba(1,2,3,0.25)");
    }

    #[test]
    fn builds_meta_series_counts_for_bar() {
        let json = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],
          "datasets":[{"label":"売上","data":[120,200,150]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.meta.r#type, "bar");
        assert_eq!(model.series.len(), 1);
        assert_eq!(model.series[0].label, "売上");
        // 既定パレット先頭 #36A2EB、fill alpha=0.5 / stroke alpha=1.0(chart.js v4)
        assert_eq!(
            model.series[0].fill,
            vec!["rgba(54,162,235,0.5)".to_string()]
        );
        assert_eq!(
            model.series[0].stroke,
            vec!["rgba(54,162,235,1)".to_string()]
        );
        assert_eq!(model.series[0].values, vec![120.0, 200.0, 150.0]);
        assert_eq!(model.counts.datasets, 1);
        assert_eq!(model.counts.x_ticks, 3);
    }

    #[test]
    fn pie_emits_per_slice_fill() {
        let json = r##"{"type":"pie","data":{"labels":["a","b","c"],
          "datasets":[{"data":[1,2,3],
          "backgroundColor":["#ff0000","#00ff00","#0000ff"]}]}}"##;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.series[0].fill.len(), 3);
        assert_eq!(model.series[0].fill[0], "rgba(255,0,0,1)");
    }

    #[test]
    fn bar_has_linear_y_and_category_x() {
        let json = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],
          "datasets":[{"data":[0,100,50]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("bar には軸があるべき");
        assert_eq!(axes.y.kind, "linear");
        assert_eq!(axes.y.min, Some(0.0));
        assert_eq!(axes.x.kind, "category");
        assert_eq!(
            axes.x.labels.as_deref(),
            Some(&["1月".to_string(), "2月".to_string(), "3月".to_string()][..])
        );
        // y_ticks は目盛り数に同期
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn pie_has_no_axes() {
        let json = r#"{"type":"pie","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.axes.is_none());
    }

    #[test]
    fn horizontal_bar_puts_value_axis_on_y() {
        // 横棒でも値(線形)軸は y に、カテゴリは x に載る(JS 抽出器の規約に揃える)。
        let json = r#"{"type":"bar","data":{"labels":["a","b"],
          "datasets":[{"data":[10,90]}]},"options":{"indexAxis":"y"}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("横棒には軸があるべき");
        assert_eq!(axes.y.kind, "linear");
        assert_eq!(axes.y.min, Some(0.0));
        assert_eq!(axes.x.kind, "category");
        assert!(model.counts.y_ticks > 0);
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn scatter_has_linear_x_and_y_axes() {
        let json = r#"{"type":"scatter","data":{"datasets":[{"data":[
          {"x":1,"y":2},{"x":3,"y":8}]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("scatter には軸があるべき");
        assert_eq!(axes.x.kind, "linear");
        assert_eq!(axes.y.kind, "linear");
        assert!(model.counts.y_ticks > 0);
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn pie_stroke_claims_rendered_white() {
        // renderer は borderColor を無視し白でスライス境界を描くので、モデルも白を主張する。
        let json = r##"{"type":"pie","data":{"labels":["a","b"],
          "datasets":[{"data":[1,2],"borderColor":"#ff0000"}]}}"##;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(
            model.series[0].stroke,
            vec!["rgba(255,255,255,1)".to_string()]
        );
    }

    #[test]
    fn empty_dataset_emits_no_element_colors() {
        // 空データセットは描画マークが無いため fill/stroke とも空(chart.js と一致)。
        let json = r#"{"type":"bar","data":{"labels":[],"datasets":[{"data":[]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert!(model.series[0].fill.is_empty());
        assert!(model.series[0].stroke.is_empty());
    }

    /// クロス言語フィクスチャ: ここの行は
    /// `tools/chartjs-compat/rgba-fixture.json` と同一でなければならない。
    /// Rust `rgba_string` と JS `fmtAlpha` の乖離をどちらか一方のテストで必ず捕捉する。
    #[test]
    fn rgba_string_matches_cross_language_fixture() {
        let rows: &[(u8, u8, u8, f32, &str)] = &[
            (0, 0, 0, 0.0, "rgba(0,0,0,0)"),
            (1, 2, 3, 1.0, "rgba(1,2,3,1)"),
            (54, 162, 235, 0.5, "rgba(54,162,235,0.5)"),
            (255, 99, 132, 0.25, "rgba(255,99,132,0.25)"),
            (10, 20, 30, 0.333, "rgba(10,20,30,0.333)"),
            (10, 20, 30, 0.3333333, "rgba(10,20,30,0.333)"),
            (10, 20, 30, 0.1, "rgba(10,20,30,0.1)"),
            (10, 20, 30, 0.999, "rgba(10,20,30,0.999)"),
            (10, 20, 30, 0.9999, "rgba(10,20,30,1)"),
            (10, 20, 30, 0.0004, "rgba(10,20,30,0)"),
        ];
        for &(r, g, b, a, expected) in rows {
            let c = Color { r, g, b, a };
            assert_eq!(rgba_string(&c), expected, "row r={r} g={g} b={b} a={a}");
        }
    }
}
