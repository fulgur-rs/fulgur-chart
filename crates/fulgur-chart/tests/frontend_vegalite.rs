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

#[test]
fn circle_mark_maps_to_scatter_with_points() {
    let json = r#"{
        "mark": "circle",
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
fn circle_mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "circle"},
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
}

#[test]
fn circle_mark_renders_svg() {
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
    // scatter renderer emits one <circle> per point.
    assert_eq!(svg.matches("<circle ").count(), 2);
}

#[test]
fn strict_circle_rejects_shape_encoding() {
    // 構造的 shape 非対応の invariant を strict パーサで pin する。
    // 現状 check_unknown_keys の encoding allow-list は shape を含まない
    // ため強制されるが、allow-list ドリフトで壊れないようテストで固定する。
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}, "shape": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_bar_rejects_theta_encoding() {
    let json = r#"{
        "mark": "bar",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    // strict では VlBarEncoding が受理しない theta を拒否する。
    assert!(vegalite::parse(json, true).is_err());
    // 非 strict では現状通り黙って許容(挙動維持)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_line_rejects_theta_encoding() {
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_point_rejects_theta_encoding() {
    let json = r#"{
        "mark": "point",
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_arc_accepts_x_encoding() {
    // arc の allow-list は [theta, color, x, y] を含むので strict でも OK。
    let json = r#"{
        "mark": "arc",
        "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5}]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}, "x": {"field":"cat"}}
    }"#;
    assert!(vegalite::parse(json, true).is_ok());
}

#[test]
fn strict_arc_rejects_unknown_encoding_channel() {
    // arc の allow-list に含まれない channel(size)は strict で拒否される。
    // arc 側からも invariant を pin して、将来の allow-list ドリフトを検出する。
    let json = r#"{
        "mark": "arc",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}, "size": {"field":"val"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    // 非 strict では現状通り黙って許容(挙動維持)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_object_form_mark_dispatches_allow_list() {
    // object 形の mark(`{"type": "bar"}`)からも mark 名を読めることを pin する。
    // read_mark_name の object 分岐が strict 経路で使われる保証。
    let json = r#"{
        "mark": {"type": "bar"},
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_unknown_mark_falls_through_to_parse_error() {
    // 未対応 mark は check_unknown_keys で早期 Err にせず、後段の
    // parse_mark へフォールスルーする。エラー文言も encoding.* ではなく
    // mark 名についてのものであることを確認して invariant を pin する。
    let json = r#"{
        "mark": "unknownX",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    let err = vegalite::parse(json, true).unwrap_err();
    assert!(
        err.contains("mark") && !err.contains("encoding."),
        "expected mark-name error, got: {err}"
    );
}

#[test]
fn rect_ir_variant_exists() {
    use fulgur_chart::ir::{ChartKind, Color};
    let kind = ChartKind::VegaRect {
        x_labels: vec!["A".to_string(), "B".to_string()],
        y_labels: vec!["X".to_string()],
        cells: vec![vec![
            Some(Color {
                r: 10,
                g: 20,
                b: 30,
                a: 1.0,
            }),
            None,
        ]],
    };
    assert!(matches!(kind, ChartKind::VegaRect { .. }));
}

#[test]
fn rect_mark_quantitative_maps_to_vegarect() {
    // 2x2 grid, quantitative color.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"day":"Mon","hour":"AM","v":1},
            {"day":"Tue","hour":"AM","v":3},
            {"day":"Mon","hour":"PM","v":5},
            {"day":"Tue","hour":"PM","v":7}
        ]},
        "encoding": {
            "x": {"field":"day","type":"nominal"},
            "y": {"field":"hour","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let (x_labels, y_labels, cells) = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } => (x_labels.clone(), y_labels.clone(), cells.clone()),
        _ => panic!("expected VegaRect, got {:?}", spec.kind),
    };
    // first-seen order
    assert_eq!(x_labels, vec!["Mon", "Tue"]);
    assert_eq!(y_labels, vec!["AM", "PM"]);
    // 2 rows x 2 cols
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].len(), 2);
    // min (v=1) at (Mon, AM) → color_lo (#ffffff white)
    let c00 = cells[0][0].expect("cell should not be None");
    assert_eq!(
        (c00.r, c00.g, c00.b),
        (255, 255, 255),
        "min cell should be white"
    );
    // max (v=7) at (Tue, PM) → color_hi (VL theme palette[0] = Tableau steel-blue #4c78a8 = (76, 120, 168))
    let c11 = cells[1][1].expect("cell should not be None");
    assert_eq!(
        (c11.r, c11.g, c11.b),
        (76, 120, 168),
        "max cell should be Tableau steel-blue"
    );
}

