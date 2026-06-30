use fulgur_chart::frontend::chartjs;
use fulgur_chart::ir::{ChartKind, Point, SeriesType};

#[test]
fn parses_minimal_bar_spec() {
    let json = r#"{
      "type": "bar",
      "data": {
        "labels": ["1月", "2月", "3月"],
        "datasets": [{ "label": "売上", "data": [120, 200, 150] }]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            horizontal: false,
            ..
        }
    ));
    assert_eq!(spec.categories, vec!["1月", "2月", "3月"]);
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].name, "売上");
    assert_eq!(spec.series[0].values, vec![120.0, 200.0, 150.0]);
    // 色未指定 → パレット先頭(#36A2EB) を全点へブロードキャスト(len==1)
    let c = spec.series[0].fill_at(0);
    assert_eq!((c.r, c.g, c.b), (54, 162, 235));
    assert_eq!(spec.series[0].fill.len(), 1); // bar は系列1色
}

#[test]
fn horizontal_bar_via_index_axis_y() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y"} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            horizontal: true,
            ..
        }
    ));
}

#[test]
fn pie_with_per_slice_colors() {
    let json = r##"{ "type":"pie","data":{"labels":["a","b","c"],
      "datasets":[{"data":[1,2,3],"backgroundColor":["#ff0000","#00ff00","#0000ff"]}]} }"##;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].fill.len(), 3);
    let c2 = spec.series[0].fill_at(2);
    assert_eq!((c2.r, c2.g, c2.b), (0, 0, 255));
}

#[test]
fn pie_without_colors_uses_palette_per_slice() {
    let json = r#"{ "type":"pie","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].fill.len(), 2); // pie はスライス別パレット
    assert_ne!(spec.series[0].fill_at(0), spec.series[0].fill_at(1));
}

#[test]
fn area_fill_string_mode_is_filled() {
    let json = r#"{ "type":"line","data":{"labels":["a"],
      "datasets":[{"data":[1],"fill":"origin"}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[0].area);
}

#[test]
fn title_from_plugins() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},
      "options":{"plugins":{"title":{"display":true,"text":"四半期売上"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.title.as_deref(), Some("四半期売上"));
}

#[test]
fn title_not_displayed_is_none() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},
      "options":{"plugins":{"title":{"display":false,"text":"x"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.title, None);
}

#[test]
fn invalid_json_is_err() {
    assert!(chartjs::parse("{ not json", false).is_err());
}

#[test]
fn unknown_type_is_err() {
    let json = r#"{ "type":"unknownChart","data":{"labels":[],"datasets":[]} }"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn parses_radar_spec() {
    let json = r#"{
      "type": "radar",
      "data": {
        "labels": ["速度", "力", "技"],
        "datasets": [
          { "label": "A", "data": [60, 80, 40] },
          { "label": "B", "data": [50, 30, 90] }
        ]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Radar));
    assert_eq!(spec.categories, vec!["速度", "力", "技"]);
    assert_eq!(spec.series.len(), 2);
    assert_eq!(spec.series[0].values, vec![60.0, 80.0, 40.0]);
    assert_eq!(spec.series[1].values, vec![50.0, 30.0, 90.0]);
    // radar はカテゴリ系なので点データは空。
    assert!(spec.series[0].points.is_empty());
    // r 軸はゼロ起点(begin_at_zero)。
    assert!(spec.y_axis.begin_at_zero);
}

#[test]
fn strict_accepts_radar() {
    let json = r#"{ "type":"radar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_unknown_top_level_key() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},"wat":1 }"#;
    assert!(chartjs::parse(json, true).is_err()); // strict は未知キーで Err
    assert!(chartjs::parse(json, false).is_ok()); // 非strict は無視
}

#[test]
fn strict_rejects_unknown_dataset_key() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],
      "datasets":[{"data":[1],"bogusKey":1}]} }"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok());
}

#[test]
fn datalabels_key_present_enables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{}}} }"#;
    assert!(chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_display_true_enables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":true}}} }"#;
    assert!(chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_display_false_disables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":false}}} }"#;
    assert!(!chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_absent_is_false() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    assert!(!chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn strict_accepts_known_datalabels_keys() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}
#[test]
fn strict_rejects_unknown_datalabels_key() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"foo":1}}} }"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn scales_y_only_on_vertical_is_not_stacked() {
    // 縦棒(既定 indexAxis:x)で値軸 y のみ stacked → chart.js は棒を dodge(並置)する。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: true,
            ..
        }
    ));
}

