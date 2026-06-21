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

#[test]
fn radial_gauge_shows_center_value_by_default() {
    // displayText 既定 true → 中央に丸めた値テキスト。
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[72]}]}}"#);
    assert!(svg.contains(">72<"), "center value missing: {svg}");
}

#[test]
fn radial_gauge_center_text_hidden_when_disabled() {
    let svg = render(
        r#"{"type":"radialGauge","data":{"datasets":[{"data":[72]}]},
        "options":{"centerArea":{"displayText":false}}}"#,
    );
    assert!(
        !svg.contains(">72<"),
        "center value should be hidden: {svg}"
    );
}

#[test]
fn radial_gauge_rounded_default_adds_caps() {
    // roundedCorners 既定 true → 値弧の両端に半円キャップ(<circle>)が出る。
    // flat(false)指定時はキャップなし(radialGauge は針なし=他に circle 無し)。
    // 値が中間(両端が露出)で比較。キャップは Prim::Circle → <circle> 要素。
    let rounded = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[50]}]}}"#);
    let flat = render(
        r#"{"type":"radialGauge","data":{"datasets":[{"data":[50]}]},
        "options":{"roundedCorners":false}}"#,
    );
    assert!(
        rounded.matches("<circle").count() > flat.matches("<circle").count(),
        "rounded should add cap circles: rounded={} flat={}",
        rounded.matches("<circle").count(),
        flat.matches("<circle").count()
    );
}

#[test]
fn gauge_renders_one_path_per_zone() {
    // data=[2,4,6] → 3 ゾーン。各ゾーン 1 path + 針。ゾーン path は 3 つ以上。
    let svg = render(
        r##"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"##,
    );
    assert!(count(&svg, "<path") >= 3, "3 zones: {svg}");
    assert!(
        svg.contains("#00ff00") && svg.contains("#ff0000"),
        "zone colors: {svg}"
    );
}

#[test]
fn gauge_needle_present() {
    // 針(三角形 path or polygon)が描かれる。針色 黒(既定)。
    let svg = render(
        r##"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"##,
    );
    // 針は polygon/path。最低限 path 数がゾーン数より多い(針を足した分)。
    assert!(count(&svg, "<path") >= 4, "needle adds a path: {svg}");
}

#[test]
fn gauge_no_panic_on_empty_zones() {
    let svg = render(r#"{"type":"gauge","data":{"datasets":[{"value":0,"data":[]}]}}"#);
    assert!(svg.starts_with("<svg") && !svg.contains("NaN"), "{svg}");
}

#[test]
fn gauge_shows_value_label_by_default() {
    let svg = render(
        r##"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"##,
    );
    assert!(svg.contains(">3<"), "value label missing: {svg}");
}

#[test]
fn gauge_value_label_hidden_when_disabled() {
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6]}]},
        "options":{"valueLabel":{"display":false}}}"#,
    );
    assert!(!svg.contains(">3<"), "value label should be hidden: {svg}");
}

#[test]
fn gauge_deterministic() {
    let j = r##"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn radial_gauge_deterministic() {
    let j = r##"{"type":"radialGauge","data":{"datasets":[{"data":[63],"backgroundColor":"#36a2eb"}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn radial_gauge_snapshot() {
    let svg = render(
        r##"{"type":"radialGauge","data":{"datasets":[{"data":[63],"backgroundColor":"#36a2eb"}]},
        "options":{"plugins":{"title":{"display":true,"text":"CPU"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn gauge_snapshot() {
    let svg = render(
        r##"{"type":"gauge","data":{"datasets":[{"value":58,"minValue":0,"data":[33,66,100],
        "backgroundColor":["#4caf50","#ffc107","#f44336"]}]},
        "options":{"plugins":{"title":{"display":true,"text":"Load"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn radial_gauge_rasterizes_value_color_in_png() {
    // 回帰: 弧パスが PNG 直接ラスタライザで描画される。前景色ピクセルが PNG に現れる。
    use resvg::tiny_skia::Pixmap;
    let spec = chartjs::parse(
        r##"{"type":"radialGauge","data":{"datasets":[{"data":[100],"backgroundColor":"#36a2eb"}]},
        "options":{"roundedCorners":false}}"##,
        false,
    )
    .unwrap();
    let png = fulgur_chart::raster_direct::render_chart_to_png(
        &spec,
        1.0,
        fulgur_chart::font::DEFAULT_FONT,
    )
    .unwrap();
    let pixmap = Pixmap::decode_png(&png).unwrap();
    let found = pixmap
        .pixels()
        .iter()
        .any(|p| p.red() == 54 && p.green() == 162 && p.blue() == 235);
    assert!(
        found,
        "radialGauge value arc (#36a2eb) must be rasterized into the PNG"
    );
}

#[test]
fn gauge_strict_rejects_unknown_key() {
    let err = chartjs::parse(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4],"bogus":1}]}}"#,
        true,
    );
    assert!(err.is_err(), "strict should reject unknown dataset key");
}

#[test]
fn gauge_strict_accepts_known_keys() {
    let ok = chartjs::parse(
        r##"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4],
        "backgroundColor":["#0f0","#f00"]}]},
        "options":{"needle":{"color":"#000"},"valueLabel":{"display":true}}}"##,
        true,
    );
    assert!(ok.is_ok(), "known keys should pass strict: {ok:?}");
}
