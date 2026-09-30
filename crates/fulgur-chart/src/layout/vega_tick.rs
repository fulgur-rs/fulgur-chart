//! Shared Scene layout for Vega-Lite `tick` marks.

use crate::guard::{InputLimits, validate_vega_tick};
use crate::ir::{
    ChartKind, ChartSpec, ErrorPosition, VegaTickData, VegaTickOrient, VegaTickPosition,
    VegaTickSize,
};
use crate::layout::error_mark::{self, ErrorAxis, ErrorMarkFrame};
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

const DEFAULT_DISCRETE_STEP: f64 = 20.0;
const MIN_SIZE_SCALE: f64 = 2.0;
const MAX_SIZE_SCALE: f64 = DEFAULT_DISCRETE_STEP - 1.0;

fn tick_data(spec: &ChartSpec) -> &VegaTickData {
    let ChartKind::VegaTick(data) = &spec.kind else {
        unreachable!("Vega-Lite tick layout only accepts ChartKind::VegaTick")
    };
    data
}

fn error_position(position: VegaTickPosition) -> ErrorPosition {
    match position {
        VegaTickPosition::Center => ErrorPosition::FullAxis,
        VegaTickPosition::Category(index) => ErrorPosition::Category(index),
        VegaTickPosition::Quantitative(value) => ErrorPosition::Quantitative(value),
        VegaTickPosition::Temporal(value) => ErrorPosition::Temporal(value),
    }
}

fn frame_positions(data: &VegaTickData, x_axis: bool) -> Vec<ErrorPosition> {
    data.marks
        .iter()
        .map(|mark| error_position(if x_axis { mark.x } else { mark.y }))
        .collect()
}

pub(crate) fn compute_frame(spec: &ChartSpec, measurer: &TextMeasurer<'_>) -> ErrorMarkFrame {
    let data = tick_data(spec);
    error_mark::compute_frame_for_positions(
        spec,
        measurer,
        &frame_positions(data, true),
        &data.x_categories,
        &frame_positions(data, false),
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

fn default_band_size(data: &VegaTickData, frame: &ErrorMarkFrame) -> f64 {
    if let Some(size) = data.band_size {
        return size;
    }
    let (categories, span) = match data.orient {
        VegaTickOrient::Horizontal => (data.x_categories.len(), frame.plot_right - frame.plot_left),
        VegaTickOrient::Vertical => (data.y_categories.len(), frame.plot_bottom - frame.plot_top),
    };
    let step = if categories > 0 {
        span / categories as f64
    } else {
        DEFAULT_DISCRETE_STEP
    };
    (step * 0.75).max(0.0)
}

fn tick_length(size: VegaTickSize, default: f64) -> f64 {
    match size {
        VegaTickSize::Default => default,
        VegaTickSize::Pixels(value) => value,
        VegaTickSize::Scaled(value) => {
            MIN_SIZE_SCALE + value.clamp(0.0, 1.0) * (MAX_SIZE_SCALE - MIN_SIZE_SCALE)
        }
    }
}

fn build_with_frame(
    spec: &ChartSpec,
    measurer: &TextMeasurer<'_>,
    frame: &ErrorMarkFrame,
) -> Result<(Scene, usize), String> {
    let data = tick_data(spec);
    let mut scene = error_mark::build_axes_scene(spec, measurer, frame);
    let band_size = default_band_size(data, frame);
    let mut marks = Vec::with_capacity(data.marks.len());
    for (index, mark) in data.marks.iter().enumerate() {
        let x = frame
            .map_position(ErrorAxis::X, error_position(mark.x))
            .map_err(|error| format!("tick mark {index} x: {error}"))?;
        let y = frame
            .map_position(ErrorAxis::Y, error_position(mark.y))
            .map_err(|error| format!("tick mark {index} y: {error}"))?;
        let length = tick_length(mark.size, band_size);
        let rect = match data.orient {
            VegaTickOrient::Horizontal => Prim::Rect {
                x: x - length / 2.0,
                y: y - data.thickness / 2.0,
                w: length,
                h: data.thickness,
                fill: mark.fill,
            },
            VegaTickOrient::Vertical => Prim::Rect {
                x: x - data.thickness / 2.0,
                y: y - length / 2.0,
                w: data.thickness,
                h: length,
                fill: mark.fill,
            },
        };
        marks.push(rect);
    }
    let mark_count = marks.len();
    scene.items.extend(marks);
    Ok((scene, mark_count))
}

pub(crate) fn build_checked_with_layer_parts(
    spec: &ChartSpec,
    measurer: &TextMeasurer<'_>,
    limits: &InputLimits,
) -> Result<(Scene, usize), String> {
    validate_vega_tick(spec, limits)?;
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