#[test]
fn rect_mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "rect"},
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        fulgur_chart::ir::ChartKind::VegaRect { .. }
    ));
}

#[test]
fn rect_mark_nominal_color_uses_palette_roundrobin() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","c":"cat0"},
            {"x":"B","y":"X","c":"cat1"},
            {"x":"A","y":"Y","c":"cat0"}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"c","type":"nominal"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // Vega-Lite Tableau10 first color: #4c78a8 (76, 120, 168)
    // Vega-Lite Tableau10 second color: #f58518 (245, 133, 24)
    let cat0_color = cells[0][0].expect("cell (A,X) present"); // cat0 → palette[0]
    let cat1_color = cells[0][1].expect("cell (B,X) present"); // cat1 → palette[1]
    let cat0_color_again = cells[1][0].expect("cell (A,Y) present"); // cat0 → palette[0]
    assert_eq!((cat0_color.r, cat0_color.g, cat0_color.b), (76, 120, 168));
    assert_eq!((cat1_color.r, cat1_color.g, cat1_color.b), (245, 133, 24));
    assert_eq!(cat0_color, cat0_color_again, "same category → same color");
    // (B, Y) は未出現 → None
    assert!(cells[1][1].is_none(), "missing (B,Y) should be None");
}

#[test]
fn rect_mark_rejects_null_color_value() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":null}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"c"}}
    }"#;
    let err = vegalite::parse(json, false).unwrap_err();
    assert!(
        err.contains("見つかりません") || err.contains("null"),
        "got: {err}"
    );
}

#[test]
fn rect_mark_aggregate_mean() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":2},
            {"x":"A","y":"X","v":4},
            {"x":"B","y":"X","v":10}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A, X) mean = (2 + 4) / 2 = 3.0 → min
    // (B, X) mean = 10 → max
    // range = 10 - 3 = 7, (A,X) t = 0.0 → white
    let ax = cells[0][0].expect("cell (A,X)");
    assert_eq!((ax.r, ax.g, ax.b), (255, 255, 255), "mean=3 → min → white");
    // (B, X) is at column index 1, row 0
    let bx = cells[0][1].expect("cell (B,X)");
    assert_eq!(
        (bx.r, bx.g, bx.b),
        (76, 120, 168),
        "mean=10 → max → Tableau blue"
    );
}

#[test]
fn rect_mark_aggregate_sum() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":2},
            {"x":"A","y":"X","v":4},
            {"x":"B","y":"X","v":10}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"sum"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A, X) sum = 6, (B, X) sum = 10, range = 4
    // (A,X) t=0 → white, (B,X) t=1 → blue
    let ax = cells[0][0].expect("cell (A,X)");
    assert_eq!((ax.r, ax.g, ax.b), (255, 255, 255));
    let bx = cells[0][1].expect("cell (B,X)");
    assert_eq!((bx.r, bx.g, bx.b), (76, 120, 168));
}

#[test]
fn rect_mark_aggregate_mean_vs_sum_are_distinguishable() {
    // 3 x-buckets so the aggregated (A,X) can land at intermediate positions.
    // Data:
    //   (A,X): [10, 10] — mean=10, sum=20
    //   (B,X): [15]      — mean=15, sum=15
    //   (C,X): [5]       — mean=5,  sum=5
    // Mean: range [5, 15], (A,X)=10 → t=0.5 → intermediate color
    // Sum:  range [5, 20], (A,X)=20 → t=1.0 → RECT_COLOR_HI (blue)
    let json_mean = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":10},
            {"x":"A","y":"X","v":10},
            {"x":"B","y":"X","v":15},
            {"x":"C","y":"X","v":5}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let json_sum = json_mean.replace(r#""aggregate":"mean""#, r#""aggregate":"sum""#);

    let mean_cells = match &vegalite::parse(json_mean, false).unwrap().kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    let sum_cells = match &vegalite::parse(&json_sum, false).unwrap().kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };

    // (A,X) is at row 0, col 0.
    let mean_ax = mean_cells[0][0].expect("mean (A,X) present");
    let sum_ax = sum_cells[0][0].expect("sum (A,X) present");
    // Mean: (A,X) is intermediate (not blue).
    assert_ne!(
        (mean_ax.r, mean_ax.g, mean_ax.b),
        (76, 120, 168),
        "mean (A,X) should not be at max endpoint"
    );
    // Sum: (A,X) becomes the max (t=1.0) → Tableau blue.
    assert_eq!(
        (sum_ax.r, sum_ax.g, sum_ax.b),
        (76, 120, 168),
        "sum (A,X) should be at max endpoint"
    );
    // And they must differ.
    assert_ne!(
        mean_ax, sum_ax,
        "mean and sum should produce different colors here"
    );
}

