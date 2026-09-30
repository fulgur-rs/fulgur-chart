//! Vega-Lite text marks placed in the shared quantitative scatter frame.

use crate::ir::{ChartKind, VegaTextAlign};
use crate::layout::scatter;
use crate::scene::{Anchor, Prim, Scene, StyledText};
use crate::text::TextMeasurer;

pub(crate) fn build(spec: &crate::ir::ChartSpec, measurer: &TextMeasurer) -> Scene {
    let mut scene = scatter::build(spec, measurer);
    let ChartKind::VegaText(data) = &spec.kind else {
        return scene;
    };
    let layout = scatter::compute_scatter_layout(spec, measurer);

    for mark in &data.marks {
        let point = mark.point;
        if !crate::layout::common::axis_value_in_bounds(point.x, &layout.x_ticks)
            || !crate::layout::common::axis_value_in_bounds(point.y, &layout.y_ticks)
        {
            continue;
        }
        let Some(x) = scatter::map_scatter_line_axis(&layout.xs, point.x) else {
            continue;
        };
        let Some(y) = scatter::map_scatter_line_axis(&layout.ys, point.y) else {
            continue;
        };
        let anchor = match mark.align {
            VegaTextAlign::Left => Anchor::Start,
            VegaTextAlign::Center => Anchor::Middle,
            VegaTextAlign::Right => Anchor::End,
        };
        scene.items.push(Prim::StyledText(Box::new(StyledText {
            x: x + mark.dx,
            y: y + mark.dy,
            size: mark.size,
            anchor,
            fill: mark.fill,
            content: mark.text.clone(),
            rotate_deg: mark.angle,
            baseline: mark.baseline,
            font_family: mark.font_family.clone(),
            font_weight: mark.font_weight.clone(),
            font_style: mark.font_style.clone(),
        })));
    }

    scene
}
