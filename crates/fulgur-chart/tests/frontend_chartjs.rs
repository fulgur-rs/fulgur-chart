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
    let json = r#"{ "type":"polarArea","data":{"labels":[],"datasets":[]} }"#;
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
fn scales_y_stacked_true_marks_bar_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { stacked: true, .. }));
}

#[test]
fn scales_x_stacked_true_marks_bar_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"x":{"stacked":true}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { stacked: true, .. }));
}

#[test]
fn scales_absent_is_not_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { stacked: false, .. }));
}

#[test]
fn scales_stacked_false_is_not_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":false}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { stacked: false, .. }));
}

#[test]
fn strict_accepts_scales_stacked() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"stacked":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
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
