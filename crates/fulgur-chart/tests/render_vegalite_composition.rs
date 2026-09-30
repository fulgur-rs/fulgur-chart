use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::{ChartKind, VegaCompositionNode};
use fulgur_chart::scene::{Prim, Scene};

fn parsed(json: &str) -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(json, false).expect("composition parses")
}

fn parsed_example(path: &str) -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(path, true).expect("composition example parses")
}

fn collect_groups(items: &[Prim], output: &mut Vec<(f64, f64, bool)>) {
    for item in items {
        if let Prim::Group {
            translate_x,
            translate_y,
            clip,
            children,
        } = item
        {
            output.push((*translate_x, *translate_y, clip.is_some()));
            collect_groups(children, output);
        }
    }
}

fn count_text(items: &[Prim], content: &str) -> usize {
    items
        .iter()
        .map(|item| match item {
            Prim::Text { content: text, .. } if text == content => 1,
            Prim::StyledText(text) if text.content == content => 1,
            Prim::Group { children, .. } => count_text(children, content),
            _ => 0,
        })
        .sum()
}

fn count_filled_rects(items: &[Prim], fill: fulgur_chart::ir::Color) -> usize {
    items
        .iter()
        .map(|item| match item {
            Prim::Rect { fill: color, .. } if *color == fill => 1,
            Prim::Group { children, .. } => count_filled_rects(children, fill),
            _ => 0,
        })
        .sum()
}

fn count_lines_with_stroke(items: &[Prim], stroke: fulgur_chart::ir::Color) -> usize {
    items
        .iter()
        .map(|item| match item {
            Prim::Line { stroke: color, .. } if *color == stroke => 1,
            Prim::Group { children, .. } => count_lines_with_stroke(children, stroke),
            _ => 0,
        })
        .sum()
}

fn count_circles(items: &[Prim]) -> usize {
    items
        .iter()
        .map(|item| match item {
            Prim::Circle { .. } | Prim::ClippedCircle { .. } => 1,
            Prim::Group { children, .. } => count_circles(children),
            _ => 0,
        })
        .sum()
}

fn size_legend_groups<'a>(items: &'a [Prim], title: &str) -> Vec<&'a [Prim]> {
    let mut groups = Vec::new();
    for item in items {
        if let Prim::Group {
            clip: Some(_),
            children,
            ..
        } = item
        {
            let has_title = children.iter().any(|child| match child {
                Prim::Text { content, .. } => content == title,
                Prim::StyledText(text) => text.content == title,
                _ => false,
            });
            if has_title {
                groups.push(children.as_slice());
            }
            groups.extend(size_legend_groups(children, title));
        }
    }
    groups
}

fn collect_text_positions(items: &[Prim], content: &str, output: &mut Vec<(f64, f64)>) {
    for item in items {
        match item {
            Prim::Text {
                x,
                y,
                content: text,
                ..
            } if text == content => output.push((*x, *y)),
            Prim::StyledText(text) if text.content == content => output.push((text.x, text.y)),
            Prim::Group {
                translate_x,
                translate_y,
                children,
                ..
            } => {
                let mut nested = Vec::new();
                collect_text_positions(children, content, &mut nested);
                output.extend(
                    nested
                        .into_iter()
                        .map(|(x, y)| (x + translate_x, y + translate_y)),
                );
            }
            _ => {}
        }
    }
}

fn count_clipped_mark_groups(items: &[Prim]) -> usize {
    items
        .iter()
        .map(|item| match item {
            Prim::Group {
                clip: Some(_),
                children,
                ..
            } if children.iter().any(|child| {
                matches!(
                    child,
                    Prim::ClippedPath { .. }
                        | Prim::ClippedCircle { .. }
                        | Prim::Polyline { .. }
                        | Prim::StyledPolyline { .. }
                        | Prim::Circle { .. }
                )
            }) =>
            {
                1
            }
            Prim::Group { children, .. } => count_clipped_mark_groups(children),
            _ => 0,
        })
        .sum()
}

fn assert_scene_dimensions(spec: &fulgur_chart::ir::ChartSpec, scene: &Scene) {
    assert!((spec.width - scene.width).abs() < 0.001);
    assert!((spec.height - scene.height).abs() < 0.001);
}

