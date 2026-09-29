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
            let mut independent_color_guides = Vec::new();
            let mut independent_size_guides = Vec::new();
            for (index, (child_node, child)) in layer.children.iter().zip(children).enumerate() {
                if let VegaCompositionNode::Unit(leaf) = child_node {
                    let mut frame_spec = (*leaf.spec).clone();
                    frame_spec.size_mode = crate::ir::SizeMode::Canvas;
                    apply_leaf_domains(&mut frame_spec, &leaf.scales);
                    let frame = crate::layout::common::compute(&frame_spec, measurer);
                    let (marks, y_guides, x_guides, title_items) = split_layer_scene_items(
                        &child.items,
                        &leaf.spec.kind,
                        leaf.spec.title.as_deref(),
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
                        if layer.resolve.color_legend == crate::ir::VegaResolutionMode::Independent
                        {
                            let guides = leaf_color_legend_items(
                                &child.items,
                                &leaf.spec,
                                frame.plot_left,
                                frame.plot_right,
                                frame.plot_top,
                                frame.plot_bottom,
                            );
                            if !guides.is_empty() {
                                independent_color_guides.push(guides);
                            }
                        }
                        if layer.resolve.size_legend == crate::ir::VegaResolutionMode::Independent
                            && let Some(guide) = &leaf.spec.vega_size_legend
                        {
                            let guides = leaf_size_legend_items(&child.items, guide);
                            if !guides.is_empty() {
                                independent_size_guides.push(guides);
                            }
                        }
                        if independent_y {
                            independent_y_guides.push((index, y_guides));
                        }
                        if independent_x {
                            independent_x_guides.push((index, x_guides));
                        }
                        if !title_items.is_empty() {
                            items.push(Prim::Group {
                                translate_x: 0.0,
                                translate_y: content_y,
                                clip: Some(Box::new(ClipRect {
                                    x: 0.0,
                                    y: 0.0,
                                    w: view_width,
                                    h: view_height,
                                })),
                                children: title_items,
                            });
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
                } else {
                    let mut child_items = child.items;
                    let translate_y = if index == 0 { content_y } else { title_height };
                    if index > 0 {
                        let shared_legends = LegendChannels {
                            color: layer.resolve.color_legend
                                == crate::ir::VegaResolutionMode::Shared,
                            size: layer.resolve.size_legend
                                == crate::ir::VegaResolutionMode::Shared,
                        };
                        if shared_legends.any() {
                            strip_node_legend(
                                child_node,
                                &mut child_items,
                                measurer,
                                shared_legends,
                            );
                        }
                    }
                    items.push(Prim::Group {
                        translate_x: 0.0,
                        translate_y,
                        clip: Some(Box::new(ClipRect {
                            x: 0.0,
                            y: 0.0,
                            w: view_width,
                            h: view_height,
                        })),
                        children: child_items,
                    });
                }
            }
            for guides in independent_color_guides {
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
            }
            for guides in independent_size_guides {
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
            LegendChannels {
                color: concat.resolve.color_legend == crate::ir::VegaResolutionMode::Shared,
                size: concat.resolve.size_legend == crate::ir::VegaResolutionMode::Shared,
            },
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
            LegendChannels {
                color: concat.resolve.color_legend == crate::ir::VegaResolutionMode::Shared,
                size: concat.resolve.size_legend == crate::ir::VegaResolutionMode::Shared,
            },
            false,
            measurer,
            limits,
        ),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct LegendChannels {
    color: bool,
    size: bool,
}

impl LegendChannels {
    fn any(self) -> bool {
        self.color || self.size
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
    shared_legends: LegendChannels,
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
        if shared_legends.any() && index > 0 {
            strip_node_legend(node, &mut child.items, measurer, shared_legends);
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
    requested: LegendChannels,
) {
    match node {
        VegaCompositionNode::Unit(leaf) => {
            if requested.color {
                strip_unit_legend_color(leaf, items, measurer);
            }
            if requested.size
                && let Some(guide) = &leaf.spec.vega_size_legend
            {
                strip_leaf_size_legend(items, guide);
            }
        }
        VegaCompositionNode::Layer(layer) => {
            let channels = LegendChannels {
                color: requested.color
                    && layer.resolve.color_legend == crate::ir::VegaResolutionMode::Shared,
                size: requested.size
                    && layer.resolve.size_legend == crate::ir::VegaResolutionMode::Shared,
            };
            if !channels.any() {
                return;
            }
            let Some(first) = layer.children.first() else {
                return;
            };
            let Some(group) = items.iter_mut().find_map(group_children_mut) else {
                return;
            };
            match first {
                VegaCompositionNode::Unit(leaf) => {
                    if channels.color {
                        strip_unit_legend_color(leaf, group, measurer);
                    }
                    if channels.size
                        && let Some(guide) = &leaf.spec.vega_size_legend
                    {
                        strip_leaf_size_legend(group, guide);
                    }
                }
                nested => strip_node_legend(nested, group, measurer, channels),
            }
        }
        VegaCompositionNode::HConcat(concat) | VegaCompositionNode::VConcat(concat) => {
            let channels = LegendChannels {
                color: requested.color
                    && concat.resolve.color_legend == crate::ir::VegaResolutionMode::Shared,
                size: requested.size
                    && concat.resolve.size_legend == crate::ir::VegaResolutionMode::Shared,
            };
            if !channels.any() {
                return;
            }
            let mut groups = items.iter_mut().filter_map(group_children_mut);
            for child in &concat.children {
                let Some(child_items) = groups.next() else {
                    break;
                };
                strip_node_legend(child, child_items, measurer, channels);
            }
        }
    }
}

fn strip_unit_legend_color(
    leaf: &crate::ir::VegaCompositionLeaf,
    items: &mut Vec<Prim>,
    measurer: &TextMeasurer<'_>,
) {
    let mut spec = (*leaf.spec).clone();
    spec.size_mode = crate::ir::SizeMode::Canvas;
    apply_leaf_domains(&mut spec, &leaf.scales);
    let frame = crate::layout::common::compute(&spec, measurer);
    strip_leaf_color_legend(
        items,
        &spec,
        frame.plot_left,
        frame.plot_right,
        frame.plot_top,
        frame.plot_bottom,
    );
}

fn group_children_mut(prim: &mut Prim) -> Option<&mut Vec<Prim>> {
    match prim {
        Prim::Group { children, .. } => Some(children),
        _ => None,
    }
}

fn strip_leaf_color_legend(
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
        if spec
            .vega_size_legend
            .as_ref()
            .is_some_and(|guide| is_size_legend_group(item, guide))
        {
            return true;
        }
        if let Prim::Group { children, .. } = item {
            strip_leaf_color_legend(children, spec, plot_left, plot_right, plot_top, plot_bottom);
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

fn strip_leaf_size_legend(items: &mut Vec<Prim>, guide: &crate::ir::VegaSizeLegend) {
    items.retain_mut(|item| {
        if is_size_legend_group(item, guide) {
            return false;
        }
        if let Prim::Group { children, .. } = item {
            strip_leaf_size_legend(children, guide);
            return !children.is_empty();
        }
        true
    });
}

fn leaf_size_legend_items(items: &[Prim], guide: &crate::ir::VegaSizeLegend) -> Vec<Prim> {
    items
        .iter()
        .filter(|item| is_size_legend_group(item, guide))
        .cloned()
        .collect()
}

fn is_size_legend_group(item: &Prim, guide: &crate::ir::VegaSizeLegend) -> bool {
    let Prim::Group {
        clip: Some(_),
        children,
        ..
    } = item
    else {
        return false;
    };
    let mut texts = Vec::new();
    collect_prim_text(children, &mut texts);
    guide
        .entries
        .iter()
        .all(|entry| texts.contains(&entry.label.as_str()))
        && guide
            .title
            .as_ref()
            .is_none_or(|title| texts.contains(&title.as_str()))
}

fn collect_prim_text<'a>(items: &'a [Prim], output: &mut Vec<&'a str>) {
    for item in items {
        match item {
            Prim::Text { content, .. } => output.push(content),
            Prim::StyledText(text) => output.push(&text.content),
            Prim::Group { children, .. } => collect_prim_text(children, output),
            _ => {}
        }
    }
}

fn leaf_color_legend_items(
    items: &[Prim],
    spec: &ChartSpec,
    plot_left: f64,
    plot_right: f64,
    plot_top: f64,
    plot_bottom: f64,
) -> Vec<Prim> {
    if spec.legend == crate::ir::LegendPos::None {
        return Vec::new();
    }
    let labels = spec
        .series
        .iter()
        .filter(|series| !series.name.is_empty())
        .map(|series| series.name.as_str())
        .chain(crate::layout::common::legend_title(spec))
        .collect::<Vec<_>>();
    items
        .iter()
        .filter(|item| {
            let (x, y) = prim_position(item);
            let in_band = match spec.legend {
                crate::ir::LegendPos::Right => x >= plot_right + 2.0,
                crate::ir::LegendPos::Left => x <= plot_left - 2.0,
                crate::ir::LegendPos::Top => y <= plot_top - 2.0,
                crate::ir::LegendPos::Bottom => y >= plot_bottom + 2.0,
                crate::ir::LegendPos::None => false,
            };
            if !in_band {
                return false;
            }
            let is_label = match item {
                Prim::Text { content, .. } => labels.contains(&content.as_str()),
                Prim::StyledText(text) => labels.contains(&text.content.as_str()),
                _ => false,
            };
            is_label
                || is_legend_marker(
                    item,
                    spec.legend,
                    plot_left,
                    plot_right,
                    plot_top,
                    plot_bottom,
                )
        })
        .cloned()
        .collect()
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
    title: Option<&str>,
    plot_left: f64,
    plot_bottom: f64,
) -> (Vec<Prim>, Vec<Prim>, Vec<Prim>, Vec<Prim>) {
    let mut marks = Vec::new();
    let mut y_guides = Vec::new();
    let mut x_guides = Vec::new();
    let mut titles = Vec::new();
    for item in items {
        if is_mark_primitive(item, kind) {
            marks.push(item.clone());
        } else if is_title_primitive(item, title) {
            titles.push(item.clone());
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
    (marks, y_guides, x_guides, titles)
}

fn is_title_primitive(prim: &Prim, title: Option<&str>) -> bool {
    let Some(title) = title else { return false };
    match prim {
        Prim::Text { content, .. } => content == title,
        Prim::StyledText(text) => text.content == title,
        _ => false,
    }
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
                for (row_index, row_cells) in cells.iter_mut().enumerate() {
                    let mut expanded = vec![None; domain.len()];
                    if let Some(old_row) = old_cells.get(row_index) {
                        for (old_column, new_column) in new_positions.iter().enumerate() {
                            if let (Some(new_column), Some(cell)) =
                                (new_column, old_row.get(old_column))
                            {
                                expanded[*new_column] = *cell;
                            }
                        }
                    }
                    *row_cells = expanded;
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
