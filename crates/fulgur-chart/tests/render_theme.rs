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
#[test]
fn custom_palette_rgba_alpha_preserved_in_fill() {
    // rgba(255,0,0,0.3) をパレットとして設定 → fill-opacity="0.3" が SVG に現れる
    let json = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"theme":{"palette":["rgba(255,0,0,0.3)"]}}}"##;
    let svg = render(json);
    assert!(
        svg.contains(r#"fill-opacity="0.3""#),
        "custom palette alpha should be preserved, got:\n{svg}"
    );
}
#[test]
fn custom_palette_rgba_alpha_preserved_in_stroke() {
    // rgba(255,0,0,0.3) をパレットとして設定 → stroke-opacity="0.3" が SVG に現れる
    // line チャートは Prim::Polyline を使い stroke-opacity を出力する
    // (bar チャートは Prim::Rect のみで stroke フィールドを持たないため対象外)
    let json = r##"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
      "options":{"theme":{"palette":["rgba(255,0,0,0.3)"]}}}"##;
    let svg = render(json);
    assert!(
        svg.contains(r#"stroke-opacity="0.3""#),
        "custom palette alpha should be preserved in stroke, got:\n{svg}"
    );
}
#[test]
fn default_palette_fill_uses_default_alpha() {
    // パレット未指定 → bar の fill は 0.5 alpha
    let json = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#;
    let svg = render(json);
    assert!(
        svg.contains(r#"fill-opacity="0.5""#),
        "default palette should use 0.5 fill alpha, got:\n{svg}"
    );
}
#[test]
fn opaque_theme_background_marks_scene_opaque() {
    use fulgur_chart::layout::build_scene;
    use fulgur_chart::text::TextMeasurer;
    let m = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();

    let opaque = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
          "options":{"theme":{"backgroundColor":"#ff00ff"}}}"##,
        false,
    )
    .unwrap();
    assert!(build_scene(&opaque, &m).has_opaque_background());

    let semi = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
          "options":{"theme":{"backgroundColor":"rgba(255,0,255,0.5)"}}}"##,
        false,
    )
    .unwrap();
    assert!(!build_scene(&semi, &m).has_opaque_background());

    let none = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
        false,
    )
    .unwrap();
    assert!(!build_scene(&none, &m).has_opaque_background());
}