#[test]
fn scales_x_stacked_true_marks_bar_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: true,
            value_stacked: false,
            ..
        }
    ));
}

#[test]
fn horizontal_y_stacked_marks_bar_stacked() {
    // 横棒(indexAxis:y)は index 軸が y。y.stacked → 積み上げ。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y","scales":{"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: true,
            value_stacked: false,
            horizontal: true,
            ..
        }
    ));
}

#[test]
fn horizontal_x_stacked_only_is_not_stacked() {
    // 横棒(indexAxis:y)で値軸 x のみ stacked → index 軸(y)未指定なので dodge。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y","scales":{"x":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: true,
            horizontal: true,
            ..
        }
    ));
}

#[test]
fn scales_absent_is_not_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: false,
            ..
        }
    ));
}

#[test]
fn scales_stacked_false_is_not_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":false}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: false,
            value_stacked: false,
            ..
        }
    ));
}

#[test]
fn strict_accepts_scales_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_accepts_scales_offset() {
    // chart.js category スケールの offset は認識済みキー。strict でも通る。
    let json = r#"{ "type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
      "options":{"scales":{"x":{"offset":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn both_axes_stacked_sets_both_flags() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        ChartKind::Bar {
            placement_stacked: true,
            value_stacked: true,
            ..
        }
    ));
}

#[test]
fn parses_scatter_point_data() {
    let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2},{"x":3,"y":4}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
    assert_eq!(
        spec.series[0].points,
        vec![
            Point {
                x: 1.0,
                y: 2.0,
                r: None
            },
            Point {
                x: 3.0,
                y: 4.0,
                r: None
            },
        ]
    );
    // scatter は数値配列を使わない。
    assert!(spec.series[0].values.is_empty());
}

#[test]
fn categorical_bar_has_empty_points() {
    // 既存のカテゴリ系パース(数値配列)は points を空に保つ。
    let json = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].values, vec![1.0, 2.0]);
    assert!(spec.series[0].points.is_empty());
}

#[test]
fn strict_accepts_scatter() {
    let json = r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2}]}]}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn parses_bubble_point_data_with_radius() {
    // bubble は scatter と同じ点データだが、第3次元 r を保持する。
    let json = r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"r":10}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bubble));
    assert_eq!(spec.series[0].points[0].r, Some(10.0));
    assert_eq!(
        spec.series[0].points[0],
        Point {
            x: 1.0,
            y: 2.0,
            r: Some(10.0)
        }
    );
    // 点ベースなので数値配列は使わない。
    assert!(spec.series[0].values.is_empty());
}

#[test]
fn bar_base_with_line_dataset_is_mixed() {
    // 基本型 bar + dataset 別 type:"line" → Mixed、種別は [Bar, Line]。
    let json = r#"{"type":"bar","data":{"labels":["a","b","c"],
      "datasets":[{"label":"棒","data":[1,2,3]},{"type":"line","label":"折れ線","data":[4,5,6]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Mixed));
    assert_eq!(spec.series[0].series_type, SeriesType::Bar);
    assert_eq!(spec.series[1].series_type, SeriesType::Line);
}

#[test]
fn all_bar_without_type_stays_bar() {
    // dataset 別 type 未指定の全棒は従来どおり Bar(混合に昇格しない)。
    let json = r#"{"type":"bar","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]},{"data":[3,4]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { .. }));
    assert_eq!(spec.series[0].series_type, SeriesType::Bar);
    assert_eq!(spec.series[1].series_type, SeriesType::Bar);
}

#[test]
fn line_base_with_bar_dataset_is_mixed() {
    // 基本型 line + dataset 別 type:"bar" でも混合になる(対称性の確認)。
    let json = r#"{"type":"line","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]},{"type":"bar","data":[3,4]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Mixed));
    assert_eq!(spec.series[0].series_type, SeriesType::Line);
    assert_eq!(spec.series[1].series_type, SeriesType::Bar);
}

#[test]
fn strict_accepts_dataset_type() {
    let json = r#"{"type":"bar","data":{"labels":["a"],
      "datasets":[{"data":[1]},{"type":"line","data":[2]}]}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn single_dataset_type_override_changes_kind() {
    // 基本 type=bar でも、全 dataset が type:"line" なら kind は Line になる
    // (混合でない単独上書き。以前は kind=Bar のままで line が棒描画されていた)。
    let json = r#"{"type":"bar","data":{"labels":["a","b"],
      "datasets":[{"type":"line","data":[1,2]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line));
    assert_eq!(spec.series[0].series_type, SeriesType::Line);
}

