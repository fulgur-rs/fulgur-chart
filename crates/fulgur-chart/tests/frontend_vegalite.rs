use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::ChartKind;

const BAR_SPEC: &str = r#"{
    "mark": "bar",
    "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5},{"cat":"C","val":2}]},
    "encoding": {"x": {"field":"cat","type":"nominal"}, "y": {"field":"val","type":"quantitative"}}
}"#;

#[test]
fn bar_categorical_single_series() {
    let spec = vegalite::parse(BAR_SPEC, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { .. }));
    assert_eq!(spec.categories, vec!["A", "B", "C"]);
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].values, vec![3.0, 5.0, 2.0]);
}

#[test]
fn color_split_creates_one_series_per_group() {
    let json = r#"{
        "mark": "bar",
        "data": {"values": [
            {"cat":"A","val":3,"g":"x"},
            {"cat":"B","val":5,"g":"y"},
            {"cat":"A","val":1,"g":"y"},
            {"cat":"C","val":2,"g":"x"}
        ]},
        "encoding": {
            "x": {"field":"cat","type":"nominal"},
            "y": {"field":"val","type":"quantitative"},
            "color": {"field":"g"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert_eq!(spec.categories, vec!["A", "B", "C"]);
    assert_eq!(spec.series.len(), 2);
    // 系列名は g の first-seen 順: x, y
    assert_eq!(spec.series[0].name, "x");
    assert_eq!(spec.series[1].name, "y");
    // x グループ: A=3, B=0(欠落), C=2
    assert_eq!(spec.series[0].values, vec![3.0, 0.0, 2.0]);
    // y グループ: A=1, B=5, C=0(欠落)
    assert_eq!(spec.series[1].values, vec![1.0, 5.0, 0.0]);
}

#[test]
fn line_mark_maps_to_line() {
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line));
}

#[test]
fn point_mark_maps_to_scatter_with_points() {
    let json = r#"{
        "mark": "point",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
    assert_eq!(spec.series.len(), 1);
    let pts = &spec.series[0].points;
    assert_eq!(pts.len(), 2);
    assert_eq!((pts[0].x, pts[0].y), (1.0, 2.0));
    assert_eq!((pts[1].x, pts[1].y), (3.0, 4.0));
}

#[test]
fn arc_mark_maps_to_pie_with_theta_sums() {
    let json = r#"{
        "mark": "arc",
        "data": {"values": [
            {"cat":"A","val":3},
            {"cat":"B","val":5},
            {"cat":"A","val":2}
        ]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Pie { .. }));
    assert_eq!(spec.categories, vec!["A", "B"]);
    assert_eq!(spec.series.len(), 1);
    // A = 3+2 = 5, B = 5
    assert_eq!(spec.series[0].values, vec![5.0, 5.0]);
}

#[test]
fn mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "bar"},
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { .. }));
}

#[test]
fn strict_accepts_known_keys() {
    assert!(vegalite::parse(BAR_SPEC, true).is_ok());
}

#[test]
fn strict_rejects_unknown_top_level_key() {
    let json = r#"{
        "mark": "bar",
        "wat": 1,
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn parse_is_deterministic() {
    let a = vegalite::parse(BAR_SPEC, false).unwrap();
    let b = vegalite::parse(BAR_SPEC, false).unwrap();
    assert_eq!(a, b);
}

#[test]
fn unknown_mark_errors() {
    let json = r#"{
        "mark": "wedge",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn url_data_errors() {
    let json = r#"{
        "mark": "bar",
        "data": {"url": "data.csv"},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn render_smoke_produces_svg() {
    let spec = vegalite::parse(BAR_SPEC, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
}

#[test]
fn category_field_non_scalar_errors() {
    // x の値が object → カテゴリ値にできず Err(空カテゴリへの統合を防ぐ)。
    let json = r#"{"mark":"bar","data":{"values":[{"cat":{"nested":1},"val":3}]},
        "encoding":{"x":{"field":"cat"},"y":{"field":"val"}}}"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn typo_missing_or_nonnumeric_field_errors() {
    // y.field の typo → 全 0 の誤チャートを防ぐため Err。
    let typo = r#"{"mark":"bar","data":{"values":[{"cat":"A","val":3}]},
        "encoding":{"x":{"field":"cat"},"y":{"field":"vall"}}}"#;
    assert!(vegalite::parse(typo, false).is_err());
    // y が文字列(非数値) → Err。
    let nonnum = r#"{"mark":"bar","data":{"values":[{"cat":"A","val":"x"}]},
        "encoding":{"x":{"field":"cat"},"y":{"field":"val"}}}"#;
    assert!(vegalite::parse(nonnum, false).is_err());
    // 必須 x.field 未指定 → Err。
    let missing = r#"{"mark":"bar","data":{"values":[{"cat":"A","val":3}]},
        "encoding":{"y":{"field":"val"}}}"#;
    assert!(vegalite::parse(missing, false).is_err());
}

#[test]
fn line_with_sparse_color_errors() {
    // 色分け line で (cat,color) が疎 → 0 埋めの誤った折れ線を防ぐため Err。
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3,"g":"x"},{"cat":"B","val":5,"g":"y"}]},
        "encoding": {"x":{"field":"cat"},"y":{"field":"val"},"color":{"field":"g"}}
    }"#;
    assert!(vegalite::parse(json, false).is_err());
}

#[test]
fn line_with_dense_color_ok() {
    // 全 (cat,color) が揃っていれば 2 系列で OK。
    let json = r#"{
        "mark": "line",
        "data": {"values": [
            {"cat":"A","val":3,"g":"x"},{"cat":"B","val":5,"g":"x"},
            {"cat":"A","val":2,"g":"y"},{"cat":"B","val":4,"g":"y"}
        ]},
        "encoding": {"x":{"field":"cat"},"y":{"field":"val"},"color":{"field":"g"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert_eq!(spec.series.len(), 2);
}

#[test]
fn honors_width_height_title() {
    // VL の width/height/title を ChartSpec に反映する(strict 許可と整合)。
    let json = r#"{
        "mark": "bar",
        "width": 400, "height": 300, "title": "売上",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert_eq!(spec.width, 400.0);
    assert_eq!(spec.height, 300.0);
    assert_eq!(spec.title.as_deref(), Some("売上"));
}

#[test]
fn strict_rejects_aggregate() {
    // aggregate は未実装。strict では未対応キーとして拒否する(誤った集計を黙認しない)。
    let json = r#"{
        "mark": "bar",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val","aggregate":"mean"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}
