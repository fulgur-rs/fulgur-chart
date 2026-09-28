//! IR → SVG の最上位エントリ。

#[cfg(feature = "default-font")]
use crate::font::DEFAULT_FONT;
use crate::font::{DEFAULT_FAMILY, family_name};
use crate::text::TextMeasurer;

/// 既定フォント(Noto Sans JP)で描画する legacy の low-level 経路。
///
/// `ChartSpec` が不正で layout が失敗すると panic する。ユーザー入力には
/// [`render_chart_with_limits`] を使い、全入力 policy の検証には先に
/// [`crate::guard::validate_spec`] を呼ぶ。
#[cfg(feature = "default-font")]
pub fn render_chart(spec: &crate::ir::ChartSpec) -> String {
    let m = TextMeasurer::new(DEFAULT_FONT).expect("bundled font parses");
    render_with(
        spec,
        &m,
        "Noto Sans JP, sans-serif",
        &crate::guard::InputLimits::default(),
    )
    .expect("chart rendering failed")
}

/// 既定フォントで SVG を描画し、projection/layout のエラーを呼び出し元へ返す。
/// marker 半径と PlotArea 外周の scene 検査も描画前に行う。その他の入力 policy は
/// [`crate::guard::validate_spec`] で検証する。
#[cfg(feature = "default-font")]
pub fn render_chart_with_limits(
    spec: &crate::ir::ChartSpec,
    limits: &crate::guard::InputLimits,
) -> Result<String, String> {
    let m = TextMeasurer::new(DEFAULT_FONT).map_err(|e| format!("フォント読込失敗: {e}"))?;
    crate::guard::validate_marker_radii(spec)?;
    crate::guard::validate_plot_area_scene_with_measurer(spec, limits, &m)?;
    render_with(spec, &m, "Noto Sans JP, sans-serif", limits)
}

/// 任意フォントで描画。font_bytes がパース不能なら Err。
pub fn render_chart_with_font(
    spec: &crate::ir::ChartSpec,
    font_bytes: &[u8],
) -> Result<String, String> {
    render_chart_with_font_and_limits(spec, font_bytes, &crate::guard::InputLimits::default())
}

/// 任意フォントと入力上限で描画。font_bytes がパース不能なら Err。
///
/// 描画 backend 共通のマーカー半径安全性検証と、`limits` を使った
/// カスタムフォント計測による PlotArea 外周 scene 検証を描画前に行う。
/// その他の入力 policy は従来どおり検証しないため、必要なら呼び出し側で
/// [`crate::guard::validate_spec`] または [`crate::guard::validate_spec_with_measurer`]
/// を使う。PNG/WebP 出力では、これとは別に固定のピクセル面積 hard stop
/// （WebP は軸ごとの hard stop も）が常に適用され、`limits` では緩和できない。
pub fn render_chart_with_font_and_limits(
    spec: &crate::ir::ChartSpec,
    font_bytes: &[u8],
    limits: &crate::guard::InputLimits,
) -> Result<String, String> {
    let m = TextMeasurer::new(font_bytes).map_err(|e| format!("フォント読込失敗: {e}"))?;
    crate::guard::validate_marker_radii(spec)?;
    crate::guard::validate_plot_area_scene_with_measurer(spec, limits, &m)?;
    let fam = family_name(font_bytes).unwrap_or_else(|| DEFAULT_FAMILY.to_string());
    // family 名は CSS string としてクォートする。フォント name table はカンマや引用符を
    // 含み得るため、未クォートだと CSS が複数 family と解釈し計測/SVG/PNG の三者一致が崩れる。
    render_with(
        spec,
        &m,
        &format!("{}, sans-serif", css_quote_family(&fam)),
        limits,
    )
}

