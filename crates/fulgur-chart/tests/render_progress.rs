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