#[test]
fn layer_bar_line_keeps_paint_order_and_shared_frame() {
    let spec = parsed(
        r##"{
          "data":{"values":[{"x":"A","bar":2,"line":4},{"x":"B","bar":5,"line":3}]},
          "encoding":{"x":{"field":"x","type":"nominal"}},
          "layer":[
            {"mark":"bar","encoding":{"y":{"field":"bar","type":"quantitative"}}},
            {"mark":{"type":"line","point":true},"encoding":{"y":{"field":"line","type":"quantitative"}}}
          ]
        }"##,
    );
    let ChartKind::VegaComposition(root) = &spec.kind else {
        panic!("composition chart kind expected");
    };
    let VegaCompositionNode::Layer(layer) = root.as_ref() else {
        panic!("layer node expected");
    };
    let VegaCompositionNode::Unit(bar) = &layer.children[0] else {
        panic!("first child must be a unit");
    };
    let VegaCompositionNode::Unit(line) = &layer.children[1] else {
        panic!("second child must be a unit");
    };
    assert!(matches!(&bar.spec.kind, ChartKind::Bar { .. }));
    assert!(matches!(&line.spec.kind, ChartKind::Line { .. }));

    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("layer scene builds");
    assert_scene_dimensions(&spec, &scene);
    let mut groups = Vec::new();
    collect_groups(&scene.items, &mut groups);
    assert!(
        groups.iter().any(|(_, _, clipped)| *clipped),
        "each layer mark must retain the common plot clip: {groups:?}"
    );
    let bar = scene.items.iter().position(|item| matches!(
        item,
        Prim::Group { children, .. }
            if children.iter().any(|child| matches!(child, Prim::ClippedPath { fill: Some(_), .. }))
    )).expect("bar marks are a grouped layer contribution");
    let line = scene.items.iter().position(|item| matches!(
        item,
        Prim::Group { children, .. }
            if children.iter().any(|child| matches!(child, Prim::Polyline { .. } | Prim::StyledPolyline { .. }))
    )).expect("line marks are a grouped layer contribution");
    assert!(
        bar < line,
        "earlier layer mark must paint before later line mark"
    );
}

