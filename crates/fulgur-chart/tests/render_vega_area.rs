use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::ChartKind;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&vegalite::parse(json, true).unwrap())
}

const TEMPORAL_AREA_STACKED: &str = r#"{
    "mark": "area",
    "data": {"values": [
        {"t": "2024-01-01T00:00:00Z", "kind": "A", "v": 10},
        {"t": "2024-01-01T00:00:00Z", "kind": "B", "v": 4},
        {"t": "2024-01-02T00:00:00Z", "kind": "A", "v": 12},
        {"t": "2024-01-02T00:00:00Z", "kind": "B", "v": 6}
    ]},
    "encoding": {
        "x": {"field": "t", "type": "temporal"},
        "y": {"field": "v", "type": "quantitative"},
        "color": {"field": "kind", "type": "nominal"}
    }
}"#;

#[test]
fn temporal_area_with_color_is_stacked_and_renders() {
    let spec = vegalite::parse(TEMPORAL_AREA_STACKED, true).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: true }));
    assert!(spec.series.iter().all(|s| s.area));
    let svg = render(TEMPORAL_AREA_STACKED);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn categorical_stacked_area_snapshot() {
    let json = r#"{
        "mark": "area",
        "data": {"values": [
            {"month": "Jan", "kind": "A", "sales": 10},
            {"month": "Jan", "kind": "B", "sales": 5},
            {"month": "Feb", "kind": "A", "sales": 20},
            {"month": "Feb", "kind": "B", "sales": 15},
            {"month": "Mar", "kind": "A", "sales": 8},
            {"month": "Mar", "kind": "B", "sales": 12}
        ]},
        "encoding": {
            "x": {"field": "month", "type": "ordinal"},
            "y": {"field": "sales", "type": "quantitative"},
            "color": {"field": "kind", "type": "nominal"}
        }
    }"#;
    let svg = render(json);
    insta::assert_snapshot!(svg);
}