#[test]
fn unsupported_dataset_type_errors() {
    // bar 基本型に scatter dataset を混ぜるのは未対応。点データが空で「成功扱いの
    // 空チャート」になるのを防ぎ、明示エラーにする。
    let json = r#"{"type":"bar","data":{"labels":["a"],
      "datasets":[{"type":"scatter","data":[{"x":1,"y":2}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn radar_rejects_negative_values() {
    // 負の半径は頂点が反対スポークへ反転するため、レーダーは負値を拒否する。
    let json = r#"{"type":"radar","data":{"labels":["a","b","c"],
      "datasets":[{"data":[3,-1,2]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn mixed_with_horizontal_or_stacked_errors() {
    let base_datasets =
        r#""data":{"labels":["a"],"datasets":[{"data":[1]},{"type":"line","data":[2]}]}"#;
    // 横棒×混合 → エラー(mixed は縦・非積み上げのみ)。
    let horiz = format!(r#"{{"type":"bar",{base_datasets},"options":{{"indexAxis":"y"}}}}"#);
    assert!(chartjs::parse(&horiz, false).is_err());
    // placement_stacked×混合 → エラー。
    let stk = format!(
        r#"{{"type":"bar",{base_datasets},"options":{{"scales":{{"x":{{"stacked":true}}}}}}}}"#
    );
    assert!(chartjs::parse(&stk, false).is_err());
    // value_stacked×混合 → エラー(ChartKind::Mixed にフラグが伝わらず消えるため)。
    let vstk = format!(
        r#"{{"type":"bar",{base_datasets},"options":{{"scales":{{"y":{{"stacked":true}}}}}}}}"#
    );
    assert!(chartjs::parse(&vstk, false).is_err());
    // 通常の混合は従来どおり Mixed。
    let ok = format!(r#"{{"type":"bar",{base_datasets}}}"#);
    assert!(matches!(
        chartjs::parse(&ok, false).unwrap().kind,
        ChartKind::Mixed
    ));
}

#[test]
fn strict_rejects_unknown_point_key() {
    // 点オブジェクト {x,y,r} の typo(radius は r が正) を strict で検出。
    let json = r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"radius":20}]}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok()); // 非strict は無視
}

#[test]
fn data_shape_mismatch_errors() {
    // scatter に数値配列 → 点データが空になる空チャート化を防ぎ、明示エラーに。
    let scatter_nums = r#"{"type":"scatter","data":{"datasets":[{"data":[1,2,3]}]}}"#;
    assert!(chartjs::parse(scatter_nums, false).is_err());
    // bar に {x,y} 点配列 → エラー(values が空になる欠損を防ぐ)。
    let bar_points =
        r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[{"x":1,"y":2}]}]}}"#;
    assert!(chartjs::parse(bar_points, false).is_err());
}

#[test]
fn dataset_type_on_non_mixable_base_errors() {
    // pie に dataset type:line → 無視して別種描画せず、明示エラーに。
    let json = r#"{"type":"pie","data":{"labels":["a","b"],
      "datasets":[{"type":"line","data":[1,2]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn strict_rejects_scales_typo() {
    // stacked は描画に効くので、typo を strict で取りこぼさない。
    let typo = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stakced":true}}}}"#;
    assert!(chartjs::parse(typo, true).is_err());
    assert!(chartjs::parse(typo, false).is_ok());
    // 正しい stacked キーは strict でも通る。
    let ok = r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}}}"#;
    assert!(chartjs::parse(ok, true).is_ok());
}

#[test]
fn matrix_parses_categories_and_series() {
    let json = r#"{
        "type": "matrix",
        "data": {"datasets": [{"label": "h", "data": [
            {"x": "Mon", "y": "Morning", "v": 5.0},
            {"x": "Tue", "y": "Morning", "v": 8.0},
            {"x": "Mon", "y": "Evening", "v": 3.0},
            {"x": "Tue", "y": "Evening", "v": 9.0}
        ], "backgroundColor": "rgba(54,162,235,1.0)"}]}
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Matrix { .. }));
    assert_eq!(spec.categories, vec!["Mon", "Tue"]);
    assert_eq!(spec.series.len(), 2);
    assert_eq!(spec.series[0].name, "Morning");
    assert_eq!(spec.series[0].values, vec![5.0, 8.0]);
    assert_eq!(spec.series[1].name, "Evening");
    assert_eq!(spec.series[1].values, vec![3.0, 9.0]);
}

