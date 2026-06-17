use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn theme_palette_overrides_series_color() {
    let json = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"theme":{"palette":["#112233"]}}}"##;
    assert!(render(json).contains("#112233"));
}
#[test]
fn theme_grid_and_text_color() {
    let json = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"theme":{"gridColor":"#abcdef","textColor":"#fedcba"}}}"##;
    let svg = render(json);
    assert!(svg.contains("#abcdef"));
    assert!(svg.contains("#fedcba"));
}
#[test]
fn theme_background_rect_present_only_when_set() {
    let with = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"theme":{"backgroundColor":"#ff00ff"}}}"##;
    assert!(
        render(with).contains(r##"<rect x="0" y="0" width="800" height="450" fill="#ff00ff""##)
    );
    let without = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    assert!(!render(without).contains(r#"x="0" y="0" width="800" height="450""#));
}
#[test]
fn theme_font_size_override() {
    let json = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"theme":{"fontSize":20}}}"#;
    assert!(render(json).contains(r#"font-size="20""#));
}
#[test]
fn strict_rejects_unknown_theme_key() {
    let json = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"theme":{"foo":1}}}"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok());
}
#[test]
fn no_theme_is_deterministic() {
    let json = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    assert_eq!(render(json), render(json));
}