#[test]
fn rect_mark_aggregate_none_preserves_last_finite_numeric() {
    // Explicit quantitative + no aggregate + a bool follows a number at the same cell.
    // With bucket-based None, the finite numeric value survives (not clobbered by the non-numeric).
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":7},
            {"x":"A","y":"X","v":true},
            {"x":"B","y":"X","v":1}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A,X) uses v=7 (last finite numeric); (B,X) uses v=1.
    // range [1, 7], (A,X)=7 → max → blue; (B,X)=1 → min → white.
    let ax = cells[0][0].expect("(A,X) should be Some");
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
    let bx = cells[0][1].expect("(B,X) should be Some");
    assert_eq!((bx.r, bx.g, bx.b), (255, 255, 255));
}

#[test]
fn strict_rect_rejects_size_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "size": {"field":"v"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "size should be rejected in strict"
    );
    assert!(
        vegalite::parse(json, false).is_ok(),
        "size should be tolerated in non-strict"
    );
}

#[test]
fn strict_rect_rejects_tooltip_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "tooltip": {"field":"v"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_x2_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "x2": {"field":"x2"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_rect_rejects_y2_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "y2": {"field":"y2"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_rect_rejects_quantitative_x() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":1,"y":2,"v":3}]},
        "encoding": {
            "x": {"field":"x","type":"quantitative"},
            "y": {"field":"y"},
            "color": {"field":"v"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "quantitative x should be rejected in strict"
    );
    // 非 strict では文字列化して受理される(既存の緩さと同型)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_quantitative_y() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":2,"v":3}]},
        "encoding": {
            "x": {"field":"x"},
            "y": {"field":"y","type":"quantitative"},
            "color": {"field":"v"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "quantitative y should be rejected in strict"
    );
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_unsupported_aggregate() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"v","aggregate":"count"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "aggregate=count should be rejected"
    );
    // 非 strict では既存挙動(未対応値は無視)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_nominal_color_with_aggregate() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":"cat0"}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"c","type":"nominal","aggregate":"sum"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "nominal + aggregate should be rejected in strict"
    );
}

#[test]
fn strict_rect_rejects_non_string_aggregate() {
    // aggregate が数値など非文字列のとき、strict は明示 Err にする。
    // (as_str で None になり silently 無視されるバグを pin する。)
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"v","aggregate":1}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "non-string aggregate should be rejected in strict"
    );
}

#[test]
fn strict_rect_rejects_inferred_nominal_with_aggregate() {
    // encoding.color.type 省略 + データが文字列 → 推論で nominal → aggregate は無効。
    // 現状 check_unknown_keys は explicit nominal しか reject しないので、
    // 推論経由のケースを pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":"cat0"}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"c","aggregate":"sum"}
        }
    }"#;
    assert!(
        vegalite::parse(json, true).is_err(),
        "inferred nominal + aggregate should be rejected in strict"
    );
    // 非 strict では既存の緩さを維持(aggregate 指定は silently 無視される)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn rect_mark_renders_svg_with_expected_rect_count() {
    // 2x2 grid, all cells present → 4 rects.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},
            {"x":"B","y":"X","v":2},
            {"x":"A","y":"Y","v":3},
            {"x":"B","y":"Y","v":4}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
    let rect_count = svg.matches("<rect").count();
    // 4 cells + 1 vegalite_theme white background rect.
    assert_eq!(
        rect_count, 5,
        "expected 4 cells + 1 background, got {rect_count}"
    );
    // Axis labels appear.
    assert!(svg.contains(">A<"));
    assert!(svg.contains(">B<"));
    assert!(svg.contains(">X<"));
    assert!(svg.contains(">Y<"));
}