#[test]
fn matrix_multiple_datasets_is_error() {
    let json = r#"{"type":"matrix","data":{"datasets":[
        {"data":[{"x":"A","y":"X","v":1}]},
        {"data":[{"x":"A","y":"X","v":2}]}
    ]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn matrix_missing_cell_becomes_nan() {
    let json = r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":1.0},
        {"x":"Tue","y":"Morning","v":2.0},
        {"x":"Mon","y":"Evening","v":3.0}
    ]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[1].values[1].is_nan());
}

#[test]
fn matrix_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "matrix",
        "data": {
            "datasets": [{
                "label": "Heat",
                "data": [{"x": "Mon", "y": "AM", "v": 5.0}],
                "backgroundColor": "#36a2eb"
            }]
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Matrix(_)));
}

#[test]
fn treemap_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    // Grouped tree (objects + key/groups), exercising the hierarchy path.
    let grouped = r##"{
        "type": "treemap",
        "options": {"plugins": {"title": {"display": true, "text": "T"}}},
        "data": {
            "datasets": [{
                "key": "value",
                "groups": ["region", "product"],
                "tree": [
                    {"region": "EMEA", "product": "A", "value": 12},
                    {"region": "APAC", "product": "B", "value": 7}
                ]
            }]
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(grouped).unwrap();
    assert!(matches!(spec, ChartJsSpec::Treemap(_)));
    // The same document must be accepted by the runtime parser in strict mode.
    assert!(
        chartjs::parse(grouped, true).is_ok(),
        "strict parser should accept grouped treemap"
    );

    // Flat numeric tree (the untagged Numbers branch).
    let numeric = r##"{
        "type": "treemap",
        "data": {"datasets": [{"tree": [6, 4, 3, 2, 1]}]}
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(numeric).unwrap();
    assert!(matches!(spec, ChartJsSpec::Treemap(_)));
    assert!(
        chartjs::parse(numeric, true).is_ok(),
        "strict parser should accept numeric treemap"
    );

    // Documented asymmetry: the JSON Schema is a deliberate superset, so it accepts
    // an object tree without `key` (the untagged enum can't make `key` conditionally
    // required), but the runtime parser rejects it because `key` is required to sum
    // object values into a hierarchy.
    let object_no_key = r##"{
        "type": "treemap",
        "data": {"datasets": [{"groups": ["g"], "tree": [{"g": "a", "v": 1}]}]}
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(object_no_key)
        .expect("schema (superset) should accept object tree without key");
    assert!(matches!(spec, ChartJsSpec::Treemap(_)));
    assert!(
        chartjs::parse(object_no_key, false).is_err(),
        "runtime parser must reject an object tree without key"
    );
}

#[test]
fn matrix_strict_mode_accepts_v_key() {
    let json = r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1}
    ]}]}}"#;
    // strict モードでも matrix は受理されるべき
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict mode should accept matrix with v key"
    );
}

#[test]
fn parses_polar_area_spec() {
    let json = r#"{
      "type": "polarArea",
      "data": {
        "labels": ["A", "B", "C"],
        "datasets": [{ "data": [10, 20, 30] }]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::PolarArea));
    assert_eq!(spec.categories, vec!["A", "B", "C"]);
    assert_eq!(spec.series[0].values, vec![10.0, 20.0, 30.0]);
}

#[test]
fn wordcloud_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    // color 配列 + options 付き
    let json = r##"{
        "type": "wordCloud",
        "data": {
            "labels": ["Rust", "SVG", "Chart"],
            "datasets": [{"data": [90.0, 60.0, 45.0], "color": ["#e63946", "#457b9d", "#2a9d8f"]}]
        },
        "options": {
            "elements": {"word": {"minRotation": -90.0, "maxRotation": 0.0, "rotationSteps": 2, "padding": 2.0}}
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::WordCloud(_)));

    // scalar color
    let scalar = r##"{"type":"wordCloud","data":{"labels":["Hi"],"datasets":[{"data":[40.0],"color":"#ff0000"}]}}"##;
    let s: ChartJsSpec = serde_json::from_str(scalar).unwrap();
    assert!(matches!(s, ChartJsSpec::WordCloud(_)));

    // options なし
    let minimal = r##"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[20.0]}]}}"##;
    let m: ChartJsSpec = serde_json::from_str(minimal).unwrap();
    assert!(matches!(m, ChartJsSpec::WordCloud(_)));
}

