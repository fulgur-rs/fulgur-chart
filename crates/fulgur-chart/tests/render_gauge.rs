use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn radial_gauge_renders_svg() {
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[70]}]}}"#);
    assert!(
        svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
        "{svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}

#[test]
fn gauge_renders_svg() {
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["green","yellow","red"]}]}}"#,
    );
    assert!(
        svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
        "{svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

#[test]
fn radial_gauge_has_track_and_value_arc() {
    // トラックリング(全周) + 値弧 = path 2 以上。色も両方出る。
    let svg = render(
        r##"{"type":"radialGauge","data":{"datasets":[{"data":[70],"backgroundColor":"#ff0000"}]}}"##,
    );
    assert!(count(&svg, "<path") >= 2, "track + value arc: {svg}");
    assert!(
        svg.contains("#ff0000") || svg.contains("rgb"),
        "value color: {svg}"
    );
}

#[test]
fn radial_gauge_zero_value_track_only() {
    // value=min(0) → 値弧 sweep 0、トラックのみ。NaN/inf なし。
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[0]}]}}"#);
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
    assert_eq!(count(&svg, "<path"), 2, "track only = 2 paths: {svg}");
}

#[test]
fn radial_gauge_clamps_over_domain() {
    // domain 既定 [0,100]、value=150 → クランプして panic/NaN なし。
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[150]}]}}"#);
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
    assert_eq!(
        count(&svg, "<path"),
        4,
        "clamp to full circle = 2 track + 2 value arc paths: {svg}"
    );
}