#[test]
fn rect_mark_skips_missing_cells() {
    // (B, Y) missing → 3 cells, None cell does not emit <rect>.
    let json = r#"{
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
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    let rect_count = svg.matches("<rect").count();
    // 3 cells (one None skipped) + 1 background.
    assert_eq!(
        rect_count, 4,
        "expected 3 cells + 1 background, got {rect_count}"
    );
}

#[test]
fn rect_mark_rejects_quantitative_with_non_numeric_color() {
    // encoding.color.type: "quantitative" は数値を要求。文字列/bool の場合、
    // 黙って空チャートを返さず明示 Err にする(bar/line の validate_numeric と同型)。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":"foo"},
            {"x":"B","y":"X","v":"bar"}
        ]},
        "encoding": {
            "x": {"field":"x"},
            "y": {"field":"y"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let err = vegalite::parse(json, false).unwrap_err();
    assert!(
        err.contains("数値") || err.contains("v"),
        "expected numeric-type error, got: {err}"
    );
}

#[test]
fn rect_mark_degenerate_all_equal_values_use_hi() {
    // range が 0 (全値同一) のとき、全セルは RECT_COLOR_HI = Tableau steel-blue。
    // 白セルが白背景に埋没しないよう HI を採用する invariant を pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":5},
            {"x":"B","y":"X","v":5},
            {"x":"A","y":"Y","v":5},
            {"x":"B","y":"Y","v":5}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    for row in &cells {
        for cell in row {
            let c = cell.expect("all cells should be Some (degenerate uses HI)");
            assert_eq!(
                (c.r, c.g, c.b),
                (76, 120, 168),
                "degenerate min==max should resolve every cell to RECT_COLOR_HI"
            );
        }
    }
}

#[test]
fn rect_hi_color_matches_vegalite_palette_head() {
    // RECT_COLOR_HI は Vega-Lite テーマの palette[0] (Tableau10 steel-blue) と揃える
    // 前提。パレット定数の drift を pin する。
    let palette_head = fulgur_chart::palette::vegalite_theme().palette[0];
    assert_eq!(
        (palette_head.r, palette_head.g, palette_head.b),
        (76, 120, 168),
        "vegalite palette[0] must remain (76, 120, 168); RECT_COLOR_HI depends on this"
    );
}

#[test]
fn rect_mark_aggregate_mean_running_avoids_overflow() {
    // [1e308, 1e308] の mean は数学的には 1e308。単純 sum → 除算だと intermediate が
    // inf に overflow して cell が None に落ちる。running mean は finite を返す。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1.0e308},
            {"x":"A","y":"X","v":1.0e308}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // 単一有限セル → degenerate → RECT_COLOR_HI (76, 120, 168)。
    // 重要なのは None (overflow 起因) にならないこと。
    let ax = cells[0][0].expect("(A,X) should not be None due to sum overflow");
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
}

#[test]
fn rect_mark_range_overflow_treated_as_degenerate() {
    // max - min が inf に overflow するケース → 全セル HI (白 埋没を防ぐ)。
    // 現状 lerp が NaN → 0 → LO で silently 全白になるバグを pin する。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":-1.0e308},
            {"x":"B","y":"X","v":1.0e308}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    let ax = cells[0][0].expect("(A,X) should be Some (degenerate uses HI)");
    let bx = cells[0][1].expect("(B,X) should be Some (degenerate uses HI)");
    // Degenerate treatment → both cells = HI, not both = LO (白背景に埋没する古い挙動)。
    assert_eq!((ax.r, ax.g, ax.b), (76, 120, 168));
    assert_eq!((bx.r, bx.g, bx.b), (76, 120, 168));
}

#[test]
fn rect_mark_inspect_model_reports_labels() {
    // build_model_core は spec.series/categories が空だと datasets=0/x_ticks=0 になるが、
    // VegaRect は x/y_labels に情報が入っているので特殊対応で正しい counts を返す。
    let json = r#"{
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
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let model = fulgur_chart::model::build_model_core(&spec);
    assert_eq!(model.meta.r#type, "vegaRect");
    assert_eq!(
        model.counts.x_ticks, 2,
        "x_ticks should reflect x_labels len"
    );
    assert_eq!(
        model.counts.y_ticks, 2,
        "y_ticks should reflect y_labels len"
    );
    assert_eq!(model.counts.datasets, 1);
    assert_eq!(model.counts.legend_items, 0);
}