#[test]
fn strict_accepts_wordcloud_with_width_height() {
    let json = r#"{"type":"wordCloud","width":800,"height":600,"data":{"labels":["A"],"datasets":[{"data":[30.0]}]}}"#;
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict mode should allow width/height"
    );
}

#[test]
fn strict_rejects_wordcloud_unknown_top_level_key() {
    let json =
        r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"typo":1}"#;
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict mode should reject unknown top-level key"
    );
    assert!(
        chartjs::parse(json, false).is_ok(),
        "non-strict should ignore unknown key"
    );
}

#[test]
fn strict_rejects_wordcloud_unknown_dataset_key() {
    let json =
        r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0],"typo":1}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok());
}

#[test]
fn strict_accepts_wordcloud_elements_word() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"elements":{"word":{"minRotation":-90,"maxRotation":0,"rotationSteps":2,"padding":2}}}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_wordcloud_unknown_word_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"elements":{"word":{"minRotation":-90,"typo":1}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn strict_accepts_wordcloud_plugins_title() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"plugins":{"title":{"display":true,"text":"Cloud"}}}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_wordcloud_unknown_plugins_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"plugins":{"legend":{}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn strict_accepts_wordcloud_theme() {
    let json = r##"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"theme":{"palette":"warm","textColor":"#333"}}}"##;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_wordcloud_unknown_theme_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"theme":{"unknownKey":1}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn strict_rejects_wordcloud_unknown_options_key() {
    let json = r#"{"type":"wordCloud","data":{"labels":["A"],"datasets":[{"data":[30.0]}]},"options":{"typo":1}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn sankey_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "sankey",
        "data": { "datasets": [{
            "label": "Energy",
            "data": [
                {"from": "A", "to": "B", "flow": 10},
                {"from": "A", "to": "C", "flow": 5},
                {"from": "B", "to": "C", "flow": 10}
            ],
            "colorFrom": "#36a2eb",
            "colorTo": "#ff6384",
            "colorMode": "gradient",
            "labels": {"A": "Alpha"},
            "priority": {"A": 0},
            "column": {"A": 0}
        }],
        "labels": []
        },
        "options": { "plugins": { "title": {"display": true, "text": "T"} } }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Sankey(_)));
    // 同じ文書を strict パーサも受理すること(parser↔schema パリティ)。
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict parser should accept sankey"
    );
}

#[test]
fn sankey_basic_parse() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[
        {"from":"A","to":"B","flow":10},
        {"from":"A","to":"C","flow":5},
        {"from":"B","to":"C","flow":10},
        {"from":"C","to":"D","flow":15}
    ]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(
        spec.kind,
        fulgur_chart::ir::ChartKind::Sankey { .. }
    ));
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].links.len(), 4);
    assert_eq!(spec.series[0].links[0].from, "A");
    assert_eq!(spec.series[0].links[0].flow, 10.0);
}

#[test]
fn sankey_defaults_match_chartjs() {
    use fulgur_chart::ir::{ChartKind, Color, SankeyColorMode, SankeyModeX, SankeySize};
    let json =
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    let ChartKind::Sankey {
        color_from,
        color_to,
        color_mode,
        alpha,
        node_width,
        node_padding,
        mode_x,
        size,
        border_width,
        ..
    } = spec.kind
    else {
        panic!()
    };
    assert_eq!(
        color_from,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0
        }
    ); // 'red'
    assert_eq!(
        color_to,
        Color {
            r: 0,
            g: 128,
            b: 0,
            a: 1.0
        }
    ); // 'green'
    assert_eq!(color_mode, SankeyColorMode::Gradient);
    assert!((alpha - 0.5).abs() < 1e-9);
    assert_eq!(node_width, 10.0);
    assert_eq!(node_padding, 10.0);
    assert_eq!(mode_x, SankeyModeX::Edge);
    assert_eq!(size, SankeySize::Max);
    assert_eq!(border_width, 1.0);
}

