//! IR → SVG の最上位エントリ。

use crate::font::{DEFAULT_FAMILY, DEFAULT_FONT, family_name};
use crate::text::TextMeasurer;

/// 既定フォント(Noto Sans JP)で描画。出力は従来と byte 一致。
pub fn render_chart(spec: &crate::ir::ChartSpec) -> String {
    let m = TextMeasurer::new(DEFAULT_FONT).expect("bundled font parses");
    render_with(spec, &m, "Noto Sans JP, sans-serif")
}

/// 任意フォントで描画。font_bytes がパース不能なら Err。
pub fn render_chart_with_font(
    spec: &crate::ir::ChartSpec,
    font_bytes: &[u8],
) -> Result<String, String> {
    let m = TextMeasurer::new(font_bytes).map_err(|e| format!("フォント読込失敗: {e}"))?;
    let fam = family_name(font_bytes).unwrap_or_else(|| DEFAULT_FAMILY.to_string());
    // family 名は CSS string としてクォートする。フォント name table はカンマや引用符を
    // 含み得るため、未クォートだと CSS が複数 family と解釈し計測/SVG/PNG の三者一致が崩れる。
    Ok(render_with(
        spec,
        &m,
        &format!("{}, sans-serif", css_quote_family(&fam)),
    ))
}

/// CSS font-family 値用に family 名を二重引用符で囲む。CSS 文字列規則に従い
/// `\` と `"` をエスケープし、カンマ等を含む名前でも 1 つの family として扱わせる。
fn css_quote_family(name: &str) -> String {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn render_with(spec: &crate::ir::ChartSpec, m: &TextMeasurer, font_family: &str) -> String {
    let scene = crate::layout::build_scene(spec, m);
    crate::svg::render_svg(&scene, font_family)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::chartjs;

    fn spec() -> crate::ir::ChartSpec {
        let json = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        chartjs::parse(json, false).unwrap()
    }

    #[test]
    fn with_default_font_is_ok_svg() {
        let out = render_chart_with_font(&spec(), DEFAULT_FONT).unwrap();
        assert!(out.starts_with("<svg"));
    }

    #[test]
    fn with_font_is_deterministic() {
        let a = render_chart_with_font(&spec(), DEFAULT_FONT).unwrap();
        let b = render_chart_with_font(&spec(), DEFAULT_FONT).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn with_invalid_font_is_err() {
        assert!(render_chart_with_font(&spec(), b"not a font").is_err());
    }

    #[test]
    fn css_quote_family_escapes_and_wraps() {
        assert_eq!(css_quote_family("IPAGothic"), "\"IPAGothic\"");
        // カンマ・引用符・バックスラッシュを安全に CSS 文字列化する。
        assert_eq!(css_quote_family(r#"A,"B\C"#), "\"A,\\\"B\\\\C\"");
    }

    #[test]
    fn custom_font_family_is_css_quoted_in_svg() {
        // 同梱フォント(family "Noto Sans JP")でもカスタム経路はクォートされる。
        let out = render_chart_with_font(&spec(), DEFAULT_FONT).unwrap();
        // SVG 属性では XML エスケープされ &quot; になる。
        assert!(
            out.contains("&quot;Noto Sans JP&quot;, sans-serif"),
            "{out}"
        );
    }

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