#[test]
fn layer_keeps_titles_from_every_unit_child() {
    let spec = parsed(
        r##"{
          "layer":[
            {"mark":"bar","title":"First unit","data":{"values":[{"x":"A","y":2}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}},
            {"mark":"line","title":"Second unit","data":{"values":[{"x":"A","y":3}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}}
          ]
        }"##,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("layer titles render");

    assert_eq!(count_text(&scene.items, "First unit"), 1);
    assert_eq!(count_text(&scene.items, "Second unit"), 1);
}

#[test]
fn layer_independent_axes_get_separate_gutters_and_keep_marks_clipped() {
    let spec = parsed(
        r#"{
          "data":{"values":[{"x":"A","left":2,"right":8},{"x":"B","left":5,"right":3}]},
          "layer":[
            {"mark":"line","encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"left","type":"quantitative"}}},
            {"mark":"line","encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"right","type":"quantitative"}}}
          ],
          "resolve":{"scale":{"x":"independent","y":"independent"},"axis":{"x":"independent","y":"independent"}}
        }"#,
    );
    let measurer = fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = fulgur_chart::layout::build_scene_checked(&spec, &measurer).expect("layout builds");
    assert_eq!(scene.width, spec.width);
    assert_eq!(
        scene.width, 836.0,
        "independent y axes reserve one 36 px gutter"
    );
    assert_eq!(
        scene.height, 486.0,
        "independent x axes reserve a top gutter"
    );
    let mut groups = Vec::new();
    collect_groups(&scene.items, &mut groups);
    let clipped = count_clipped_mark_groups(&scene.items);
    assert_eq!(
        clipped, 2,
        "each independently scaled layer mark keeps its clip: {groups:?}"
    );
    let y_axis_positions = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Group { translate_x, .. } if *translate_x != 0.0 => Some(*translate_x),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        y_axis_positions.iter().any(|x| *x > 0.0),
        "right independent axis gets an outer gutter: {y_axis_positions:?}"
    );
    let mut x_labels = Vec::new();
    collect_text_positions(&scene.items, "A", &mut x_labels);
    assert_eq!(
        x_labels.len(),
        2,
        "each independent x axis contributes labels"
    );
    assert!(
        x_labels[1].1 < 80.0,
        "second independent x axis moves above the plot: {x_labels:?}"
    );
}

#[test]
fn layer_shared_domains_align_child_marks_on_the_same_frame() {
    let spec = parsed(
        r#"{
          "layer":[
            {"mark":"bar","data":{"values":[{"x":"A","y":2},{"x":"B","y":10}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}},
            {"mark":{"type":"line","point":true},"data":{"values":[{"x":"B","y":2}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}}
          ]
        }"#,
    );
    let VegaCompositionNode::Layer(layer) = (match &spec.kind {
        ChartKind::VegaComposition(root) => root.as_ref(),
        _ => panic!("composition root expected"),
    }) else {
        panic!("layer node expected");
    };
    let VegaCompositionNode::Unit(line) = &layer.children[1] else {
        panic!("line leaf expected");
    };
    let measurer = fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
    let mut resolved_line = (*line.spec).clone();
    resolved_line.size_mode = fulgur_chart::ir::SizeMode::Canvas;
    resolved_line.categories = vec!["A".into(), "B".into()];
    resolved_line.series[0].values = vec![f64::NAN, 2.0];
    resolved_line.y_axis.min = Some(2.0);
    resolved_line.y_axis.max = Some(10.0);
    let frame = fulgur_chart::layout::common::compute(&resolved_line, &measurer);
    let scene = fulgur_chart::layout::build_scene_checked(&spec, &measurer).expect("layer builds");
    let mut circles = Vec::new();
    fn collect_circles(items: &[Prim], circles: &mut Vec<(f64, f64)>) {
        for item in items {
            match item {
                Prim::Circle { cx, cy, .. } | Prim::ClippedCircle { cx, cy, .. } => {
                    circles.push((*cx, *cy))
                }
                Prim::Group { children, .. } => collect_circles(children, circles),
                _ => {}
            }
        }
    }
    collect_circles(&scene.items, &mut circles);
    assert_eq!(
        circles.len(),
        1,
        "the line's single data point remains at category B"
    );
    assert!(
        circles[0].0 > frame.plot_left + (frame.plot_right - frame.plot_left) * 0.75,
        "shared categorical x domain places B in the second slot: {:?}",
        circles[0]
    );
    assert!(
        circles[0].1 > frame.plot_top + (frame.plot_bottom - frame.plot_top) * 0.75,
        "shared numeric y domain places value 2 near the lower end: {:?}",
        circles[0]
    );
}

#[test]
fn concat_nodes_report_derived_dimensions_and_order() {
    let spec = parsed(
        r#"{
          "width":160,"height":100,"spacing":12,
          "hconcat":[
            {"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}},
            {"mark":"line","data":{"values":[{"x":"B","y":2}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}
          ]
        }"#,
    );
    let ChartKind::VegaComposition(root) = &spec.kind else {
        panic!("composition root expected")
    };
    let VegaCompositionNode::HConcat(concat) = root.as_ref() else {
        panic!("hconcat expected")
    };
    assert_eq!(
        (concat.width, concat.height, concat.spacing),
        (332.0, 100.0, 12.0)
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("concat scene builds");
    assert_eq!((scene.width, scene.height), (332.0, 100.0));
    let mut groups = Vec::new();
    collect_groups(&scene.items, &mut groups);
    assert!(
        groups.iter().any(|(x, _, _)| (*x - 172.0).abs() < 0.001),
        "second view follows the first view and gap: {groups:?}"
    );
}

#[test]
fn concat_shared_legends_render_once() {
    let spec = parsed(
        r#"{
          "hconcat":[
            {"mark":{"type":"line","point":true},"data":{"values":[{"x":"A","y":1,"group":"north"},{"x":"B","y":3,"group":"north"}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"}}},
            {"mark":{"type":"line","point":true},"data":{"values":[{"x":"A","y":4,"group":"north"},{"x":"B","y":2,"group":"north"}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"}}}
          ]
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("concat scene builds");
    assert_eq!(
        count_text(&scene.items, "north"),
        1,
        "shared color legend is emitted once"
    );
    let child_groups = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Group { children, .. } => Some(children.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(child_groups.len(), 2);
    assert_eq!(
        count_circles(child_groups[1]),
        2,
        "legend merging keeps both data point markers"
    );

    let nested = parsed(
        r#"{
          "hconcat":[
            {"mark":"line","data":{"values":[{"x":"A","y":1,"group":"North"}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"},"color":{"field":"group"}}},
            {"layer":[{"mark":"bar","encoding":{"y":{"field":"bar"}}},{"mark":"line","encoding":{"y":{"field":"line"}}}],"data":{"values":[{"x":"A","bar":2,"line":3,"group":"North"}]},"encoding":{"x":{"field":"x"},"color":{"field":"group"}}}
          ]
        }"#,
    );
    let nested_scene = fulgur_chart::layout::build_scene_checked(
        &nested,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("nested concat scene builds");
    assert_eq!(
        count_text(&nested_scene.items, "North"),
        1,
        "a nested layer's shared legend is merged into its parent concat"
    );
}

#[test]
fn concat_shared_legends_keep_the_first_child_that_has_each_guide() {
    let spec = parsed(
        r#"{
          "hconcat":[
            {"mark":"point","data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"}}},
            {"mark":"point","data":{"values":[{"x":3,"y":4,"group":"north","amount":12}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"},"size":{"field":"amount","type":"quantitative"}}}
          ]
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("concat scene builds");

    assert_eq!(count_text(&scene.items, "north"), 1);
    assert_eq!(size_legend_groups(&scene.items, "amount").len(), 1);
}

#[test]
fn layer_shared_legends_keep_the_first_child_that_has_each_guide() {
    let spec = parsed(
        r#"{
          "data":{"values":[{"x":1,"y":2,"group":"north","amount":12}]},
          "layer":[
            {"mark":"point","encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"}}},
            {"mark":"point","encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"},"size":{"field":"amount","type":"quantitative"}}}
          ]
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("layer scene builds");

    assert_eq!(count_text(&scene.items, "north"), 1);
    assert_eq!(size_legend_groups(&scene.items, "amount").len(), 1);
}

#[test]
fn layer_keeps_errorbar_marks_from_later_units() {
    let spec = parsed(
        r#"{
          "layer":[
            {"mark":"bar","data":{"values":[{"x":"A","y":2}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}},
            {"mark":{"type":"errorbar","color":"red"},"data":{"values":[{"x":"A","lo":2,"hi":8}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}
          ]
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("layer scene builds");
    let red = fulgur_chart::color::parse_color("red").unwrap();
    assert!(
        count_lines_with_stroke(&scene.items, red) > 0,
        "the later errorbar rule must remain in the composed scene"
    );
}

#[test]
fn layer_keeps_boxplot_marks_from_later_units() {
    let boxplot = parsed(
        r#"{"mark":{"type":"boxplot","color":"red"},"data":{"values":[{"x":"A","y":1},{"x":"A","y":3},{"x":"A","y":5},{"x":"A","y":7},{"x":"A","y":9}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}}"#,
    );
    let spec = parsed(
        r#"{
          "layer":[
            {"mark":"bar","data":{"values":[{"x":"A","y":2}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}},
            {"mark":{"type":"boxplot","color":"red"},"data":{"values":[{"x":"A","y":1},{"x":"A","y":3},{"x":"A","y":5},{"x":"A","y":7},{"x":"A","y":9}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}}
          ]
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("layer scene builds");
    let boxplot_scene = fulgur_chart::layout::build_scene_checked(
        &boxplot,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("standalone boxplot scene builds");
    let red = fulgur_chart::color::parse_color("red").unwrap();

    assert!(
        count_filled_rects(&boxplot_scene.items, red) > 0,
        "standalone boxplot produces a red box"
    );
    assert_eq!(
        count_filled_rects(&scene.items, red),
        count_filled_rects(&boxplot_scene.items, red),
        "the later boxplot box must remain in the composed scene"
    );
}

#[test]
fn concat_resolves_color_and_size_legends_independently() {
    let spec = parsed(
        r#"{
          "hconcat":[
            {"mark":"point","data":{"values":[{"x":1,"y":2,"group":"north","size":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"},"size":{"field":"size","type":"quantitative"}}},
            {"mark":"point","data":{"values":[{"x":3,"y":4,"group":"south","size":20}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"},"size":{"field":"size","type":"quantitative"}}}
          ],
          "resolve":{"legend":{"color":"independent","size":"shared"}}
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("independent color and shared size guides build");

    assert_eq!(count_text(&scene.items, "north"), 1);
    assert_eq!(count_text(&scene.items, "south"), 1);
    let guides = size_legend_groups(&scene.items, "size");
    assert_eq!(guides.len(), 1, "shared size guide appears once");
    assert_eq!(count_text(guides[0], "2"), 1);
    assert_eq!(count_text(guides[0], "20"), 1);
    let radii = guides[0]
        .iter()
        .filter_map(|item| match item {
            Prim::Circle { r, .. } => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        radii.len(),
        3,
        "the guide shows minimum, midpoint, and maximum"
    );
    assert!(radii[0] < radii[1] && radii[1] < radii[2]);
}

#[test]
fn layer_keeps_independent_color_legends_from_each_unit() {
    let spec = parsed(
        r#"{
          "layer":[
            {"mark":"point","data":{"values":[{"x":1,"y":2,"group":"north"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"}}},
            {"mark":"point","data":{"values":[{"x":3,"y":4,"group":"south"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"}}}
          ],
          "resolve":{"legend":{"color":"independent"}}
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("independent layer legends build");

    assert_eq!(count_text(&scene.items, "north"), 1);
    assert_eq!(count_text(&scene.items, "south"), 1);
}

#[test]
fn layer_resolves_legends_from_nested_layer_children() {
    let spec = parsed(
        r#"{
          "layer":[
            {"layer":[{"mark":"point","data":{"values":[{"x":1,"y":2,"group":"north"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"}}}]},
            {"layer":[{"mark":"point","data":{"values":[{"x":3,"y":4,"group":"south"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"}}}]}
          ]
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("nested layer children build");

    assert_eq!(count_text(&scene.items, "north"), 1);
    assert_eq!(count_text(&scene.items, "south"), 0);
}

#[test]
fn layer_keeps_independent_size_legends_from_each_unit() {
    let spec = parsed(
        r#"{
          "data":{"values":[{"x":1,"y":2,"first":2,"second":8},{"x":3,"y":4,"first":4,"second":10}]},
          "layer":[
            {"mark":"point","encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"size":{"field":"first","type":"quantitative","title":"first size"}}},
            {"mark":"point","encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"size":{"field":"second","type":"quantitative","title":"second size"}}}
          ],
          "resolve":{"scale":{"size":"independent"},"legend":{"size":"independent"}}
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("independent layer size guides build");

    assert_eq!(size_legend_groups(&scene.items, "first size").len(), 1);
    assert_eq!(size_legend_groups(&scene.items, "second size").len(), 1);
}

#[test]
fn concat_resolves_shared_color_and_independent_size_legends() {
    let spec = parsed(
        r#"{
          "hconcat":[
            {"mark":"point","data":{"values":[{"x":1,"y":2,"group":"shared","size":2},{"x":2,"y":3,"group":"shared","size":4}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"},"size":{"field":"size","type":"quantitative"}}},
            {"mark":"point","data":{"values":[{"x":3,"y":4,"group":"shared","size":6},{"x":4,"y":5,"group":"shared","size":8}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"color":{"field":"group","type":"nominal"},"size":{"field":"size","type":"quantitative"}}}
          ],
          "resolve":{"scale":{"size":"independent"},"legend":{"color":"shared","size":"independent"}}
        }"#,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("shared color and independent size guides build");

    assert_eq!(count_text(&scene.items, "shared"), 1);
    let guides = size_legend_groups(&scene.items, "size");
    assert_eq!(guides.len(), 2, "independent size guides remain per view");
    assert_eq!(count_text(guides[0], "2"), 1);
    assert_eq!(count_text(guides[0], "4"), 1);
    assert_eq!(count_text(guides[1], "6"), 1);
    assert_eq!(count_text(guides[1], "8"), 1);
}

#[test]
fn nested_concat_contains_layer_and_vconcat_groups() {
    let spec = parsed(
        r#"{
          "vconcat":[
            {"layer":[{"mark":"bar","encoding":{"x":{"field":"x"},"y":{"field":"y"}}},{"mark":"line","encoding":{"x":{"field":"x"},"y":{"field":"y"}}}],"data":{"values":[{"x":"A","y":2}]}},
            {"hconcat":[{"mark":"point","data":{"values":[{"x":1,"y":2}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"}}}]}
          ]
        }"#,
    );
    let ChartKind::VegaComposition(root) = &spec.kind else {
        panic!("composition root expected")
    };
    let VegaCompositionNode::VConcat(vconcat) = root.as_ref() else {
        panic!("vconcat expected")
    };
    assert!(matches!(vconcat.children[0], VegaCompositionNode::Layer(_)));
    assert!(matches!(
        vconcat.children[1],
        VegaCompositionNode::HConcat(_)
    ));
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("nested scene builds");
    let mut groups = Vec::new();
    collect_groups(&scene.items, &mut groups);
    assert!(
        groups.len() >= 4,
        "nested nodes retain translated groups: {groups:?}"
    );
}

#[test]
fn composition_titles_and_background_render_once() {
    let spec = parsed(
        r##"{
          "title":"Composition title","background":"#fafafa","spacing":8,
          "hconcat":[
            {"title":"First view","mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}},
            {"title":"Second view","mark":"bar","data":{"values":[{"x":"B","y":2}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}
          ]
        }"##,
    );
    let scene = fulgur_chart::layout::build_scene_checked(
        &spec,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("titled scene builds");
    assert_eq!(count_text(&scene.items, "Composition title"), 1);
    assert_eq!(count_text(&scene.items, "First view"), 1);
    assert_eq!(count_text(&scene.items, "Second view"), 1);
    let background = fulgur_chart::color::parse_color("#fafafa").unwrap();
    assert_eq!(count_filled_rects(&scene.items, background), 1);
    assert_eq!(scene.height, spec.height);
    assert_eq!(
        scene.height, 450.0,
        "composition title reserves the default height"
    );
    let mut category_labels = Vec::new();
    collect_text_positions(&scene.items, "A", &mut category_labels);
    assert_eq!(category_labels.len(), 1);
    assert!(
        category_labels[0].1 < scene.height && category_labels[0].1 > 400.0,
        "root title must not clip the child's bottom axis labels: {category_labels:?}"
    );

    let layer = parsed(
        r#"{
          "title":"Layer title",
          "layer":[
            {"mark":"bar","data":{"values":[{"x":"A","y":1}]}},
            {"mark":"line","data":{"values":[{"x":"A","y":2}]}}
          ],
          "encoding":{"x":{"field":"x"},"y":{"field":"y"}}
        }"#,
    );
    let layer_scene = fulgur_chart::layout::build_scene_checked(
        &layer,
        &fulgur_chart::text::TextMeasurer::new(DEFAULT_FONT).unwrap(),
    )
    .expect("titled layer builds");
    assert_eq!(layer_scene.height, layer.height);
    let mut layer_labels = Vec::new();
    collect_text_positions(&layer_scene.items, "A", &mut layer_labels);
    assert_eq!(layer_labels.len(), 1);
    assert!(layer_labels[0].1 < layer_scene.height);
}

#[test]
fn composition_with_image_keeps_svg_reference_and_rejects_raster() {
    let spec = parsed(
        r#"{
          "hconcat":[
            {"mark":{"type":"image","width":12,"height":8},"data":{"values":[{"x":1,"y":2,"src":"https://example.test/a.png"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"url":{"field":"src"}}},
            {"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}
          ]
        }"#,
    );
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(
        svg.contains("<image"),
        "SVG retains nested external image: {svg}"
    );
    let png_error =
        fulgur_chart::raster_direct::render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap_err();
    assert!(
        png_error.contains("image marks") && png_error.contains("SVG"),
        "{png_error}"
    );
    let webp_error =
        fulgur_chart::raster_direct::render_chart_to_webp(&spec, 1.0, DEFAULT_FONT).unwrap_err();
    assert!(
        webp_error.contains("image marks") && webp_error.contains("SVG"),
        "{webp_error}"
    );
}

#[test]
fn vegalite_layer_example_snapshot() {
    let spec = parsed_example(include_str!("../../../examples/specs/vegalite-layer.json"));
    insta::assert_snapshot!(fulgur_chart::render::render_chart(&spec));
}

#[test]
fn vegalite_nested_concat_example_snapshot() {
    let spec = parsed_example(include_str!(
        "../../../examples/specs/vegalite-nested-concat.json"
    ));
    insta::assert_snapshot!(fulgur_chart::render::render_chart(&spec));
}