#[test]
fn sankey_rejects_non_finite_flow() {
    let json =
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":"x"}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn sankey_strict_rejects_unknown_key() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}],"bogus":1}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn sankey_options_override() {
    use fulgur_chart::ir::{ChartKind, Color, SankeyColorMode, SankeyModeX, SankeySize};
    let json = r##"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":3}],
        "colorFrom":"#102030",
        "colorTo":"#405060",
        "colorMode":"from",
        "alpha":0.25,
        "borderColor":"#708090",
        "borderWidth":2.5,
        "color":"#a0b0c0",
        "nodeWidth":14,
        "nodePadding":8,
        "modeX":"even",
        "size":"min",
        "labels":{"A":"Alpha"},
        "priority":{"A":0},
        "column":{"A":2}
    }]}}"##;
    let spec = chartjs::parse(json, false).unwrap();
    let ChartKind::Sankey {
        color_from,
        color_to,
        color_mode,
        alpha,
        node_width,
        node_padding,
        mode_x,
        size,
        border,
        border_width,
        label_color,
        labels,
        priority,
        columns,
    } = spec.kind
    else {
        panic!()
    };
    assert_eq!(
        color_from,
        Color {
            r: 16,
            g: 32,
            b: 48,
            a: 1.0
        }
    );
    assert_eq!(
        color_to,
        Color {
            r: 64,
            g: 80,
            b: 96,
            a: 1.0
        }
    );
    assert_eq!(color_mode, SankeyColorMode::From);
    assert!((alpha - 0.25).abs() < 1e-9);
    assert_eq!(node_width, 14.0);
    assert_eq!(node_padding, 8.0);
    assert_eq!(mode_x, SankeyModeX::Even);
    assert_eq!(size, SankeySize::Min);
    assert_eq!(
        border,
        Color {
            r: 112,
            g: 128,
            b: 144,
            a: 1.0
        }
    );
    assert_eq!(border_width, 2.5);
    assert_eq!(
        label_color,
        Color {
            r: 160,
            g: 176,
            b: 192,
            a: 1.0
        }
    );
    assert_eq!(labels.get("A").map(String::as_str), Some("Alpha"));
    assert_eq!(priority.get("A"), Some(&0.0));
    assert_eq!(columns.get("A"), Some(&2usize));

    // colorMode "to" → To も検証(From↔To のスワップ回帰を捕捉)。
    let json_to = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}],"colorMode":"to"}]}}"#;
    let spec_to = chartjs::parse(json_to, false).unwrap();
    let ChartKind::Sankey { color_mode, .. } = spec_to.kind else {
        panic!()
    };
    assert_eq!(color_mode, SankeyColorMode::To);
}

#[test]
fn sankey_rejects_negative_flow() {
    // 数値として有効だが負の flow は guard で弾く(deserialize は通過する)。
    let json =
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":-5}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn sankey_schema_and_parser_both_reject_datalabels() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // sankey は datalabels を持たない。schema(SankeyPlugins)と strict パーサが共に拒否し、
    // 「schema 受理 → strict 拒否」のパリティ破れを起こさないこと。
    let json = r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":{"datalabels":{}}}}"##;
    assert!(
        serde_json::from_str::<ChartJsSpec>(json).is_err(),
        "schema は sankey の plugins.datalabels を拒否すべき"
    );
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict パーサも plugins.datalabels を拒否すべき"
    );
    // title は schema・parser とも受理する(正常系の確認)。
    let ok = r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":{"title":{"display":true,"text":"T"}}}}"##;
    assert!(matches!(
        serde_json::from_str::<ChartJsSpec>(ok).unwrap(),
        ChartJsSpec::Sankey(_)
    ));
    assert!(chartjs::parse(ok, true).is_ok());
}

#[test]
fn sankey_schema_and_parser_both_reject_legend() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // sankey は legend を描画しないため契約から外す。schema・strict パーサ双方が拒否し、
    // 「strict が受理するのに描画では無視される」silent-drop を避ける。
    let json = r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":{"legend":{"display":true}}}}"##;
    assert!(
        serde_json::from_str::<ChartJsSpec>(json).is_err(),
        "schema は sankey の plugins.legend を拒否すべき"
    );
    assert!(
        chartjs::parse(json, true).is_err(),
        "strict パーサも plugins.legend を拒否すべき"
    );
}

