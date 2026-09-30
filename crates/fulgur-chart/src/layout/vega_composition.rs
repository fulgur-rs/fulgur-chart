//! Scene composition for recursive Vega-Lite layer and concat views.

use crate::guard::InputLimits;
use crate::ir::{ChartKind, ChartSpec, Color, VegaCompositionNode};
use crate::scene::{Anchor, ClipRect, Prim, Scene};
use crate::text::TextMeasurer;

const COMPOSITION_TITLE_BAND: f64 = crate::layout::common::TITLE_BAND;

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlotRect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl PlotRect {
    fn intersection(self, other: Self) -> Option<Self> {
        let rect = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
    }

    fn is_close_to(self, other: Self) -> bool {
        [
            self.left - other.left,
            self.top - other.top,
            self.right - other.right,
            self.bottom - other.bottom,
        ]
        .into_iter()
        .all(|difference| difference.abs() <= 1e-6)
    }
}

fn plot_rect_for_spec(spec: &ChartSpec, measurer: &TextMeasurer<'_>) -> Option<PlotRect> {
    let (left, top, right, bottom) = match &spec.kind {
        ChartKind::Bar {
            horizontal: true, ..
        } => {
            let frame = crate::layout::bar::horizontal_bar_layout(spec, measurer);
            (
                frame.plot_left,
                frame.plot_top,
                frame.plot_right,
                frame.plot_bottom,
            )
        }
        ChartKind::Bar { .. } | ChartKind::Line { .. } | ChartKind::Trail | ChartKind::Mixed => {
            let frame = crate::layout::common::compute(spec, measurer);
            (
                frame.plot_left,
                frame.plot_top,
                frame.plot_right,
                frame.plot_bottom,
            )
        }
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square | ChartKind::VegaImage(_) => {
            let frame = crate::layout::scatter::compute_scatter_layout(spec, measurer);
            (
                frame.plot_left,
                frame.plot_top,
                frame.plot_right,
                frame.plot_bottom,
            )
        }
        ChartKind::VegaRect { .. } => crate::layout::vega_rect::plot_rect(spec, measurer),
        ChartKind::ErrorMark(_) => {
            let frame = crate::layout::error_mark::compute_frame(spec, measurer);
            (
                frame.plot_left,
                frame.plot_top,
                frame.plot_right,
                frame.plot_bottom,
            )
        }
        ChartKind::VegaBoxPlot(_) => {
            let frame = crate::layout::vega_boxplot::compute_frame(spec, measurer);
            (
                frame.plot_left,
                frame.plot_top,
                frame.plot_right,
                frame.plot_bottom,
            )
        }
        _ => return None,
    };
    [left, top, right, bottom]
        .into_iter()
        .all(f64::is_finite)
        .then_some(PlotRect {
            left,
            top,
            right,
            bottom,
        })
}

fn common_plot_rect(spec: &ChartSpec, measurer: &TextMeasurer<'_>) -> PlotRect {
    let frame = crate::layout::common::compute(spec, measurer);
    PlotRect {
        left: frame.plot_left,
        top: frame.plot_top,
        right: frame.plot_right,
        bottom: frame.plot_bottom,
    }
}

