use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::build_scene;
use fulgur_chart::raster_direct::render_chart_to_png;
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::text::TextMeasurer;
use serde_json::{Value, json};

fn render_scene(spec: &fulgur_chart::ir::ChartSpec) -> Scene {
    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    build_scene(spec, &measurer)
}

#[test]
fn chartjs_title_scene_preserves_original_scene_when_plugins_are_hidden() {
    let without_plugins = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
        false,
    )
    .unwrap();
    let hidden_plugins = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]},"options":{"plugins":{"title":{"display":false,"text":"Hidden"},"subtitle":{"display":false,"text":"Also hidden"}}}}"#,
        false,
    )
    .unwrap();

    assert_eq!(
        render_scene(&hidden_plugins),
        render_scene(&without_plugins)
    );
    assert_eq!(
        render_chart(&hidden_plugins),
        render_chart(&without_plugins)
    );
}

#[test]
fn legacy_chart_title_still_renders_without_chartjs_title_plugins() {
    let mut spec = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
        false,
    )
    .unwrap();
    spec.title = Some("Legacy title".into());
    assert!(spec.chartjs_title.is_none());
    assert!(spec.chartjs_subtitle.is_none());

    let scene = render_scene(&spec);
    assert!(
        !scene
            .items
            .iter()
            .any(|item| matches!(item, Prim::Group { .. }))
    );
    assert!(render_chart(&spec).contains("Legacy title"));
}

#[test]
fn chartjs_title_and_subtitle_render_for_every_chart_kind() {
    let cases = [
        ("bar", include_str!("../../../examples/specs/bar.json")),
        ("line", include_str!("../../../examples/specs/line.json")),
        ("pie", include_str!("../../../examples/specs/pie.json")),
        (
            "doughnut",
            include_str!("../../../examples/specs/doughnut.json"),
        ),
        (
            "scatter",
            include_str!("../../../examples/specs/scatter.json"),
        ),
        (
            "bubble",
            include_str!("../../../examples/specs/bubble.json"),
        ),
        ("radar", include_str!("../../../examples/specs/radar.json")),
        (
            "matrix",
            include_str!("../../../examples/specs/matrix.json"),
        ),
        (
            "treemap",
            include_str!("../../../examples/specs/treemap.json"),
        ),
        (
            "progress",
            include_str!("../../../examples/specs/progress.json"),
        ),
        (
            "progressBar",
            include_str!("../../../examples/specs/progress.json"),
        ),
        (
            "boxplot",
            include_str!("../../../examples/specs/boxplot_with_null.json"),
        ),
        (
            "violin",
            include_str!("../../../examples/specs/violin.json"),
        ),
        (
            "horizontalViolin",
            include_str!("../../../examples/specs/violin-horizontal.json"),
        ),
        (
            "sparkline",
            include_str!("../../../examples/specs/sparkline_decimated.json"),
        ),
        ("gauge", include_str!("../../../examples/specs/gauge.json")),
        (
            "polarArea",
            include_str!("../../../examples/specs/pie.json"),
        ),
        (
            "radialGauge",
            include_str!("../../../examples/specs/radial-gauge.json"),
        ),
        (
            "outlabeledPie",
            include_str!("../../../examples/specs/outlabeled_pie.json"),
        ),
        (
            "outlabeledDoughnut",
            include_str!("../../../examples/specs/outlabeled_doughnut.json"),
        ),
        (
            "wordCloud",
            include_str!("../../../examples/specs/wordcloud.json"),
        ),
        (
            "sankey",
            include_str!("../../../examples/specs/sankey.json"),
        ),
    ];

    let measurer = TextMeasurer::new(DEFAULT_FONT).unwrap();
    for (kind, fixture) in cases {
        let mut input: Value = serde_json::from_str(fixture).unwrap();
        let root = input.as_object_mut().expect("chart fixture object");
        root.insert("type".into(), Value::String(kind.into()));
        let options = root
            .entry("options")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("chart options object");
        let plugins = options
            .entry("plugins")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("chart plugins object");
        plugins.insert(
            "title".into(),
            json!({
                "display":true,"text":[format!("Primary {kind}"),"Second line"],
                "position":"top","align":"start","font":{"size":20,"lineHeight":1.2},"padding":0
            }),
        );
        plugins.insert(
            "subtitle".into(),
            json!({
                "display":true,"text":["Secondary one","Secondary two"],
                "position":"right","align":"end","font":{"size":10,"lineHeight":1.2},"padding":0
            }),
        );

        let spec = chartjs::parse(&input.to_string(), false)
            .unwrap_or_else(|error| panic!("{kind} parser rejected title options: {error}"));
        let scene = build_scene(&spec, &measurer);
        assert_eq!(
            (scene.width, scene.height),
            (spec.width, spec.height),
            "{kind}"
        );

        let expected_title_height = 48.0;
        let expected_subtitle_width = 24.0;
        let Prim::Group {
            translate_x,
            translate_y,
            clip,
            children,
        } = &scene.items[0]
        else {
            panic!("{kind} chart must be composed as the first group");
        };
        assert_eq!(
            (*translate_x, *translate_y),
            (0.0, expected_title_height),
            "{kind}"
        );
        assert_eq!(
            clip.as_deref(),
            Some(&fulgur_chart::scene::ClipRect {
                x: 0.0,
                y: 0.0,
                w: spec.width - expected_subtitle_width,
                h: spec.height - expected_title_height,
            }),
            "{kind} chart viewport clip"
        );

        let mut child_spec = spec.clone();
        child_spec.width -= expected_subtitle_width;
        child_spec.height -= expected_title_height;
        child_spec.chartjs_title = None;
        child_spec.chartjs_subtitle = None;
        assert_eq!(
            children,
            &build_scene(&child_spec, &measurer).items,
            "{kind} chart primitive order or content changed inside the shared title wrapper"
        );
        assert_eq!(
            group_texts(&scene.items[1]),
            vec![format!("Primary {kind}"), "Second line".to_string()],
            "{kind}"
        );
        assert_eq!(
            group_texts(&scene.items[2]),
            ["Secondary one".to_string(), "Secondary two".to_string()],
            "{kind}"
        );
        assert_eq!(
            scene.items[2].group_translation_x(),
            Some(spec.width - expected_subtitle_width)
        );
    }
}

