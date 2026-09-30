//! Scene layout for Vega-Lite Cartesian `rule` marks.

use crate::guard::{InputLimits, validate_vega_rule};
use crate::ir::{ChartKind, ChartSpec, ErrorPosition, VegaRuleData, VegaRulePosition};
use crate::layout::error_mark::{self, ErrorAxis, ErrorMarkFrame};
use crate::scene::{ClipRect, Prim, Scene};
use crate::text::TextMeasurer;

fn rule_data(spec: &ChartSpec) -> &VegaRuleData {
    let ChartKind::VegaRule(data) = &spec.kind else {
        unreachable!("Vega-Lite rule layout only accepts ChartKind::VegaRule")
    };
    data
}

fn axis_positions(data: &VegaRuleData, x_axis: bool) -> Vec<ErrorPosition> {
    data.segments
        .iter()
        .flat_map(|segment| {
            if x_axis {
                [segment.x1, segment.x2]
            } else {
                [segment.y1, segment.y2]
            }
        })
        .map(|position| match position {
            VegaRulePosition::FullAxisStart
            | VegaRulePosition::FullAxisCenter
            | VegaRulePosition::FullAxisEnd => ErrorPosition::FullAxis,
            VegaRulePosition::Category(index) => ErrorPosition::Category(index),
            VegaRulePosition::Quantitative(value) => ErrorPosition::Quantitative(value),
            VegaRulePosition::Temporal(millis) => ErrorPosition::Temporal(millis),
        })
        .collect()
}

pub(crate) fn compute_frame(spec: &ChartSpec, measurer: &TextMeasurer<'_>) -> ErrorMarkFrame {
    let data = rule_data(spec);
    let x_positions = axis_positions(data, true);
    let y_positions = axis_positions(data, false);
    error_mark::compute_frame_for_positions(
        spec,
        measurer,
        &x_positions,
        &data.x_categories,
        &y_positions,
        &data.y_categories,
    )
}

pub(crate) fn plot_rect(spec: &ChartSpec, measurer: &TextMeasurer<'_>) -> (f64, f64, f64, f64) {
    let frame = compute_frame(spec, measurer);
    (
        frame.plot_left,
        frame.plot_top,
        frame.plot_right,
        frame.plot_bottom,
    )
}

fn build_with_frame(
    spec: &ChartSpec,
    measurer: &TextMeasurer<'_>,
    frame: &ErrorMarkFrame,
) -> Result<(Scene, usize), String> {
    let data = rule_data(spec);
    let mut scene = error_mark::build_axes_scene(spec, measurer, frame);
    let mut lines = Vec::with_capacity(data.segments.len());
    for (index, segment) in data.segments.iter().enumerate() {
        let x1 = frame
            .map_rule_position(ErrorAxis::X, segment.x1)
            .map_err(|error| format!("rule segment {index} start x: {error}"))?;
        let y1 = frame
            .map_rule_position(ErrorAxis::Y, segment.y1)
            .map_err(|error| format!("rule segment {index} start y: {error}"))?;
        let x2 = frame
            .map_rule_position(ErrorAxis::X, segment.x2)
            .map_err(|error| format!("rule segment {index} end x: {error}"))?;
        let y2 = frame
            .map_rule_position(ErrorAxis::Y, segment.y2)
            .map_err(|error| format!("rule segment {index} end y: {error}"))?;
        lines.push(Prim::Line {
            x1,
            y1,
            x2,
            y2,
            stroke: segment.color,
            stroke_width: data.stroke_width,
            dash: data.stroke_dash.clone(),
        });
    }
    let mark_count = if data.clip { 1 } else { lines.len() };
    if data.clip {
        scene.items.push(Prim::Group {
            translate_x: 0.0,
            translate_y: 0.0,
            clip: Some(Box::new(ClipRect {
                x: frame.plot_left,
                y: frame.plot_top,
                w: (frame.plot_right - frame.plot_left).max(0.0),
                h: (frame.plot_bottom - frame.plot_top).max(0.0),
            })),
            children: lines,
        });
    } else {
        scene.items.extend(lines);
    }
    Ok((scene, mark_count))
}

pub(crate) fn build_checked_with_layer_parts(
    spec: &ChartSpec,
    measurer: &TextMeasurer<'_>,
    limits: &InputLimits,
) -> Result<(Scene, usize), String> {
    validate_vega_rule(spec, limits)?;
    let frame = compute_frame(spec, measurer);
    build_with_frame(spec, measurer, &frame)
}

pub(crate) fn build_checked(
    spec: &ChartSpec,
    measurer: &TextMeasurer<'_>,
    limits: &InputLimits,
) -> Result<Scene, String> {
    build_checked_with_layer_parts(spec, measurer, limits).map(|(scene, _)| scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranged_rule_without_orthogonal_channel_uses_the_plot_center() {
        let measurer = TextMeasurer::new(crate::font::TEST_FONT).unwrap();
        for (json, axis) in [
            (
                r##"{"mark":"rule","data":{"values":[{"x":10,"x2":30}]},"encoding":{"x":{"field":"x","type":"quantitative"},"x2":{"field":"x2"}}}"##,
                ErrorAxis::Y,
            ),
            (
                r##"{"mark":"rule","data":{"values":[{"y":10,"y2":30}]},"encoding":{"y":{"field":"y","type":"quantitative"},"y2":{"field":"y2"}}}"##,
                ErrorAxis::X,
            ),
        ] {
            let spec = crate::frontend::vegalite::parse(json, true)
                .expect("one-axis ranged rules should parse");
            let frame = compute_frame(&spec, &measurer);
            let data = rule_data(&spec);
            let segment = &data.segments[0];
            let position = if axis == ErrorAxis::X {
                segment.x1
            } else {
                segment.y1
            };
            let mapped = frame
                .map_rule_position(axis, position)
                .expect("default orthogonal position should map");
            let expected = match axis {
                ErrorAxis::X => (frame.plot_left + frame.plot_right) / 2.0,
                ErrorAxis::Y => (frame.plot_top + frame.plot_bottom) / 2.0,
            };

            assert!((mapped - expected).abs() < 1e-8);
        }
    }
}