#[derive(Clone, Debug)]
struct VegaNodeLayout {
    width: f64,
    height: f64,
    items: Vec<Prim>,
    plot_rect: Option<PlotRect>,
    layer_mark_count: Option<usize>,
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
            let plot_rect = plot_rect_for_spec(&leaf_spec, measurer);
            let shared_color_categories = match leaf.scales.color.as_ref() {
                Some(crate::ir::VegaScaleDomain::Categories(categories)) => {
                    Some(categories.as_slice())
                }
                _ => None,
            };
            let (scene, layer_mark_count) = crate::layout::build_scene_checked_with_layer_marks(
                &leaf_spec,
                measurer,
                limits,
                shared_color_categories,
            )
            .map_err(|error| format!("{}: {error}", leaf.path))?;
            Ok(VegaNodeLayout {
                width: scene.width,
                height: scene.height,
                items: scene.items,
                plot_rect,
                layer_mark_count,
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
            let mut common_plot_rect: Option<PlotRect> = None;
            for (index, child) in children.iter().enumerate() {
                let child_rect = child.plot_rect.ok_or_else(|| {
                    format!(
                        "{}layer child {index} has no Cartesian plot rectangle",
                        path_prefix(&layer.path)
                    )
                })?;
                common_plot_rect = Some(match common_plot_rect {
                    Some(parent) => parent.intersection(child_rect).ok_or_else(|| {
                        format!(
                            "{}layer child plot rectangles do not overlap at child {index}",
                            path_prefix(&layer.path)
                        )
                    })?,
                    None => child_rect,
                });
            }
            let common_plot_rect = common_plot_rect.expect("layer has at least one child");
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
            let mut retained_shared_color_legend = false;
            let mut retained_shared_size_legend = false;
            for (index, (child_node, mut child)) in layer.children.iter().zip(children).enumerate()
            {
                if let VegaCompositionNode::Unit(leaf) = child_node {
                    let mut frame_spec = (*leaf.spec).clone();
                    frame_spec.size_mode = crate::ir::SizeMode::Canvas;
                    apply_leaf_domains(&mut frame_spec, &leaf.scales);
                    let frame = child.plot_rect.expect("layer unit has a plot rectangle");
                    let color_guides = leaf_color_legend_items(&child.items, leaf, frame);
                    let size_guides = leaf
                        .spec
                        .vega_size_legend
                        .as_ref()
                        .map(|guide| leaf_size_legend_items(&child.items, guide))
                        .unwrap_or_default();
                    let mut protected_guides = color_guides.clone();
                    protected_guides.extend(size_guides.iter().cloned());
                    transform_layer_scene_to_rect(
                        &mut child.items,
                        frame,
                        common_plot_rect,
                        &protected_guides,
                        leaf.spec.title.as_deref(),
                        child.width,
                        child.height,
                    )
                    .map_err(|error| format!("{}{error}", path_prefix(&layer.path)))?;
                    let (marks, y_guides, x_guides, title_items) = split_layer_scene_items(
                        &child.items,
                        &leaf.spec.kind,
                        child.layer_mark_count,
                        &protected_guides,
                        leaf.spec.title.as_deref(),
                        common_plot_rect.left,
                        common_plot_rect.bottom,
                    );
                    if index == 0 {
                        // Keep the first child's frame and guides, but defer its marks so all
                        // layer marks follow source order after the guides.
                        let guides = layer_guides(
                            child.items,
                            &leaf.spec.kind,
                            child.layer_mark_count,
                            &protected_guides,
                        );
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
                        if layer.resolve.color_legend == crate::ir::VegaResolutionMode::Shared
                            && !color_guides.is_empty()
                        {
                            retained_shared_color_legend = true;
                        }
                        if layer.resolve.size_legend == crate::ir::VegaResolutionMode::Shared
                            && !size_guides.is_empty()
                        {
                            retained_shared_size_legend = true;
                        }
                    } else {
                        if layer.resolve.color_legend == crate::ir::VegaResolutionMode::Independent
                        {
                            if !color_guides.is_empty() {
                                independent_color_guides.push(color_guides.clone());
                            }
                        } else if !retained_shared_color_legend && !color_guides.is_empty() {
                            if !color_guides.is_empty() {
                                items.push(Prim::Group {
                                    translate_x: 0.0,
                                    translate_y: content_y,
                                    clip: Some(Box::new(ClipRect {
                                        x: 0.0,
                                        y: 0.0,
                                        w: view_width,
                                        h: view_height,
                                    })),
                                    children: color_guides.clone(),
                                });
                            }
                            retained_shared_color_legend = true;
                        }
                        if layer.resolve.size_legend == crate::ir::VegaResolutionMode::Independent {
                            if !size_guides.is_empty() {
                                independent_size_guides.push(size_guides.clone());
                            }
                        } else if !retained_shared_size_legend && !size_guides.is_empty() {
                            items.push(Prim::Group {
                                translate_x: 0.0,
                                translate_y: content_y,
                                clip: Some(Box::new(ClipRect {
                                    x: 0.0,
                                    y: 0.0,
                                    w: view_width,
                                    h: view_height,
                                })),
                                children: size_guides.clone(),
                            });
                            retained_shared_size_legend = true;
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
                    if let Some(frame) = child.plot_rect {
                        let protected_guides =
                            node_legend_items(child_node, &child_items, measurer);
                        transform_layer_scene_to_rect(
                            &mut child_items,
                            frame,
                            common_plot_rect,
                            &protected_guides,
                            None,
                            child.width,
                            child.height,
                        )
                        .map_err(|error| format!("{}{error}", path_prefix(&layer.path)))?;
                    }
                    let child_has_color_legend = layer.resolve.color_legend
                        == crate::ir::VegaResolutionMode::Shared
                        && node_has_color_legend(child_node);
                    let child_has_size_legend = layer.resolve.size_legend
                        == crate::ir::VegaResolutionMode::Shared
                        && node_has_size_legend(child_node);
                    let translate_y = if index == 0 { content_y } else { title_height };
                    if index > 0 {
                        let shared_legends = LegendChannels {
                            color: child_has_color_legend && retained_shared_color_legend,
                            size: child_has_size_legend && retained_shared_size_legend,
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
                    if child_has_color_legend {
                        retained_shared_color_legend = true;
                    }
                    if child_has_size_legend {
                        retained_shared_size_legend = true;
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
                plot_rect: Some(PlotRect {
                    left: common_plot_rect.left,
                    top: common_plot_rect.top + content_y,
                    right: common_plot_rect.right,
                    bottom: common_plot_rect.bottom + content_y,
                }),
                layer_mark_count: None,
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

fn node_has_color_legend(node: &VegaCompositionNode) -> bool {
    match node {
        VegaCompositionNode::Unit(leaf) => {
            if leaf.spec.legend == crate::ir::LegendPos::None {
                return false;
            }
            match &leaf.spec.kind {
                ChartKind::VegaBoxPlot(data) => {
                    data.groups.iter().any(|group| group.color_label.is_some())
                }
                _ => {
                    leaf.spec
                        .series
                        .iter()
                        .any(|series| !series.name.is_empty())
                        || crate::layout::common::legend_title(&leaf.spec).is_some()
                }
            }
        }
        VegaCompositionNode::Layer(layer)
            if layer.resolve.color_legend == crate::ir::VegaResolutionMode::Shared =>
        {
            layer.children.iter().any(node_has_color_legend)
        }
        VegaCompositionNode::HConcat(concat) | VegaCompositionNode::VConcat(concat)
            if concat.resolve.color_legend == crate::ir::VegaResolutionMode::Shared =>
        {
            concat.children.iter().any(node_has_color_legend)
        }
        _ => false,
    }
}

fn node_has_size_legend(node: &VegaCompositionNode) -> bool {
    match node {
        VegaCompositionNode::Unit(leaf) => leaf
            .spec
            .vega_size_legend
            .as_ref()
            .is_some_and(|guide| !guide.entries.is_empty()),
        VegaCompositionNode::Layer(layer)
            if layer.resolve.size_legend == crate::ir::VegaResolutionMode::Shared =>
        {
            layer.children.iter().any(node_has_size_legend)
        }
        VegaCompositionNode::HConcat(concat) | VegaCompositionNode::VConcat(concat)
            if concat.resolve.size_legend == crate::ir::VegaResolutionMode::Shared =>
        {
            concat.children.iter().any(node_has_size_legend)
        }
        _ => false,
    }
}

fn node_legend_items(
    node: &VegaCompositionNode,
    items: &[Prim],
    measurer: &TextMeasurer<'_>,
) -> Vec<Prim> {
    let mut guides = Vec::new();
    collect_node_legend_items(node, items, measurer, &mut guides);
    guides
}

fn collect_node_legend_items(
    node: &VegaCompositionNode,
    items: &[Prim],
    measurer: &TextMeasurer<'_>,
    guides: &mut Vec<Prim>,
) {
    match node {
        VegaCompositionNode::Unit(leaf) => {
            let mut spec = (*leaf.spec).clone();
            spec.size_mode = crate::ir::SizeMode::Canvas;
            apply_leaf_domains(&mut spec, &leaf.scales);
            if let Some(frame) = plot_rect_for_spec(&spec, measurer) {
                let labels = leaf_color_legend_labels(leaf);
                collect_leaf_color_legend_items(items, &spec, &labels, frame, guides);
            }
            if let Some(guide) = &leaf.spec.vega_size_legend {
                collect_leaf_size_legend_items(items, guide, guides);
            }
        }
        VegaCompositionNode::Layer(layer) => {
            for child in &layer.children {
                collect_node_legend_items(child, items, measurer, guides);
            }
        }
        VegaCompositionNode::HConcat(concat) | VegaCompositionNode::VConcat(concat) => {
            let mut child_items = items.iter().filter_map(|item| match item {
                Prim::Group { children, .. } => Some(children.as_slice()),
                _ => None,
            });
            for child in &concat.children {
                let Some(child_items) = child_items.next() else {
                    break;
                };
                collect_node_legend_items(child, child_items, measurer, guides);
            }
        }
    }
}

fn collect_leaf_color_legend_items(
    items: &[Prim],
    spec: &ChartSpec,
    labels: &[String],
    frame: PlotRect,
    guides: &mut Vec<Prim>,
) {
    if spec.legend == crate::ir::LegendPos::None {
        return;
    }
    for item in items {
        if spec
            .vega_size_legend
            .as_ref()
            .is_some_and(|guide| is_size_legend_group(item, guide))
        {
            continue;
        }
        if let Prim::Group { children, .. } = item {
            collect_leaf_color_legend_items(children, spec, labels, frame, guides);
            continue;
        }
        let (x, y) = prim_position(item);
        let in_band = match spec.legend {
            crate::ir::LegendPos::Right => x >= frame.right + 2.0,
            crate::ir::LegendPos::Left => x <= frame.left - 2.0,
            crate::ir::LegendPos::Top => y <= frame.top - 2.0,
            crate::ir::LegendPos::Bottom => y >= frame.bottom + 2.0,
            crate::ir::LegendPos::None => false,
        };
        if !in_band {
            continue;
        }
        let is_label = match item {
            Prim::Text { content, .. } => labels.iter().any(|label| label == content),
            Prim::StyledText(text) => labels.iter().any(|label| label == &text.content),
            _ => false,
        };
        if (is_label
            || is_legend_marker(
                item,
                spec.legend,
                frame.left,
                frame.right,
                frame.top,
                frame.bottom,
            ))
            && !guides.contains(item)
        {
            guides.push(item.clone());
        }
    }
}

fn collect_leaf_size_legend_items(
    items: &[Prim],
    guide: &crate::ir::VegaSizeLegend,
    guides: &mut Vec<Prim>,
) {
    for item in items {
        if is_size_legend_group(item, guide) {
            if !guides.contains(item) {
                guides.push(item.clone());
            }
        } else if let Prim::Group { children, .. } = item {
            collect_leaf_size_legend_items(children, guide, guides);
        }
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
    let mut retained_shared_color_legend = false;
    let mut retained_shared_size_legend = false;
    for (index, (node, mut child)) in nodes.iter().zip(children).enumerate() {
        let child_has_color_legend = shared_legends.color && node_has_color_legend(node);
        let child_has_size_legend = shared_legends.size && node_has_size_legend(node);
        let duplicate_legends = LegendChannels {
            color: child_has_color_legend && retained_shared_color_legend,
            size: child_has_size_legend && retained_shared_size_legend,
        };
        if index > 0 && duplicate_legends.any() {
            strip_node_legend(node, &mut child.items, measurer, duplicate_legends);
        }
        if child_has_color_legend {
            retained_shared_color_legend = true;
        }
        if child_has_size_legend {
            retained_shared_size_legend = true;
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
        plot_rect: None,
        layer_mark_count: None,
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
            // Layer guides are distributed among the frame group and each child contribution.
            // Strip every contributing child so this nested layer can be deduplicated by its
            // parent even when its first child does not produce a guide.
            for child in &layer.children {
                match child {
                    VegaCompositionNode::Unit(leaf) => {
                        if channels.color {
                            strip_unit_legend_color(leaf, items, measurer);
                        }
                        if channels.size
                            && let Some(guide) = &leaf.spec.vega_size_legend
                        {
                            strip_leaf_size_legend(items, guide);
                        }
                    }
                    nested => strip_node_legend(nested, items, measurer, channels),
                }
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
    let frame =
        plot_rect_for_spec(&spec, measurer).unwrap_or_else(|| common_plot_rect(&spec, measurer));
    strip_leaf_color_legend(
        items,
        &spec,
        &leaf_color_legend_labels(leaf),
        frame.left,
        frame.right,
        frame.top,
        frame.bottom,
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
    labels: &[String],
    plot_left: f64,
    plot_right: f64,
    plot_top: f64,
    plot_bottom: f64,
) {
    if spec.legend == crate::ir::LegendPos::None {
        return;
    }
    items.retain_mut(|item| {
        if spec
            .vega_size_legend
            .as_ref()
            .is_some_and(|guide| is_size_legend_group(item, guide))
        {
            return true;
        }
        if let Prim::Group { children, .. } = item {
            strip_leaf_color_legend(
                children,
                spec,
                labels,
                plot_left,
                plot_right,
                plot_top,
                plot_bottom,
            );
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
            Prim::Text { content, .. } => labels.iter().any(|label| label == content),
            Prim::StyledText(text) => labels.iter().any(|label| label == &text.content),
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
    leaf: &crate::ir::VegaCompositionLeaf,
    plot_rect: PlotRect,
) -> Vec<Prim> {
    let spec = &leaf.spec;
    if spec.legend == crate::ir::LegendPos::None {
        return Vec::new();
    }
    let labels = leaf_color_legend_labels(leaf);
    items
        .iter()
        .filter(|item| {
            let (x, y) = prim_position(item);
            let in_band = match spec.legend {
                crate::ir::LegendPos::Right => x >= plot_rect.right + 2.0,
                crate::ir::LegendPos::Left => x <= plot_rect.left - 2.0,
                crate::ir::LegendPos::Top => y <= plot_rect.top - 2.0,
                crate::ir::LegendPos::Bottom => y >= plot_rect.bottom + 2.0,
                crate::ir::LegendPos::None => false,
            };
            if !in_band {
                return false;
            }
            let is_label = match item {
                Prim::Text { content, .. } => labels.iter().any(|label| label == content),
                Prim::StyledText(text) => labels.iter().any(|label| label == &text.content),
                _ => false,
            };
            is_label
                || is_legend_marker(
                    item,
                    spec.legend,
                    plot_rect.left,
                    plot_rect.right,
                    plot_rect.top,
                    plot_rect.bottom,
                )
        })
        .cloned()
        .collect()
}

fn leaf_color_legend_labels(leaf: &crate::ir::VegaCompositionLeaf) -> Vec<String> {
    let mut labels = leaf
        .spec
        .series
        .iter()
        .filter(|series| !series.name.is_empty())
        .map(|series| series.name.clone())
        .collect::<Vec<_>>();
    if let Some(crate::ir::VegaScaleDomain::Categories(categories)) = &leaf.scales.color {
        labels.extend(categories.iter().cloned());
    }
    if let ChartKind::VegaBoxPlot(data) = &leaf.spec.kind {
        labels.extend(
            data.groups
                .iter()
                .filter_map(|group| group.color_label.clone()),
        );
    }
    labels.extend(crate::layout::common::legend_title(&leaf.spec).map(str::to_owned));
    let mut unique = Vec::with_capacity(labels.len());
    for label in labels {
        if !unique.contains(&label) {
            unique.push(label);
        }
    }
    unique
}

fn prim_position(prim: &Prim) -> (f64, f64) {
    match prim {
        Prim::Text { x, y, .. } => (*x, *y),
        Prim::StyledText(text) => (text.x, text.y),
        Prim::Rect { x, y, w, h, .. } => (x + w / 2.0, y + h / 2.0),
        Prim::Circle { cx, cy, .. } | Prim::ClippedCircle { cx, cy, .. } => (*cx, *cy),
        Prim::Path { d, .. } | Prim::ClippedPath { d, .. } => path_position(d),
        Prim::Line { x1, y1, x2, y2, .. } => ((x1 + x2) / 2.0, (y1 + y2) / 2.0),
        Prim::Polyline { points, .. } | Prim::StyledPolyline { points, .. } => points
            .first()
            .copied()
            .unwrap_or((f64::NEG_INFINITY, f64::NEG_INFINITY)),
        _ => (f64::NEG_INFINITY, f64::NEG_INFINITY),
    }
}

fn path_position(data: &str) -> (f64, f64) {
    let values = data
        .split_ascii_whitespace()
        .filter_map(|token| token.parse::<f64>().ok())
        .collect::<Vec<_>>();
    if values.len() < 2 || values.len() % 2 != 0 {
        return (f64::NEG_INFINITY, f64::NEG_INFINITY);
    }
    let points = values.chunks_exact(2).collect::<Vec<_>>();
    let min_x = points
        .iter()
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point[1])
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max);
    ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
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

fn transform_layer_scene_to_rect(
    items: &mut [Prim],
    from: PlotRect,
    to: PlotRect,
    protected_guides: &[Prim],
    title: Option<&str>,
    scene_width: f64,
    scene_height: f64,
) -> Result<(), String> {
    let from_width = from.right - from.left;
    let from_height = from.bottom - from.top;
    let to_width = to.right - to.left;
    let to_height = to.bottom - to.top;
    if ![from_width, from_height, to_width, to_height]
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
    {
        return Err("layer plot rectangle must have positive finite dimensions".into());
    }
    let scale_x = to_width / from_width;
    let scale_y = to_height / from_height;
    let offset_x = to.left - scale_x * from.left;
    let offset_y = to.top - scale_y * from.top;
    if from.is_close_to(to) {
        return Ok(());
    }
    for item in items {
        if is_title_primitive(item, title) || is_full_background(item, scene_width, scene_height) {
            continue;
        }
        transform_primitive(
            item,
            scale_x,
            scale_y,
            offset_x,
            offset_y,
            true,
            protected_guides,
        )?;
    }
    Ok(())
}

fn is_full_background(item: &Prim, width: f64, height: f64) -> bool {
    matches!(item,
        Prim::Rect { x, y, w, h, .. }
            if x.abs() <= f64::EPSILON
                && y.abs() <= f64::EPSILON
                && (*w - width).abs() <= f64::EPSILON
                && (*h - height).abs() <= f64::EPSILON
    )
}

fn transform_primitive(
    prim: &mut Prim,
    scale_x: f64,
    scale_y: f64,
    offset_x: f64,
    offset_y: f64,
    apply_offset: bool,
    protected_guides: &[Prim],
) -> Result<(), String> {
    if protected_guides.contains(prim) {
        return Ok(());
    }
    let x = |value: f64| value * scale_x + if apply_offset { offset_x } else { 0.0 };
    let y = |value: f64| value * scale_y + if apply_offset { offset_y } else { 0.0 };
    let clip = |clip: &mut ClipRect, add_offset: bool| {
        clip.x = clip.x * scale_x + if add_offset { offset_x } else { 0.0 };
        clip.y = clip.y * scale_y + if add_offset { offset_y } else { 0.0 };
        clip.w *= scale_x;
        clip.h *= scale_y;
    };
    match prim {
        Prim::Rect {
            x: left,
            y: top,
            w,
            h,
            ..
        } => {
            *left = x(*left);
            *top = y(*top);
            *w *= scale_x;
            *h *= scale_y;
        }
        Prim::Image {
            x: left,
            y: top,
            width,
            height,
            ..
        } => {
            *left = x(*left);
            *top = y(*top);
            *width *= scale_x;
            *height *= scale_y;
        }
        Prim::Line { x1, y1, x2, y2, .. } => {
            *x1 = x(*x1);
            *y1 = y(*y1);
            *x2 = x(*x2);
            *y2 = y(*y2);
        }
        Prim::Polyline { points, .. } | Prim::StyledPolyline { points, .. } => {
            for (point_x, point_y) in points {
                *point_x = x(*point_x);
                *point_y = y(*point_y);
            }
        }
        Prim::Path { d, .. } | Prim::StyledPath { d, .. } => {
            *d = transform_path_data(d, scale_x, scale_y, offset_x, offset_y, apply_offset)?;
        }
        Prim::ClippedPath {
            d, clip: clip_rect, ..
        } => {
            *d = transform_path_data(d, scale_x, scale_y, offset_x, offset_y, apply_offset)?;
            clip(clip_rect, apply_offset);
        }
        Prim::GradientPath { d, x0, x1, .. } => {
            *d = transform_path_data(d, scale_x, scale_y, offset_x, offset_y, apply_offset)?;
            *x0 = x(*x0);
            *x1 = x(*x1);
        }
        Prim::Circle { cx, cy, .. } => {
            *cx = x(*cx);
            *cy = y(*cy);
        }
        Prim::ClippedCircle {
            cx,
            cy,
            clip: clip_rect,
            ..
        } => {
            *cx = x(*cx);
            *cy = y(*cy);
            clip(clip_rect, apply_offset);
        }
        Prim::Text {
            x: text_x,
            y: text_y,
            ..
        } => {
            *text_x = x(*text_x);
            *text_y = y(*text_y);
        }
        Prim::StyledText(text) => {
            text.x = x(text.x);
            text.y = y(text.y);
        }
        Prim::Group {
            translate_x,
            translate_y,
            clip: clip_rect,
            children,
        } => {
            *translate_x = x(*translate_x);
            *translate_y = y(*translate_y);
            if let Some(clip_rect) = clip_rect {
                clip(clip_rect, false);
            }
            for child in children {
                transform_primitive(child, scale_x, scale_y, 0.0, 0.0, false, protected_guides)?;
            }
        }
    }
    Ok(())
}

fn transform_path_data(
    data: &str,
    scale_x: f64,
    scale_y: f64,
    offset_x: f64,
    offset_y: f64,
    apply_offset: bool,
) -> Result<String, String> {
    let tokens = data.split_ascii_whitespace().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(tokens.len());
    let mut command = None;
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].len() == 1
            && tokens[index]
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
        {
            let next = tokens[index].as_bytes()[0] as char;
            if !matches!(
                next,
                'M' | 'L' | 'H' | 'V' | 'C' | 'S' | 'Q' | 'T' | 'A' | 'Z'
            ) {
                return Err(format!(
                    "layer cannot align unsupported path command {next}"
                ));
            }
            output.push(next.to_string());
            command = Some(next);
            index += 1;
            if next == 'Z' {
                command = None;
            }
            continue;
        }
        let Some(active) = command else {
            return Err("layer cannot align malformed path data".into());
        };
        let count = match active {
            'M' | 'L' | 'T' => 2,
            'H' | 'V' => 1,
            'C' => 6,
            'S' | 'Q' => 4,
            'A' => 7,
            _ => return Err("layer cannot align malformed path data".into()),
        };
        let mut values = Vec::with_capacity(count);
        for token in tokens.iter().skip(index).take(count) {
            let value = token
                .parse::<f64>()
                .map_err(|_| "layer cannot align malformed path coordinates".to_string())?;
            values.push(value);
        }
        if values.len() != count {
            return Err("layer cannot align malformed path data".into());
        }
        match active {
            'M' | 'L' | 'T' => {
                values[0] = values[0] * scale_x + if apply_offset { offset_x } else { 0.0 };
                values[1] = values[1] * scale_y + if apply_offset { offset_y } else { 0.0 };
            }
            'H' => values[0] = values[0] * scale_x + if apply_offset { offset_x } else { 0.0 },
            'V' => values[0] = values[0] * scale_y + if apply_offset { offset_y } else { 0.0 },
            'C' => {
                for pair in values.as_chunks_mut::<2>().0 {
                    pair[0] = pair[0] * scale_x + if apply_offset { offset_x } else { 0.0 };
                    pair[1] = pair[1] * scale_y + if apply_offset { offset_y } else { 0.0 };
                }
            }
            'S' | 'Q' => {
                for pair in values.as_chunks_mut::<2>().0 {
                    pair[0] = pair[0] * scale_x + if apply_offset { offset_x } else { 0.0 };
                    pair[1] = pair[1] * scale_y + if apply_offset { offset_y } else { 0.0 };
                }
            }
            'A' => {
                values[0] *= scale_x.abs();
                values[1] *= scale_y.abs();
                values[5] = values[5] * scale_x + if apply_offset { offset_x } else { 0.0 };
                values[6] = values[6] * scale_y + if apply_offset { offset_y } else { 0.0 };
            }
            _ => unreachable!(),
        }
        output.extend(values.into_iter().map(crate::num::fmt_num));
        index += count;
        if active == 'M' {
            command = Some('L');
        }
    }
    Ok(output.join(" "))
}

fn split_layer_scene_items(
    items: &[Prim],
    kind: &ChartKind,
    explicit_mark_count: Option<usize>,
    protected_guides: &[Prim],
    title: Option<&str>,
    plot_left: f64,
    plot_bottom: f64,
) -> (Vec<Prim>, Vec<Prim>, Vec<Prim>, Vec<Prim>) {
    let mut marks = Vec::new();
    let mut y_guides = Vec::new();
    let mut x_guides = Vec::new();
    let mut titles = Vec::new();
    let explicit_mark_start = explicit_mark_count.map(|count| items.len().saturating_sub(count));
    for (index, item) in items.iter().enumerate() {
        let is_mark = explicit_mark_start.map_or_else(
            || !protected_guides.contains(item) && is_mark_primitive(item, kind),
            |mark_start| index >= mark_start,
        );
        if is_mark {
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

fn layer_guides(
    items: Vec<Prim>,
    kind: &ChartKind,
    explicit_mark_count: Option<usize>,
    protected_guides: &[Prim],
) -> Vec<Prim> {
    let explicit_mark_start = explicit_mark_count.map(|count| items.len().saturating_sub(count));
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let is_mark = explicit_mark_start.map_or_else(
                || !protected_guides.contains(&item) && is_mark_primitive(&item, kind),
                |mark_start| index >= mark_start,
            );
            (!is_mark).then_some(item)
        })
        .collect()
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
        ChartKind::ErrorMark(_) | ChartKind::VegaBoxPlot(_) => false,
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
    let per_category_values = !matches!(
        &spec.kind,
        ChartKind::ErrorMark(_) | ChartKind::VegaBoxPlot(_) | ChartKind::VegaRect { .. }
    );
    if per_category_values {
        for series in &mut spec.series {
            let mut values = vec![f64::NAN; domain.len()];
            let mut trail_widths = series
                .trail_widths
                .as_ref()
                .map(|_| vec![1.0; domain.len()]);
            for (old_index, new_index) in positions.iter().enumerate() {
                let Some(new_index) = new_index else {
                    continue;
                };
                if let Some(value) = series.values.get(old_index) {
                    values[*new_index] = *value;
                }
                if let (Some(widths), Some(width)) = (
                    trail_widths.as_mut(),
                    series
                        .trail_widths
                        .as_ref()
                        .and_then(|widths| widths.get(old_index)),
                ) {
                    widths[*new_index] = *width;
                }
            }
            series.values = values;
            if let Some(widths) = trail_widths {
                series.trail_widths = Some(Box::new(widths));
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::vegalite;

    #[test]
    fn category_remap_keeps_error_range_centers_aligned() {
        let mut spec = vegalite::parse(
            r#"{"mark":"errorbar","data":{"values":[{"x":"A","lo":2,"hi":8}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"#,
            true,
        )
        .expect("errorbar parses");
        let centers = spec.series[0].values.clone();

        remap_categories(&mut spec, "x", &["B".into(), "A".into()]);

        assert_eq!(spec.series[0].values, centers);
        let ChartKind::ErrorMark(data) = &spec.kind else {
            panic!("errorbar kind is preserved")
        };
        assert_eq!(
            data.ranges[0].position,
            crate::ir::ErrorPosition::Category(1)
        );
    }

    #[test]
    fn category_remap_keeps_trail_widths_aligned_with_values() {
        let mut spec = vegalite::parse(
            r#"{"mark":"trail","data":{"values":[{"x":"B","y":10,"size":20},{"x":"A","y":5,"size":90}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"},"size":{"field":"size","type":"quantitative"}}}"#,
            true,
        )
        .expect("trail parses");
        assert_eq!(spec.categories, ["B", "A"]);
        assert_eq!(spec.series[0].values, [10.0, 5.0]);
        assert_eq!(spec.series[0].trail_widths_slice(), [1.0, 4.0]);

        remap_categories(&mut spec, "x", &["A".into(), "B".into(), "C".into()]);

        assert_eq!(spec.series[0].values[0..2], [5.0, 10.0]);
        assert!(spec.series[0].values[2].is_nan());
        assert_eq!(spec.series[0].trail_widths_slice(), [4.0, 1.0, 1.0]);
    }
}

#[cfg(test)]
mod nested_guide_transform_tests {
    use super::*;

    fn text_x(items: &[Prim], content: &str, output: &mut Vec<f64>) {
        for item in items {
            match item {
                Prim::Text {
                    x, content: text, ..
                } if text == content => output.push(*x),
                Prim::StyledText(text) if text.content == content => output.push(text.x),
                Prim::Group { children, .. } => text_x(children, content, output),
                _ => {}
            }
        }
    }

    #[test]
    fn protected_guide_inside_composition_groups_is_not_scaled() {
        let legend = Prim::Text {
            x: 50.0,
            y: 30.0,
            size: 12.0,
            anchor: Anchor::Start,
            fill: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            },
            content: "north".into(),
            rotate_deg: None,
        };
        let mut items = vec![Prim::Group {
            translate_x: 0.0,
            translate_y: 0.0,
            clip: None,
            children: vec![legend.clone()],
        }];

        transform_layer_scene_to_rect(
            &mut items,
            PlotRect {
                left: 0.0,
                top: 0.0,
                right: 100.0,
                bottom: 100.0,
            },
            PlotRect {
                left: 10.0,
                top: 10.0,
                right: 60.0,
                bottom: 60.0,
            },
            &[legend],
            None,
            100.0,
            100.0,
        )
        .expect("nested guide aligns");

        let Prim::Group { children, .. } = &items[0] else {
            panic!("composition group remains present")
        };
        let Prim::Text { x, y, .. } = &children[0] else {
            panic!("legend label remains text")
        };
        assert_eq!((*x, *y), (50.0, 30.0));
    }

    #[test]
    fn nested_node_legend_items_protect_nested_guides_from_plot_scaling() {
        let spec = crate::frontend::vegalite::parse(
            r#"{
              "layer":[{
                "mark":"point",
                "data":{"values":[{"x":1,"y":2,"group":"north"}]},
                "encoding":{
                  "x":{"field":"x","type":"quantitative"},
                  "y":{"field":"y","type":"quantitative"},
                  "color":{"field":"group","type":"nominal"}
                }
              }]
            }"#,
            true,
        )
        .expect("nested layer parses");
        let ChartKind::VegaComposition(root) = &spec.kind else {
            panic!("composition node expected")
        };
        let measurer = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
        let mut layout = build_node(root, &measurer, &InputLimits::default())
            .expect("nested layer scene builds");
        let frame = layout.plot_rect.expect("layer has a plot frame");
        let guides = node_legend_items(root, &layout.items, &measurer);
        assert!(guides.iter().any(|item| matches!(
            item,
            Prim::Text { content, .. } if content == "north"
        )));

        let mut before = Vec::new();
        text_x(&layout.items, "north", &mut before);
        assert_eq!(before.len(), 1);
        let narrower = PlotRect {
            right: frame.left + (frame.right - frame.left) * 0.5,
            ..frame
        };
        transform_layer_scene_to_rect(
            &mut layout.items,
            frame,
            narrower,
            &guides,
            None,
            layout.width,
            layout.height,
        )
        .expect("nested legend alignment succeeds");

        let mut after = Vec::new();
        text_x(&layout.items, "north", &mut after);
        assert_eq!(after, before);
    }
}
