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
    Ok(render_with(spec, &m, &format!("{fam}, sans-serif")))
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
}
