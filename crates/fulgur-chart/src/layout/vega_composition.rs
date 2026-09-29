//! Scene composition for recursive Vega-Lite layer and concat views.

use crate::guard::InputLimits;
use crate::ir::{ChartKind, ChartSpec, Color, VegaCompositionNode};
use crate::scene::{Anchor, ClipRect, Prim, Scene};
use crate::text::TextMeasurer;

const COMPOSITION_TITLE_BAND: f64 = crate::layout::common::TITLE_BAND;

#[derive(Clone, Debug)]
struct VegaNodeLayout {
    width: f64,
    height: f64,
    items: Vec<Prim>,
}

pub(crate) fn build_checked(
    spec: &ChartSpec,
    measurer: &TextMeasurer<'_>,
    limits: &InputLimits,
) -> Result<Scene, String> {
    let ChartKind::VegaComposition(root) = &spec.kind else {
        return Err("Vega-Lite composition layout requires a composition ChartKind".into());
    };
    crate::guard::validate_vega_composition(spec, limits)?;
    let layout = build_node(root, measurer, limits)?;
    Ok(Scene {
        width: layout.width,
        height: layout.height,
        items: layout.items,
    })
}

fn build_node(
    node: &VegaCompositionNode,
    measurer: &TextMeasurer<'_>,
    limits: &InputLimits,
) -> Result<VegaNodeLayout, String> {
    match node {
        VegaCompositionNode::Unit(leaf) => {
            let mut leaf_spec = (*leaf.spec).clone();
            // Composition dimensions describe the complete view cell. Existing temporal
            // standalone charts use PlotArea sizing, so normalize composition leaves to the
            // same explicit cell dimensions before sharing their plot frame.
            leaf_spec.size_mode = crate::ir::SizeMode::Canvas;
            apply_leaf_domains(&mut leaf_spec, &leaf.scales);
            let scene =
                crate::layout::build_scene_checked_with_limits(&leaf_spec, measurer, limits)
                    .map_err(|error| format!("{}: {error}", leaf.path))?;
            Ok(VegaNodeLayout {
                width: scene.width,
                height: scene.height,
                items: scene.items,
            })
        }
        VegaCompositionNode::Layer(layer) => {
            let mut children = Vec::with_capacity(layer.children.len());
            for child in &layer.children {
                children.push(build_node(child, measurer, limits)?);
            }
            let width = layer.width;
            let height = layer.height;
            let view_width = layer.view_width;
            let view_height = layer.view_height;
            for (index, child) in children.iter().enumerate() {
                if (child.width - view_width).abs() > f64::EPSILON
                    || (child.height - view_height).abs() > f64::EPSILON
                {
                    return Err(format!(
                        "{}layer child {index} has dimensions {}×{}, expected {}×{}",
                        path_prefix(&layer.path),
                        child.width,
                        child.height,
                        view_width,
                        view_height
                    ));
                }
            }
            let mut items = Vec::with_capacity(children.len() + 2);
            if let Some(background) = layer.background {
                items.push(background_rect(width, height, background));
            }
            let title_height = if layer.title.is_some() {
                COMPOSITION_TITLE_BAND
            } else {
                0.0
            };
            let x_axis_gutter = (height - view_height - title_height).max(0.0);
            let content_y = title_height + x_axis_gutter;
            let independent_y = layer.resolve.y_axis == crate::ir::VegaResolutionMode::Independent;
            let independent_x = layer.resolve.x_axis == crate::ir::VegaResolutionMode::Independent;
            let mut independent_y_guides = Vec::new();
            let mut independent_x_guides = Vec::new();
            for (index, (child_node, child)) in layer.children.iter().zip(children).enumerate() {
                if let VegaCompositionNode::Unit(leaf) = child_node {
                    let mut frame_spec = (*leaf.spec).clone();
                    frame_spec.size_mode = crate::ir::SizeMode::Canvas;
                    apply_leaf_domains(&mut frame_spec, &leaf.scales);
                    let frame = crate::layout::common::compute(&frame_spec, measurer);
                    let (marks, y_guides, x_guides) = split_layer_scene_items(
                        &child.items,
                        &leaf.spec.kind,
                        frame.plot_left,
                        frame.plot_bottom,
                    );
                    if index == 0 {
                        // Keep the first child's frame and guides, but defer its marks so all
                        // layer marks follow source order after the guides.
                        let guides = child
                            .items
                            .into_iter()
                            .filter(|item| !is_mark_primitive(item, &leaf.spec.kind))
                            .collect();
                        items.push(Prim::Group {
                            translate_x: 0.0,
                            translate_y: content_y,
                            clip: Some(Box::new(ClipRect {
                                x: 0.0,
                                y: 0.0,
                                w: view_width,
                                h: view_height,
                            })),
                            children: guides,
                        });
                    } else {
                        if independent_y {
                            independent_y_guides.push((index, y_guides));
                        }
                        if independent_x {
                            independent_x_guides.push((index, x_guides));
                        }
                    }
                    items.push(Prim::Group {
                        translate_x: 0.0,
                        translate_y: content_y,
                        clip: Some(Box::new(ClipRect {
                            x: 0.0,
                            y: 0.0,
                            w: view_width,
                            h: view_height,
                        })),
                        children: marks,
                    });
                } else if index == 0 {
                    items.push(Prim::Group {
                        translate_x: 0.0,
                        translate_y: content_y,
                        clip: Some(Box::new(ClipRect {
                            x: 0.0,
                            y: 0.0,
                            w: view_width,
                            h: view_height,
                        })),
                        children: child.items,
                    });
                } else {
                    items.push(Prim::Group {
                        translate_x: 0.0,
                        translate_y: title_height,
                        clip: Some(Box::new(ClipRect {
                            x: 0.0,
                            y: 0.0,
                            w: view_width,
                            h: view_height,
                        })),
                        children: child.items,
                    });
                }
            }
            for (index, mut guides) in independent_y_guides {
                if guides.is_empty() {
                    continue;
                }
                position_y_axis_guides(&mut guides, view_width);
                items.push(Prim::Group {
                    translate_x: index as f64 * 36.0,
                    translate_y: content_y,
                    clip: Some(Box::new(ClipRect {
                        x: 0.0,
                        y: 0.0,
                        w: view_width,
                        h: view_height,
                    })),
                    children: guides,
                });
            }
            for (index, mut guides) in independent_x_guides {
                if !guides.is_empty() {
                    position_x_axis_guides(&mut guides, index, title_height);
                    items.push(Prim::Group {
                        translate_x: 0.0,
                        translate_y: 0.0,
                        clip: Some(Box::new(ClipRect {
                            x: 0.0,
                            y: 0.0,
                            w: view_width,
                            h: (view_height - title_height).max(0.0),
                        })),
                        children: guides,
                    });
                }
            }
            if let Some(title) = &layer.title {
                items.push(title_primitive(title, width));
            }
            Ok(VegaNodeLayout {
                width,
                height,
                items,
            })
        }
        VegaCompositionNode::HConcat(concat) => build_concat_node(
            &concat.children,
            concat.width,
            concat.height,
            concat.spacing,
            concat.title.as_deref(),
            concat.background,
            concat.resolve.color_legend == crate::ir::VegaResolutionMode::Shared
                || concat.resolve.size_legend == crate::ir::VegaResolutionMode::Shared,
            true,
            measurer,
            limits,
        ),
        VegaCompositionNode::VConcat(concat) => build_concat_node(
            &concat.children,
            concat.width,
            concat.height,
            concat.spacing,
            concat.title.as_deref(),
            concat.background,
            concat.resolve.color_legend == crate::ir::VegaResolutionMode::Shared
                || concat.resolve.size_legend == crate::ir::VegaResolutionMode::Shared,
            false,
            measurer,
            limits,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_concat_node(
    nodes: &[VegaCompositionNode],
    width: f64,
    height: f64,
    spacing: f64,
    title: Option<&str>,
    background: Option<Color>,
    merge_shared_legend: bool,
    horizontal: bool,
    measurer: &TextMeasurer<'_>,
    limits: &InputLimits,
) -> Result<VegaNodeLayout, String> {
    let mut children = Vec::with_capacity(nodes.len());
    for node in nodes {
        children.push(build_node(node, measurer, limits)?);
    }
    let mut items = Vec::with_capacity(children.len() + 2);
    if let Some(background) = background {
        items.push(background_rect(width, height, background));
    }
    let title_height = if title.is_some() {
        COMPOSITION_TITLE_BAND
    } else {
        0.0
    };
    let mut x = 0.0;
    let mut y = title_height;
    for (index, (node, mut child)) in nodes.iter().zip(children).enumerate() {
        if merge_shared_legend && index > 0 {
            strip_node_legend(node, &mut child.items, measurer);
        }
        let (translate_x, translate_y) = if horizontal {
            (x, title_height)
        } else {
            (0.0, y)
        };
        items.push(Prim::Group {
            translate_x,
            translate_y,
            clip: Some(Box::new(ClipRect {
                x: 0.0,
                y: 0.0,
                w: child.width,
                h: child.height,
            })),
            children: child.items,
        });
        if horizontal {
            x += child.width + spacing;
        } else {
            y += child.height + spacing;
        }
    }
    if let Some(title) = title {
        items.push(title_primitive(title, width));
    }
    Ok(VegaNodeLayout {
        width,
        height,
        items,
    })
}

fn strip_node_legend(
    node: &VegaCompositionNode,
    items: &mut Vec<Prim>,
    measurer: &TextMeasurer<'_>,
) {
    if let VegaCompositionNode::Unit(leaf) = node {
        let mut spec = (*leaf.spec).clone();
        spec.size_mode = crate::ir::SizeMode::Canvas;
        apply_leaf_domains(&mut spec, &leaf.scales);
        let frame = crate::layout::common::compute(&spec, measurer);
        strip_leaf_legend(
            items,
            &spec,
            frame.plot_left,
            frame.plot_right,
            frame.plot_top,
            frame.plot_bottom,
        );
    }
}

fn strip_leaf_legend(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    plot_left: f64,
    plot_right: f64,
    plot_top: f64,
    plot_bottom: f64,
) {
    if spec.legend == crate::ir::LegendPos::None {
        return;
    }
    let labels = spec
        .series
        .iter()
        .filter(|series| !series.name.is_empty())
        .map(|series| series.name.as_str())
        .chain(crate::layout::common::legend_title(spec))
        .collect::<Vec<_>>();
    items.retain_mut(|item| {
        if let Prim::Group { children, .. } = item {
            strip_leaf_legend(children, spec, plot_left, plot_right, plot_top, plot_bottom);
            return !children.is_empty();
        }
        let (x, y) = prim_position(item);
        let in_legend_band = match spec.legend {
            crate::ir::LegendPos::Right => x >= plot_right + 2.0,
            crate::ir::LegendPos::Left => x <= plot_left - 2.0,
            crate::ir::LegendPos::Top => y <= plot_top - 2.0,
            crate::ir::LegendPos::Bottom => y >= plot_bottom + 2.0,
            crate::ir::LegendPos::None => false,
        };
        if !in_legend_band {
            return true;
        }
        let is_label = match item {
            Prim::Text { content, .. } => labels.contains(&content.as_str()),
            Prim::StyledText(text) => labels.contains(&text.content.as_str()),
            _ => false,
        };
        !is_label
            && !is_legend_marker(
                item,
                spec.legend,
                plot_left,
                plot_right,
                plot_top,
                plot_bottom,
            )
    });
}

fn prim_position(prim: &Prim) -> (f64, f64) {
    match prim {
        Prim::Text { x, y, .. } => (*x, *y),
        Prim::StyledText(text) => (text.x, text.y),
        Prim::Rect { x, y, w, h, .. } => (x + w / 2.0, y + h / 2.0),
        Prim::Circle { cx, cy, .. } | Prim::ClippedCircle { cx, cy, .. } => (*cx, *cy),
        Prim::Line { x1, y1, x2, y2, .. } => ((x1 + x2) / 2.0, (y1 + y2) / 2.0),
        Prim::Polyline { points, .. } | Prim::StyledPolyline { points, .. } => points
            .first()
            .copied()
            .unwrap_or((f64::NEG_INFINITY, f64::NEG_INFINITY)),
        _ => (f64::NEG_INFINITY, f64::NEG_INFINITY),
    }
}

fn is_legend_marker(
    prim: &Prim,
    position: crate::ir::LegendPos,
    plot_left: f64,
    plot_right: f64,
    plot_top: f64,
    plot_bottom: f64,
) -> bool {
    if !matches!(
        prim,
        Prim::Rect { .. }
            | Prim::Circle { .. }
            | Prim::Line { .. }
            | Prim::Path { .. }
            | Prim::Polyline { .. }
            | Prim::StyledPolyline { .. }
    ) {
        return false;
    }
    let (x, y) = prim_position(prim);
    match position {
        crate::ir::LegendPos::Right => x >= plot_right + 2.0,
        crate::ir::LegendPos::Left => x <= plot_left - 2.0,
        crate::ir::LegendPos::Top => y <= plot_top - 2.0,
        crate::ir::LegendPos::Bottom => y >= plot_bottom + 2.0,
        crate::ir::LegendPos::None => false,
    }
}

fn split_layer_scene_items(
    items: &[Prim],
    kind: &ChartKind,
    plot_left: f64,
    plot_bottom: f64,
) -> (Vec<Prim>, Vec<Prim>, Vec<Prim>) {
    let mut marks = Vec::new();
    let mut y_guides = Vec::new();
    let mut x_guides = Vec::new();
    for item in items {
        if is_mark_primitive(item, kind) {
            marks.push(item.clone());
        } else if let Prim::Text {
            x, y, rotate_deg, ..
        } = item
        {
            if *x < plot_left - 1.0 || rotate_deg.is_some_and(|degree| degree.abs() > 45.0) {
                y_guides.push(item.clone());
            } else if *y >= plot_bottom {
                x_guides.push(item.clone());
            }
        }
    }
    (marks, y_guides, x_guides)
}

fn is_mark_primitive(prim: &Prim, kind: &ChartKind) -> bool {
    match kind {
        ChartKind::Bar { .. } => matches!(prim, Prim::ClippedPath { fill: Some(_), .. }),
        ChartKind::Line { .. } | ChartKind::Trail => matches!(
            prim,
            Prim::Path { fill: Some(_), .. }
                | Prim::ClippedPath { fill: Some(_), .. }
                | Prim::Polyline { .. }
                | Prim::StyledPolyline { .. }
                | Prim::Circle { .. }
                | Prim::ClippedCircle { .. }
        ),
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square => {
            matches!(
                prim,
                Prim::Circle { .. }
                    | Prim::ClippedCircle { .. }
                    | Prim::Path { fill: Some(_), .. }
                    | Prim::ClippedPath { fill: Some(_), .. }
            )
        }
        ChartKind::VegaImage(_) => matches!(prim, Prim::Image { .. }),
        ChartKind::VegaRect { .. } => matches!(prim, Prim::Rect { .. }),
        ChartKind::ErrorMark(_) | ChartKind::VegaBoxPlot(_) => matches!(
            prim,
            Prim::ClippedPath { .. } | Prim::ClippedCircle { .. } | Prim::Circle { .. }
        ),
        _ => false,
    }
}

fn position_y_axis_guides(guides: &mut [Prim], width: f64) {
    for guide in guides {
        if let Prim::Text {
            x,
            anchor,
            rotate_deg,
            ..
        } = guide
        {
            *x = if rotate_deg.is_some() {
                width - 14.0
            } else {
                width - 34.0
            };
            *anchor = Anchor::Start;
            if rotate_deg.is_some() {
                *rotate_deg = Some(90.0);
            }
        }
    }
}

fn position_x_axis_guides(guides: &mut [Prim], index: usize, title_height: f64) {
    let y = crate::layout::common::OUTER_PAD
        + crate::layout::common::LABEL_FONT
        + title_height
        + (index.saturating_sub(1) as f64) * 18.0;
    for guide in guides {
        if let Prim::Text {
            y: label_y,
            rotate_deg,
            ..
        } = guide
            && rotate_deg.is_none()
        {
            *label_y = y;
        }
    }
}

fn apply_leaf_domains(spec: &mut ChartSpec, domains: &crate::ir::VegaLeafScaleDomains) {
    apply_axis_domain(spec, "x", domains.x.as_ref());
    apply_axis_domain(spec, "y", domains.y.as_ref());
}

fn apply_axis_domain(
    spec: &mut ChartSpec,
    channel: &str,
    domain: Option<&crate::ir::VegaScaleDomain>,
) {
    let Some(domain) = domain else { return };
    match domain {
        crate::ir::VegaScaleDomain::Numeric { min, max } => {
            let axis = if channel == "x" {
                &mut spec.x_axis
            } else {
                &mut spec.y_axis
            };
            axis.min = Some(*min);
            axis.max = Some(*max);
        }
        crate::ir::VegaScaleDomain::Temporal {
            min_millis,
            max_millis,
        } => {
            let axis = if channel == "x" {
                &mut spec.x_axis
            } else {
                &mut spec.y_axis
            };
            axis.min = Some(*min_millis as f64);
            axis.max = Some(*max_millis as f64);
        }
        crate::ir::VegaScaleDomain::Categories(categories) => {
            remap_categories(spec, channel, categories);
        }
    }
}

fn remap_categories(spec: &mut ChartSpec, channel: &str, domain: &[String]) {
    let category_axis = match &spec.kind {
        ChartKind::Bar {
            horizontal: true, ..
        } => channel == "y",
        ChartKind::Bar {
            horizontal: false, ..
        }
        | ChartKind::Line { .. }
        | ChartKind::Trail
        | ChartKind::Mixed => channel == "x",
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square => false,
        ChartKind::VegaRect { .. } => true,
        ChartKind::ErrorMark(data) => {
            if data.orient == crate::ir::ErrorMarkOrient::Vertical {
                channel == "x"
            } else {
                channel == "y"
            }
        }
        ChartKind::VegaBoxPlot(data) => {
            if data.orient == crate::ir::VegaBoxPlotOrient::Vertical {
                channel == "x"
            } else {
                channel == "y"
            }
        }
        _ => false,
    };
    if !category_axis {
        return;
    }
    let previous = match (&spec.kind, channel) {
        (ChartKind::VegaRect { x_labels, .. }, "x") => x_labels.clone(),
        (ChartKind::VegaRect { y_labels, .. }, "y") => y_labels.clone(),
        (ChartKind::VegaBoxPlot(data), "x" | "y") => data.categories.clone(),
        _ => spec.categories.clone(),
    };
    if previous == domain {
        spec.categories = domain.to_vec();
        return;
    }
    let positions = previous
        .iter()
        .map(|category| domain.iter().position(|candidate| candidate == category))
        .collect::<Vec<_>>();
    for series in &mut spec.series {
        let mut values = vec![f64::NAN; domain.len()];
        for (old_index, new_index) in positions.iter().enumerate() {
            if let (Some(new_index), Some(value)) = (new_index, series.values.get(old_index)) {
                values[*new_index] = *value;
            }
        }
        series.values = values;
    }
    match &mut spec.kind {
        ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } => {
            if channel == "x" {
                let old_labels = x_labels.clone();
                let old_cells = cells.clone();
                let new_positions = old_labels
                    .iter()
                    .map(|label| domain.iter().position(|candidate| candidate == label))
                    .collect::<Vec<_>>();
                for row in 0..cells.len() {
                    let mut expanded = vec![None; domain.len()];
                    if let Some(old_row) = old_cells.get(row) {
                        for (old_column, new_column) in new_positions.iter().enumerate() {
                            if let (Some(new_column), Some(cell)) =
                                (new_column, old_row.get(old_column))
                            {
                                expanded[*new_column] = *cell;
                            }
                        }
                    }
                    cells[row] = expanded;
                }
                *x_labels = domain.to_vec();
            } else {
                let old_labels = y_labels.clone();
                let old_cells = cells.clone();
                let new_positions = old_labels
                    .iter()
                    .map(|label| domain.iter().position(|candidate| candidate == label))
                    .collect::<Vec<_>>();
                let columns = x_labels.len();
                let mut expanded = vec![vec![None; columns]; domain.len()];
                for (old_row, new_row) in new_positions.iter().enumerate() {
                    if let (Some(new_row), Some(row)) = (new_row, old_cells.get(old_row)) {
                        expanded[*new_row] = row.clone();
                    }
                }
                *cells = expanded;
                *y_labels = domain.to_vec();
            }
        }
        ChartKind::VegaBoxPlot(data) => {
            let old_categories = data.categories.clone();
            for group in &mut data.groups {
                group.category_index = group.category_index.and_then(|index| {
                    old_categories.get(index).and_then(|category| {
                        domain.iter().position(|candidate| candidate == category)
                    })
                });
            }
            data.categories = domain.to_vec();
        }
        ChartKind::ErrorMark(data) => {
            let category_channel = if data.orient == crate::ir::ErrorMarkOrient::Vertical {
                "x"
            } else {
                "y"
            };
            if channel == category_channel {
                for range in &mut data.ranges {
                    if let crate::ir::ErrorPosition::Category(index) = &mut range.position {
                        *index = positions
                            .get(*index)
                            .and_then(|position| *position)
                            .unwrap_or(*index);
                    }
                }
            }
        }
        _ => {}
    }
    spec.categories = domain.to_vec();
}

fn background_rect(width: f64, height: f64, fill: Color) -> Prim {
    Prim::Rect {
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
        fill,
    }
}

fn title_primitive(title: &str, width: f64) -> Prim {
    Prim::Text {
        x: width / 2.0,
        y: crate::layout::common::OUTER_PAD + crate::layout::common::TITLE_FONT,
        size: crate::layout::common::TITLE_FONT,
        anchor: Anchor::Middle,
        fill: crate::layout::common::INK,
        content: title.to_owned(),
        rotate_deg: None,
    }
}

fn path_prefix(path: &str) -> String {
    if path.is_empty() {
        "root: ".to_owned()
    } else {
        format!("{path}: ")
    }
}
