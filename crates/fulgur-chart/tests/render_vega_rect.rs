use fulgur_chart::frontend::vegalite;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&vegalite::parse(json, false).unwrap())
}

#[test]
fn rect_quantitative_snapshot() {
    let svg = render(
        r#"{
            "mark": "rect",
            "data": {"values": [
                {"day":"Mon","hour":"AM","v":1},
                {"day":"Tue","hour":"AM","v":4},
                {"day":"Wed","hour":"AM","v":2},
                {"day":"Mon","hour":"PM","v":6},
                {"day":"Tue","hour":"PM","v":9},
                {"day":"Wed","hour":"PM","v":3},
                {"day":"Mon","hour":"EVE","v":2},
                {"day":"Tue","hour":"EVE","v":5},
                {"day":"Wed","hour":"EVE","v":7}
            ]},
            "encoding": {
                "x": {"field":"day","type":"nominal"},
                "y": {"field":"hour","type":"nominal"},
                "color": {"field":"v","type":"quantitative"}
            },
            "title": "Weekly Heatmap"
        }"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn rect_nominal_snapshot() {
    let svg = render(
        r#"{
            "mark": "rect",
            "data": {"values": [
                {"x":"A","y":"X","cat":"a"},
                {"x":"B","y":"X","cat":"b"},
                {"x":"C","y":"X","cat":"a"},
                {"x":"A","y":"Y","cat":"c"},
                {"x":"B","y":"Y","cat":"a"},
                {"x":"C","y":"Y","cat":"b"}
            ]},
            "encoding": {
                "x": {"field":"x","type":"nominal"},
                "y": {"field":"y","type":"nominal"},
                "color": {"field":"cat","type":"nominal"}
            }
        }"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn rect_missing_cells_snapshot() {
    // (B, Y) missing → 3 cells + 1 background rect.
    let svg = render(
        r#"{
            "mark": "rect",
            "data": {"values": [
                {"x":"A","y":"X","v":1},
                {"x":"B","y":"X","v":2},
                {"x":"A","y":"Y","v":3}
            ]},
            "encoding": {
                "x": {"field":"x","type":"nominal"},
                "y": {"field":"y","type":"nominal"},
                "color": {"field":"v","type":"quantitative"}
            }
        }"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn rect_deterministic() {
    let j = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},{"x":"B","y":"X","v":3},{"x":"A","y":"Y","v":5}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    assert_eq!(render(j), render(j));
}