/// CSS font-family 値用に family 名を二重引用符で囲む。CSS 文字列規則に従い
/// `\` と `"` をエスケープし、カンマ等を含む名前でも 1 つの family として扱わせる。
fn css_quote_family(name: &str) -> String {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn render_with(
    spec: &crate::ir::ChartSpec,
    m: &TextMeasurer,
    font_family: &str,
    limits: &crate::guard::InputLimits,
) -> Result<String, String> {
    let scene = crate::layout::build_scene_checked_with_limits(spec, m, limits)?;
    Ok(crate::svg::render_svg(&scene, font_family))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TEST_FONT;
    use crate::frontend::chartjs;

    fn spec() -> crate::ir::ChartSpec {
        let json = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        chartjs::parse(json, false).unwrap()
    }

    #[test]
    fn with_explicit_font_is_ok_svg() {
        let out = render_chart_with_font(&spec(), TEST_FONT).unwrap();
        assert!(out.starts_with("<svg"));
    }

    #[test]
    fn with_font_is_deterministic() {
        let a = render_chart_with_font(&spec(), TEST_FONT).unwrap();
        let b = render_chart_with_font(&spec(), TEST_FONT).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn with_invalid_font_is_err() {
        assert!(render_chart_with_font(&spec(), b"not a font").is_err());
    }

    #[test]
    fn custom_font_render_preserves_legacy_unrelated_base_policy_contract() {
        let mut invalid_by_base_policy = spec();
        invalid_by_base_policy.width = 0.0;
        let svg = render_chart_with_font(&invalid_by_base_policy, TEST_FONT).unwrap();
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn css_quote_family_escapes_and_wraps() {
        assert_eq!(css_quote_family("IPAGothic"), "\"IPAGothic\"");
        // カンマ・引用符・バックスラッシュを安全に CSS 文字列化する。
        assert_eq!(css_quote_family(r#"A,"B\C"#), "\"A,\\\"B\\\\C\"");
    }

    #[test]
    fn fallible_svg_render_returns_geoshape_projection_errors() {
        let mut spec = spec();
        spec.kind = crate::ir::ChartKind::GeoShape {
            data: Box::new(crate::ir::GeoShape {
                features: vec![crate::ir::GeoFeature {
                    geometry: Some(crate::ir::GeoGeometry::Point([2.0, 0.0])),
                    fill: None,
                }],
                projection: crate::ir::GeoProjection {
                    projection_type: crate::ir::GeoProjectionType::Identity,
                    scale: Some(f64::MAX),
                    ..crate::ir::GeoProjection::default()
                },
                style: crate::ir::GeoShapeStyle::default(),
            }),
        };
        let error = render_chart_with_font(&spec, TEST_FONT)
            .expect_err("projection failures must reach the fallible render API");
        assert!(error.contains("non-finite"), "{error}");
    }

    #[cfg(feature = "default-font")]
    #[test]
    fn default_font_fallible_svg_render_returns_geoshape_projection_errors() {
        let mut spec = spec();
        spec.kind = crate::ir::ChartKind::GeoShape {
            data: Box::new(crate::ir::GeoShape {
                features: vec![crate::ir::GeoFeature {
                    geometry: Some(crate::ir::GeoGeometry::Point([2.0, 0.0])),
                    fill: None,
                }],
                projection: crate::ir::GeoProjection {
                    projection_type: crate::ir::GeoProjectionType::Identity,
                    scale: Some(f64::MAX),
                    ..crate::ir::GeoProjection::default()
                },
                style: crate::ir::GeoShapeStyle::default(),
            }),
        };
        let error = render_chart_with_limits(&spec, &crate::guard::InputLimits::default())
            .expect_err("default-font render failures must be returned to the caller");
        assert!(error.contains("non-finite"), "{error}");
    }

    #[test]
    fn custom_font_family_is_css_quoted_in_svg() {
        // 同梱フォント(family "Noto Sans JP")でもカスタム経路はクォートされる。
        let out = render_chart_with_font(&spec(), TEST_FONT).unwrap();
        // SVG 属性では XML エスケープされ &quot; になる。
        assert!(
            out.contains("&quot;Noto Sans JP&quot;, sans-serif"),
            "{out}"
        );
    }

    #[cfg(feature = "default-font")]
    #[test]
    fn boxplot_renders_to_svg() {
        let json = r#"{
            "type": "boxplot",
            "data": {
                "labels": ["Mon", "Tue", "Wed"],
                "datasets": [{
                    "label": "Temperature",
                    "backgroundColor": "rgba(54, 162, 235, 0.5)",
                    "borderColor": "rgb(54, 162, 235)",
                    "data": [
                        [10, 25, 50, 75, 90],
                        [5, 20, 45, 70, 95],
                        [15, 30, 55, 80, 100]
                    ]
                }]
            }
        }"#;
        let spec = chartjs::parse(json, false).expect("parse error");
        let svg = render_chart(&spec);
        assert!(svg.starts_with("<svg"), "should produce valid SVG");
        assert!(
            svg.contains("rect"),
            "SVG should contain rect elements for boxes"
        );
        assert!(
            svg.contains("line"),
            "SVG should contain line elements for whiskers"
        );
    }
}