#[test]
fn chartjs_title_options_render_in_svg_and_png() {
    for kind in ["bar", "pie"] {
        let json = format!(
            r##"{{
                "type":"{kind}",
                "data":{{"labels":["A","B","C"],"datasets":[{{"data":[3,2,1]}}]}},
                "options":{{"plugins":{{
                    "title":{{"display":true,"text":["Primary title","Second line"],"align":"start","position":"top","color":"#ff0000","font":{{"size":20,"family":"Inter","weight":"700","style":"italic","lineHeight":1.2}},"padding":0}},
                    "subtitle":{{"display":true,"text":"Rotated subtitle","align":"start","position":"left","color":"#00aa00","font":{{"size":18,"family":"Fira Sans","weight":"600","style":"italic","lineHeight":1.2}},"padding":0}}
                }}}}
            }}"##
        );
        let spec = chartjs::parse(&json, false).unwrap();
        let svg = render_chart(&spec);
        assert!(
            svg.contains("font-family=\"Inter\""),
            "{kind} SVG should retain the title family"
        );
        assert!(
            svg.contains("font-weight=\"700\""),
            "{kind} SVG should retain the title weight"
        );
        assert!(
            svg.contains("font-style=\"italic\""),
            "{kind} SVG should retain the title style"
        );
        assert!(
            svg.contains("fill=\"#ff0000\""),
            "{kind} SVG should retain the title color"
        );
        assert!(
            svg.contains("font-family=\"Fira Sans\""),
            "{kind} SVG should retain the subtitle family"
        );
        assert!(
            svg.contains("transform=\"rotate(-90"),
            "{kind} SVG should rotate the side subtitle"
        );
        assert!(
            svg.contains("fill=\"#00aa00\""),
            "{kind} SVG should retain the subtitle color"
        );

        let png = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("title PNG should decode");
        let width = pixmap.width() as usize;
        let (red_top_pixels, green_side_pixels) = pixmap.data().chunks_exact(4).enumerate().fold(
            (0usize, 0usize),
            |(red, green), (index, rgba)| {
                let x = index % width;
                let y = index / width;
                let is_title_red = y < 60 && rgba[0] > 180 && rgba[1] < 80 && rgba[2] < 80;
                let is_subtitle_green = x < 30 && rgba[0] < 80 && rgba[1] > 100 && rgba[2] < 80;
                (
                    red + usize::from(is_title_red),
                    green + usize::from(is_subtitle_green),
                )
            },
        );
        assert!(
            red_top_pixels > 0,
            "{kind} PNG should paint red title pixels in the reserved top band"
        );
        assert!(
            green_side_pixels > 0,
            "{kind} PNG should paint green subtitle pixels in the reserved side band"
        );
    }
}

#[test]
fn chartjs_title_subtitle_example_parses_and_renders() {
    let spec = chartjs::parse(
        include_str!("../../../examples/specs/chartjs_title_subtitle.json"),
        true,
    )
    .expect("title/subtitle example passes strict parsing");
    let title = spec.chartjs_title.as_ref().expect("example title");
    let subtitle = spec.chartjs_subtitle.as_ref().expect("example subtitle");
    assert_eq!(title.text, ["Quarterly revenue", "North America"]);
    assert_eq!(subtitle.text, ["Millions of USD"]);
    assert!(
        build_scene(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
            .items
            .len()
            >= 3
    );
}

fn group_texts(prim: &Prim) -> Vec<String> {
    let Prim::Group { children, .. } = prim else {
        panic!("expected a clipped text group");
    };
    children
        .iter()
        .map(|child| match child {
            Prim::StyledText(text) => text.content.clone(),
            _ => panic!("title group may contain only styled text"),
        })
        .collect()
}

trait GroupTranslation {
    fn group_translation_x(&self) -> Option<f64>;
}

impl GroupTranslation for Prim {
    fn group_translation_x(&self) -> Option<f64> {
        match self {
            Prim::Group { translate_x, .. } => Some(*translate_x),
            _ => None,
        }
    }
}
