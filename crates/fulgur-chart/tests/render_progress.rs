use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn progress_renders_svg() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[70]}]}}"#);
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
fn progress_two_bars_two_tracks_two_foregrounds() {
    // 値が 2 つ → トラック 2 + 前景 2 = path 4
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[70,40]}]}}"#);
    assert_eq!(count(&svg, "<path"), 4, "{svg}");
}

#[test]
fn progress_zero_value_is_track_only() {
    // 0% は前景パスを描かない（トラックのみ → path 1）
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[0]}]}}"#);
    assert_eq!(count(&svg, "<path"), 1, "{svg}");
}

#[test]
fn progress_foreground_uses_solid_background_color() {
    // 前景色は backgroundColor、ソリッド（fill-opacity の半透明指定がない）
    let svg = render(
        r##"{"type":"progress","data":{"datasets":[{"data":[60],"backgroundColor":"#ff0000"}]}}"##,
    );
    assert!(svg.contains("#ff0000"), "foreground color missing: {svg}");
    assert!(
        !svg.contains("fill-opacity=\"0.5\""),
        "should be solid: {svg}"
    );
}

#[test]
fn progress_shows_percentage_by_default() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[70]}]}}"#);
    assert!(svg.contains(">70%<"), "percentage label missing: {svg}");
}

#[test]
fn progress_datalabels_display_false_hides_percentage() {
    let svg = render(
        r#"{"type":"progress","data":{"datasets":[{"data":[70]}]},
        "options":{"plugins":{"datalabels":{"display":false}}}}"#,
    );
    assert!(!svg.contains('%'), "percentage should be hidden: {svg}");
}

#[test]
fn progress_renders_bar_names_from_labels() {
    let svg = render(
        r#"{"type":"progress","data":{"labels":["CPU","RAM"],"datasets":[{"data":[30,80]}]}}"#,
    );
    assert!(svg.contains(">CPU<"), "bar name CPU missing: {svg}");
    assert!(svg.contains(">RAM<"), "bar name RAM missing: {svg}");
}

#[test]
fn progress_second_dataset_overrides_max() {
    // 15 / 30 = 50%
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[15]},{"data":[30]}]}}"#);
    assert!(svg.contains(">50%<"), "expected 50%: {svg}");
}

#[test]
fn progress_clamps_over_max_to_100() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[150]}]}}"#);
    assert!(svg.contains(">100%<"), "expected clamp to 100%: {svg}");
}

#[test]
fn progress_empty_data_no_panic() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[]}]}}"#);
    assert!(
        svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
        "{svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}

#[test]
fn progress_no_datasets_no_panic() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[]}}"#);
    assert!(svg.starts_with("<svg"), "{svg}");
}

#[test]
fn progress_deterministic() {
    let j = r##"{"type":"progress","data":{"labels":["A","B"],
        "datasets":[{"data":[25,90],"backgroundColor":["#36a2eb","#ff6384"]}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn progress_snapshot() {
    let svg = render(
        r##"{"type":"progress","data":{"labels":["CPU","Memory","Disk"],
        "datasets":[{"data":[30,72,95],"backgroundColor":"#36a2eb"}]},
        "options":{"plugins":{"title":{"display":true,"text":"System Usage"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn progress_bar_alias_is_accepted() {
    // QuickChart の正式 type 名 "progressBar" もエイリアスとして受理する。
    let svg = render(r#"{"type":"progressBar","data":{"datasets":[{"data":[70]}]}}"#);
    assert!(
        svg.contains(">70%<"),
        "progressBar alias should render: {svg}"
    );
}

#[test]
fn progress_bars_render_in_png() {
    // 回帰テスト: 角丸パスが PNG 用の直接ラスタライザ(raster_direct)で実際に
    // 描画されること。パスのコマンドと座標が連結していると parse_path_data が
    // 失敗し、PNG でバーが消える(テキストのみになる)。前景色のピクセルが
    // PNG に現れることで、パスが解釈・描画されたことを保証する。
    use resvg::tiny_skia::Pixmap;
    let spec = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[100],"backgroundColor":"#36a2eb"}]}}"##,
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
    // 前景色 #36a2eb = (54, 162, 235)。ソリッド(不透明)なので内部ピクセルは厳密一致。
    let found = pixmap
        .pixels()
        .iter()
        .any(|p| p.red() == 54 && p.green() == 162 && p.blue() == 235);
    assert!(
        found,
        "progress foreground bar (#36a2eb) must be rasterized into the PNG"
    );
}

#[test]
fn progress_strict_accepts_datalabels() {
    // datalabels は ProgressPlugins に残っているため strict でも通る（回帰確認）
    let ok = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70]}]},"options":{"plugins":{"datalabels":{"display":false}}}}"##,
        true,
    );
    assert!(ok.is_ok(), "datalabels should be accepted: {:?}", ok);
}