#[test]
fn sankey_rejects_negative_node_width() {
    // 負の nodeWidth は <rect width="-5"> 等の不正 SVG を生むため parse で弾く。
    let json = r#"{"type":"sankey","data":{"datasets":[{"nodeWidth":-5,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
    // 非有限も拒否。
    let nan = r#"{"type":"sankey","data":{"datasets":[{"borderWidth":1e400,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(nan, false).is_err());
}

#[test]
fn sankey_rejects_huge_node_padding() {
    // 巨大な有限 nodePadding は layout の (max_y/height)*node_padding で ∞ に overflow し
    // 図形を潰すため、canvas 最大寸法を超える値は parse で弾く。
    let json = r#"{"type":"sankey","data":{"datasets":[{"nodePadding":1e308,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
    // 妥当な範囲(canvas 上限以下)は受理。
    let ok = r#"{"type":"sankey","data":{"datasets":[{"nodePadding":20,"nodeWidth":15,"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    assert!(chartjs::parse(ok, false).is_ok());
}

#[test]
fn sankey_preserves_dataset_label() {
    // 他のパーサと同様、dataset の label を Series.name に保持する。
    let json = r#"{"type":"sankey","data":{"datasets":[{"label":"Energy","data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].name, "Energy");
}

#[test]
fn sankey_rejects_unknown_enum_values() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // colorMode/modeX/size のタイポは silent default にせず、schema・parser とも拒否する。
    for (field, bad) in [("colorMode", "form"), ("modeX", "edeg"), ("size", "mn")] {
        let json = format!(
            r#"{{"type":"sankey","data":{{"datasets":[{{"{field}":"{bad}","data":[{{"from":"A","to":"B","flow":1}}]}}]}}}}"#
        );
        assert!(
            serde_json::from_str::<ChartJsSpec>(&json).is_err(),
            "schema should reject {field}={bad}"
        );
        assert!(
            chartjs::parse(&json, false).is_err(),
            "parser should reject {field}={bad}"
        );
    }
}

#[test]
fn sankey_accepts_null_options() {
    // schema は optional フィールドを nullable として描くため、parser も明示 null を
    // 既定として受理し、schema-valid な spec が strict で落ちないようにする。
    let null_opts = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":null}"#;
    assert!(chartjs::parse(null_opts, true).is_ok());
    let null_plugins = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]},"options":{"plugins":null}}"#;
    assert!(chartjs::parse(null_plugins, true).is_ok());
}

// ──────────────────────────────────────────────
// fulgur-chart-tgb: top-level width/height は全チャート種別で
// ChartSpec.width/height に反映される(ハードコード 800x450 の置換)。
// ──────────────────────────────────────────────

#[test]
fn main_path_honors_top_level_width_height() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.width, 640.0);
    assert_eq!(spec.height, 360.0);
}

#[test]
fn main_path_defaults_width_height_when_absent() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.width, 800.0);
    assert_eq!(spec.height, 450.0);
}

#[test]
fn strict_allows_top_level_width_height() {
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn special_paths_honor_top_level_width_height() {
    // matrix/treemap/sankey/gauge は主要 parse とは別パス。progress は主要 parse 経由だが
    // 専用 strict check を持つため strict 受理も併せて検証する。
    let cases = [
        r#"{"type":"matrix","data":{"datasets":[{"data":[{"x":"A","y":"X","v":1}]}]},"width":640,"height":360}"#,
        r#"{"type":"treemap","data":{"datasets":[{"tree":[6,4,3,2,1]}]},"width":640,"height":360}"#,
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":10}]}]},"width":640,"height":360}"#,
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],"backgroundColor":["green","yellow","red"]}]},"width":640,"height":360}"#,
        r#"{"type":"progress","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
    ];
    for json in cases {
        let spec = chartjs::parse(json, false).unwrap();
        assert_eq!(spec.width, 640.0, "non-strict width for {json}");
        assert_eq!(spec.height, 360.0, "non-strict height for {json}");
        assert!(
            chartjs::parse(json, true).is_ok(),
            "strict should accept top-level width/height for {json}"
        );
    }
}

#[test]
fn schema_accepts_top_level_width_height_all_kinds() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    // 各 *Spec 構造体を最低 1 つカバーする(PieSpec=pie, OutlabeledPieSpec=outlabeledPie で代表)。
    let cases = [
        r#"{"type":"bar","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"line","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"pie","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2}]}]},"width":640,"height":360}"#,
        r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"r":10}]}]},"width":640,"height":360}"#,
        r#"{"type":"radar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"matrix","data":{"datasets":[{"data":[{"x":"A","y":"X","v":1}]}]},"width":640,"height":360}"#,
        r#"{"type":"treemap","data":{"datasets":[{"tree":[6]}]},"width":640,"height":360}"#,
        r#"{"type":"progress","data":{"datasets":[{"data":[1]}]},"width":640,"height":360}"#,
        r#"{"type":"boxplot","data":{"labels":["A"],"datasets":[{"data":[[10,25,50,75,90]]}]},"width":640,"height":360}"#,
        r#"{"type":"sparkline","data":{"datasets":[{"data":[3,1,4]}]},"width":640,"height":360}"#,
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6]}]},"width":640,"height":360}"#,
        r#"{"type":"radialGauge","data":{"datasets":[{"data":[70]}]},"width":640,"height":360}"#,
        r#"{"type":"outlabeledPie","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},"width":640,"height":360}"#,
        r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":10}]}]},"width":640,"height":360}"#,
    ];
    for json in cases {
        let r: Result<ChartJsSpec, serde_json::Error> = serde_json::from_str(json);
        assert!(
            r.is_ok(),
            "schema should accept top-level width/height for {json}: {:?}",
            r.err()
        );
    }
}

#[test]
fn rendered_svg_reflects_top_level_width_height() {
    // JSON → ChartSpec.width/height → Scene → SVG ルート属性まで一貫してサイズが届くことを
    // end-to-end で検証する(既定 800x450 ではなく指定値で描画される)。
    use fulgur_chart::render::render_chart;
    let json = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"width":640,"height":360}"#;
    let svg = render_chart(&chartjs::parse(json, false).unwrap());
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(svg.contains(r#"width="640""#), "{svg}");
    assert!(svg.contains(r#"height="360""#), "{svg}");
    assert!(svg.contains(r#"viewBox="0 0 640 360""#), "{svg}");
}

// ── decimation (options.plugins.decimation) ──

#[test]
fn decimation_defaults_to_enabled_minmax_when_absent() {
    let spec = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#,
        false,
    )
    .unwrap();
    assert!(spec.decimation.enabled);
    assert_eq!(
        spec.decimation.algorithm,
        fulgur_chart::ir::DecimationAlgorithm::MinMax
    );
}

#[test]
fn decimation_explicit_disable_and_lttb() {
    let json = r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
        "options":{"plugins":{"decimation":{"enabled":false,"algorithm":"lttb","samples":300,"threshold":1000}}}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(!spec.decimation.enabled);
    assert_eq!(
        spec.decimation.algorithm,
        fulgur_chart::ir::DecimationAlgorithm::Lttb
    );
    assert_eq!(spec.decimation.samples, Some(300.0));
    assert_eq!(spec.decimation.threshold, Some(1000.0));
}

#[test]
fn decimation_invalid_algorithm_errors() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"algorithm":"bogus"}}}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn strict_accepts_decimation_keys() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"enabled":true,"algorithm":"min-max","samples":100,"threshold":500}}}}"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn strict_rejects_unknown_decimation_subkey() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"bogus":1}}}}"#;
    assert!(chartjs::parse(json, true).is_err());
}

#[test]
fn schema_strict_parity_decimation_line() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"enabled":true,"algorithm":"lttb","samples":50,"threshold":200}}}}"#;
    // strict side: 厳格パーサも受理する。
    assert!(chartjs::parse(json, true).is_ok());
    // schema side: ChartJsSpec(internally tagged, deny_unknown_fields)でも受理されること。
    let spec: fulgur_chart::schema::ChartJsSpec = serde_json::from_str(json).unwrap();
    let decimation = match spec {
        fulgur_chart::schema::ChartJsSpec::Line(line) => line
            .options
            .and_then(|o| o.plugins)
            .and_then(|p| p.decimation),
        _ => panic!("expected line variant"),
    };
    assert!(decimation.is_some());
}

#[test]
fn schema_rejects_unknown_decimation_algorithm() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"algorithm":"bogus"}}}}"#;
    // strict side は不正 algorithm を拒否する。
    assert!(chartjs::parse(json, true).is_err());
    // schema side も enum 制約で拒否すること（value レベルの parity）。
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    assert!(serde_json::from_value::<fulgur_chart::schema::ChartJsSpec>(v).is_err());
}
