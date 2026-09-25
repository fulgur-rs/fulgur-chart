//! Violin chart layout.

use crate::ir::{AxisSpec, ChartKind, ChartSpec};
use crate::layout::common::{self, Frame};
use crate::scale::{NiceTicks, ValueScale};
use crate::scene::{ClipRect, Prim, Scene};
use crate::text::TextMeasurer;

const DENSITY_POSITIONS: usize = 100;
const MARKER_RADIUS: f64 = 2.5;
const DIAMOND_VALUE_RADIUS: f64 = 3.5;
const DIAMOND_CATEGORY_RADIUS: f64 = 2.0;
const MIN_FALLBACK_BANDWIDTH: f64 = 1e-9;

pub(crate) struct ViolinFrame {
    pub(crate) horizontal: bool,
    pub(crate) scene_width: f64,
    pub(crate) scene_height: f64,
    pub(crate) plot_left: f64,
    pub(crate) plot_right: f64,
    pub(crate) plot_top: f64,
    pub(crate) plot_bottom: f64,
    pub(crate) ticks: NiceTicks,
    pub(crate) value_scale: ValueScale,
}

fn is_horizontal(spec: &ChartSpec) -> bool {
    matches!(spec.kind, ChartKind::Violin { horizontal: true })
}

fn extend_suggested_bounds(axis: &mut AxisSpec, data_min: f64, data_max: f64) {
    if data_min.is_finite() {
        axis.suggested_min = Some(
            axis.suggested_min
                .filter(|value| value.is_finite())
                .unwrap_or(data_min)
                .min(data_min),
        );
    }
    if data_max.is_finite() {
        axis.suggested_max = Some(
            axis.suggested_max
                .filter(|value| value.is_finite())
                .unwrap_or(data_max)
                .max(data_max),
        );
    }
}

/// Prepare the value range from observations. Horizontal bar layout is reused only for its
/// category/value axes, legend, and bounds; its NaN values intentionally produce no bars.
fn auxiliary_spec(spec: &ChartSpec, horizontal: bool) -> ChartSpec {
    let mut aux = spec.clone();
    let mut data_min = f64::INFINITY;
    let mut data_max = f64::NEG_INFINITY;
    for (source, target) in spec.series.iter().zip(&mut aux.series) {
        let finite = source
            .violin_samples
            .iter()
            .flat_map(|group| group.iter().copied().flatten())
            .filter(|value| value.is_finite())
            .collect::<Vec<_>>();
        for &value in &finite {
            data_min = data_min.min(value);
            data_max = data_max.max(value);
        }
        target.values = if horizontal {
            vec![f64::NAN; spec.categories.len()]
        } else {
            finite
        };
    }
    if horizontal {
        extend_suggested_bounds(&mut aux.x_axis, data_min, data_max);
        aux.kind = ChartKind::Bar {
            horizontal: true,
            placement_stacked: false,
            value_stacked: false,
        };
    } else {
        extend_suggested_bounds(&mut aux.y_axis, data_min, data_max);
    }

    // Include the KDE interval for degenerate groups in the automatic domain. Hard min/max
    // bounds still take precedence over these suggestions.
    let value_axis = if horizontal { &aux.x_axis } else { &aux.y_axis };
    let (domain_min, domain_max) = common::value_domain(&aux, value_axis);
    let ticks = common::configured_axis_ticks(domain_min, domain_max, value_axis);
    let mut fallback_min = f64::INFINITY;
    let mut fallback_max = f64::NEG_INFINITY;
    for series in &spec.series {
        for group in &series.violin_samples {
            let samples = finite_samples(group);
            let bandwidth = normal_reference_bandwidth(&samples);
            if samples.is_empty() || (bandwidth.is_finite() && bandwidth > 0.0) {
                continue;
            }
            let mean = stable_mean(&samples);
            let radius = fallback_bandwidth(&ticks) * 3.0;
            fallback_min = fallback_min.min((mean - radius).max(-f64::MAX));
            fallback_max = fallback_max.max((mean + radius).min(f64::MAX));
        }
    }
    if horizontal {
        extend_suggested_bounds(&mut aux.x_axis, fallback_min, fallback_max);
    } else {
        extend_suggested_bounds(&mut aux.y_axis, fallback_min, fallback_max);
    }
    aux
}

fn vertical_frame(spec: &ChartSpec, m: &TextMeasurer) -> Frame {
    common::compute(spec, m)
}

fn violin_frame_from_vertical(frame: &Frame) -> ViolinFrame {
    ViolinFrame {
        horizontal: false,
        scene_width: frame.scene_width,
        scene_height: frame.scene_height,
        plot_left: frame.plot_left,
        plot_right: frame.plot_right,
        plot_top: frame.plot_top,
        plot_bottom: frame.plot_bottom,
        ticks: frame.ticks.clone(),
        value_scale: frame.ys.clone(),
    }
}

fn horizontal_frame(spec: &ChartSpec, m: &TextMeasurer) -> ViolinFrame {
    let layout = crate::layout::bar::horizontal_bar_layout(spec, m);
    ViolinFrame {
        horizontal: true,
        scene_width: spec.width,
        scene_height: spec.height,
        plot_left: layout.plot_left,
        plot_right: layout.plot_right,
        plot_top: layout.plot_top,
        plot_bottom: layout.plot_bottom,
        ticks: layout.value_ticks,
        value_scale: layout.value_scale,
    }
}

fn finite_samples(group: &[Option<f64>]) -> Vec<f64> {
    group
        .iter()
        .filter_map(|sample| *sample)
        .filter(|sample| sample.is_finite())
        .collect()
}

fn stable_mean(samples: &[f64]) -> f64 {
    let scale = samples.iter().map(|value| value.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return 0.0;
    }
    let normalized_mean =
        samples.iter().map(|value| value / scale).sum::<f64>() / samples.len() as f64;
    let mean = normalized_mean * scale;
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    mean.clamp(min, max)
}

fn quantile(sorted: &[f64], probability: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let position = (sorted.len() - 1) as f64 * probability;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    sorted[lower] * (1.0 - fraction) + sorted[upper] * fraction
}

fn sample_standard_deviation(samples: &[f64]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let scale = samples.iter().map(|value| value.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return 0.0;
    }
    let normalized_mean =
        samples.iter().map(|value| value / scale).sum::<f64>() / samples.len() as f64;
    let normalized_variance = samples
        .iter()
        .map(|value| {
            let delta = value / scale - normalized_mean;
            delta * delta
        })
        .sum::<f64>()
        / (samples.len() - 1) as f64;
    normalized_variance.sqrt() * scale
}

fn normal_reference_bandwidth(samples: &[f64]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let standard_deviation = sample_standard_deviation(samples);
    let interquartile_range = quantile(&sorted, 0.75) - quantile(&sorted, 0.25);
    1.06 * standard_deviation.min(interquartile_range / 1.34) * (samples.len() as f64).powf(-0.2)
}

fn fallback_bandwidth(ticks: &NiceTicks) -> f64 {
    let span = ticks.max - ticks.min;
    let scale = if span.is_finite() {
        span.abs()
    } else {
        ticks.max.abs().max(ticks.min.abs())
    };
    (scale * 0.01).clamp(MIN_FALLBACK_BANDWIDTH, f64::MAX / 8.0)
}

fn interpolate(a: f64, b: f64, t: f64) -> f64 {
    (a * (1.0 - t) + b * t).clamp(-f64::MAX, f64::MAX)
}

fn density_samples(samples: &[f64], ticks: &NiceTicks) -> Vec<(f64, f64)> {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let mean = stable_mean(samples);
    let bandwidth = normal_reference_bandwidth(samples);
    let (bandwidth, eval_min, eval_max) = if bandwidth.is_finite() && bandwidth > 0.0 {
        (bandwidth, min, max)
    } else {
        let fallback = fallback_bandwidth(ticks);
        let radius = fallback * 3.0;
        if min < max {
            // A non-constant group can have zero IQR. Keep its evaluation range on its observed
            // minimum and maximum; only replace its unusable bandwidth.
            (fallback, min, max)
        } else {
            (
                fallback,
                (mean - radius).max(-f64::MAX),
                (mean + radius).min(f64::MAX),
            )
        }
    };
    let mut density = (0..DENSITY_POSITIONS)
        .map(|index| {
            let t = index as f64 / (DENSITY_POSITIONS - 1) as f64;
            let value = interpolate(eval_min, eval_max, t);
            let estimate = samples
                .iter()
                .map(|sample| {
                    let z = (value - sample) / bandwidth;
                    (-0.5 * z * z).exp()
                })
                .sum::<f64>();
            (value, estimate)
        })
        .collect::<Vec<_>>();
    let maximum = density
        .iter()
        .map(|(_, estimate)| *estimate)
        .fold(0.0, f64::max);
    if maximum.is_finite() && maximum > 0.0 {
        for (_, estimate) in &mut density {
            *estimate /= maximum;
        }
    } else {
        for (_, estimate) in &mut density {
            *estimate = 0.0;
        }
        density[DENSITY_POSITIONS / 2].1 = 1.0;
    }
    density
}

fn median(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    quantile(&sorted, 0.5)
}

fn category_center(frame: &ViolinFrame, index: usize, count: usize) -> f64 {
    if frame.horizontal {
        let band = (frame.plot_bottom - frame.plot_top) / count.max(1) as f64;
        frame.plot_top + (index as f64 + 0.5) * band
    } else {
        frame.plot_left
            + (index as f64 + 0.5) * (frame.plot_right - frame.plot_left) / count.max(1) as f64
    }
}

fn category_band(frame: &ViolinFrame, count: usize) -> f64 {
    if frame.horizontal {
        (frame.plot_bottom - frame.plot_top) / count.max(1) as f64
    } else {
        (frame.plot_right - frame.plot_left) / count.max(1) as f64
    }
}

fn path_from_points(points: &[(f64, f64)]) -> String {
    let mut path = String::new();
    for (index, (x, y)) in points.iter().enumerate() {
        use std::fmt::Write;
        let command = if index == 0 { "M" } else { "L" };
        let _ = write!(
            path,
            "{command} {} {} ",
            crate::num::fmt_num(*x),
            crate::num::fmt_num(*y)
        );
    }
    path.push('Z');
    path
}

fn intersection(a: (f64, f64), b: (f64, f64), axis: usize, boundary: f64) -> (f64, f64) {
    let a_value = if axis == 0 { a.0 } else { a.1 };
    let b_value = if axis == 0 { b.0 } else { b.1 };
    let scale = a_value.abs().max(b_value.abs()).max(boundary.abs());
    let t = if scale == 0.0 {
        0.0
    } else {
        ((boundary / scale) - (a_value / scale)) / ((b_value / scale) - (a_value / scale))
    }
    .clamp(0.0, 1.0);
    let mut point = (interpolate(a.0, b.0, t), interpolate(a.1, b.1, t));
    if axis == 0 {
        point.0 = boundary;
    } else {
        point.1 = boundary;
    }
    point
}

fn clip_polygon_edge(
    points: &[(f64, f64)],
    axis: usize,
    boundary: f64,
    keep_greater: bool,
) -> Vec<(f64, f64)> {
    if points.is_empty() {
        return Vec::new();
    }
    let value = |point: (f64, f64)| if axis == 0 { point.0 } else { point.1 };
    let inside = |point: (f64, f64)| {
        if keep_greater {
            value(point) >= boundary
        } else {
            value(point) <= boundary
        }
    };
    let mut output = Vec::with_capacity(points.len() + 2);
    let mut previous = *points.last().expect("non-empty polygon");
    let mut previous_inside = inside(previous);
    for &current in points {
        let current_inside = inside(current);
        if current_inside != previous_inside {
            output.push(intersection(previous, current, axis, boundary));
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn clip_polygon_to_rect(
    mut points: Vec<(f64, f64)>,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
) -> Vec<(f64, f64)> {
    if left > right || top > bottom {
        return Vec::new();
    }
    points = clip_polygon_edge(&points, 0, left, true);
    points = clip_polygon_edge(&points, 0, right, false);
    points = clip_polygon_edge(&points, 1, top, true);
    clip_polygon_edge(&points, 1, bottom, false)
}

fn stroke_inset(stroke_width: f64) -> f64 {
    if stroke_width.is_finite() {
        stroke_width.max(0.0) / 2.0
    } else {
        0.0
    }
}

fn plot_clip_rect(frame: &ViolinFrame) -> (f64, f64, f64, f64) {
    (
        frame.plot_left,
        frame.plot_right,
        frame.plot_top,
        frame.plot_bottom,
    )
}

const MAX_MITER_STROKE_EXTENSION: f64 = 2.0;

/// Leave enough geometry for a miter-limited stroke to reach the exact renderer clip.
fn stroke_clip_margin(stroke_width: f64) -> f64 {
    if !stroke_width.is_finite() {
        return 1.0;
    }
    stroke_width.clamp(0.0, f64::MAX / MAX_MITER_STROKE_EXTENSION) * MAX_MITER_STROKE_EXTENSION
        + 1.0
}

fn expanded_value_bounds(frame: &ViolinFrame, margin: f64) -> (f64, f64) {
    let (left, right, top, bottom) = plot_clip_rect(frame);
    let (first_pixel, last_pixel) = if frame.horizontal {
        (left - margin, right + margin)
    } else {
        (bottom + margin, top - margin)
    };
    let first = frame.value_scale.unmap(first_pixel);
    let last = frame.value_scale.unmap(last_pixel);
    let lower = first.min(last);
    let upper = first.max(last);
    (
        if lower.is_finite() { lower } else { -f64::MAX },
        if upper.is_finite() { upper } else { f64::MAX },
    )
}

fn clipped_path(
    frame: &ViolinFrame,
    d: String,
    fill: Option<crate::ir::Color>,
    stroke: Option<crate::ir::Color>,
    stroke_width: f64,
) -> Prim {
    let (left, right, top, bottom) = plot_clip_rect(frame);
    Prim::ClippedPath {
        d,
        fill,
        stroke,
        stroke_width,
        clip: Box::new(ClipRect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        }),
    }
}

fn body_path(
    frame: &ViolinFrame,
    center: f64,
    half_width: f64,
    stroke_width: f64,
    samples: &[f64],
) -> Option<String> {
    let densities = density_samples(samples, &frame.ticks);
    let mut points = Vec::with_capacity(DENSITY_POSITIONS * 2);
    for &(value, density) in &densities {
        let half = half_width * density.clamp(0.0, 1.0);
        points.push(if frame.horizontal {
            (value, center - half)
        } else {
            (center + half, value)
        });
    }
    for &(value, density) in densities.iter().rev() {
        let half = half_width * density.clamp(0.0, 1.0);
        points.push(if frame.horizontal {
            (value, center + half)
        } else {
            (center - half, value)
        });
    }
    // Bound coordinates before mapping, but retain the full renderer stroke extent around the
    // plot. The renderer applies the exact clip after both fill and stroke, avoiding extra joins
    // on clipped edges and keeping a stroke visible when the body lies just outside hard bounds.
    let margin = stroke_clip_margin(stroke_width);
    let (value_min, value_max) = expanded_value_bounds(frame, margin);
    let (left, right, top, bottom) = plot_clip_rect(frame);
    points = if frame.horizontal {
        clip_polygon_to_rect(points, value_min, value_max, top - margin, bottom + margin)
    } else {
        clip_polygon_to_rect(points, left - margin, right + margin, value_min, value_max)
    };
    for point in &mut points {
        if frame.horizontal {
            point.0 = frame.value_scale.map(point.0);
        } else {
            point.1 = frame.value_scale.map(point.1);
        }
    }
    (points.len() >= 3).then(|| path_from_points(&points))
}

fn marker_value_position(frame: &ViolinFrame, value: f64, radius: f64) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let (left, right, top, bottom) = plot_clip_rect(frame);
    let (min_pixel, max_pixel) = if frame.horizontal {
        (left - radius, right + radius)
    } else {
        (bottom + radius, top - radius)
    };
    let min_value = frame.value_scale.unmap(min_pixel);
    let max_value = frame.value_scale.unmap(max_pixel);
    let lower = min_value.min(max_value);
    let upper = min_value.max(max_value);
    if value < lower || value > upper {
        return None;
    }
    let position = frame.value_scale.map(value);
    position.is_finite().then_some(position)
}

fn diamond_path(
    frame: &ViolinFrame,
    category: f64,
    value: f64,
    stroke_width: f64,
) -> Option<String> {
    let value = marker_value_position(
        frame,
        value,
        DIAMOND_VALUE_RADIUS + stroke_clip_margin(stroke_width),
    )?;
    let (center_x, center_y) = if frame.horizontal {
        (value, category)
    } else {
        (category, value)
    };
    let value_radius = DIAMOND_VALUE_RADIUS;
    let category_radius = DIAMOND_CATEGORY_RADIUS;
    let points = if frame.horizontal {
        vec![
            (center_x - value_radius, center_y),
            (center_x, center_y - category_radius),
            (center_x + value_radius, center_y),
            (center_x, center_y + category_radius),
        ]
    } else {
        vec![
            (center_x, center_y - value_radius),
            (center_x + category_radius, center_y),
            (center_x, center_y + value_radius),
            (center_x - category_radius, center_y),
        ]
    };
    Some(path_from_points(&points))
}

fn circle_points(cx: f64, cy: f64, radius: f64) -> Vec<(f64, f64)> {
    const CIRCLE_POINTS: usize = 32;
    (0..CIRCLE_POINTS)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / CIRCLE_POINTS as f64;
            (cx + radius * angle.cos(), cy + radius * angle.sin())
        })
        .collect()
}

fn add_markers(
    items: &mut Vec<Prim>,
    frame: &ViolinFrame,
    series: &crate::ir::Series,
    index: usize,
    category: f64,
    samples: &[f64],
) {
    let fill = series.fill_at(index);
    let stroke = series.stroke_at(index);
    let Some(mean) = marker_value_position(
        frame,
        stable_mean(samples),
        MARKER_RADIUS + stroke_clip_margin(series.stroke_width),
    ) else {
        if let Some(d) = diamond_path(frame, category, median(samples), series.stroke_width) {
            items.push(clipped_path(
                frame,
                d,
                Some(fill),
                Some(stroke),
                series.stroke_width,
            ));
        }
        return;
    };
    let (cx, cy) = if frame.horizontal {
        (mean, category)
    } else {
        (category, mean)
    };
    if cx - MARKER_RADIUS >= frame.plot_left + stroke_inset(series.stroke_width)
        && cx + MARKER_RADIUS <= frame.plot_right - stroke_inset(series.stroke_width)
        && cy - MARKER_RADIUS >= frame.plot_top + stroke_inset(series.stroke_width)
        && cy + MARKER_RADIUS <= frame.plot_bottom - stroke_inset(series.stroke_width)
    {
        items.push(Prim::Circle {
            cx,
            cy,
            r: MARKER_RADIUS,
            fill,
            stroke,
            stroke_width: series.stroke_width,
        });
    } else {
        items.push(clipped_path(
            frame,
            path_from_points(&circle_points(cx, cy, MARKER_RADIUS)),
            Some(fill),
            Some(stroke),
            series.stroke_width,
        ));
    }
    if let Some(d) = diamond_path(frame, category, median(samples), series.stroke_width) {
        items.push(clipped_path(
            frame,
            d,
            Some(fill),
            Some(stroke),
            series.stroke_width,
        ));
    }
}

pub(crate) fn compute_frame(spec: &ChartSpec, m: &TextMeasurer) -> ViolinFrame {
    let horizontal = is_horizontal(spec);
    let aux = auxiliary_spec(spec, horizontal);
    if horizontal {
        horizontal_frame(&aux, m)
    } else {
        violin_frame_from_vertical(&vertical_frame(&aux, m))
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let horizontal = is_horizontal(spec);
    let aux = auxiliary_spec(spec, horizontal);
    let (mut scene, frame) = if horizontal {
        (
            crate::layout::bar::build(&aux, m),
            horizontal_frame(&aux, m),
        )
    } else {
        let common_frame = vertical_frame(&aux, m);
        let frame = violin_frame_from_vertical(&common_frame);
        let mut items = Vec::new();
        common::draw_frame(&mut items, spec, &common_frame, m);
        (
            Scene {
                width: frame.scene_width,
                height: frame.scene_height,
                items,
            },
            frame,
        )
    };

    let category_count = spec.categories.len().max(1);
    let series_count = spec.series.len().max(1);
    let category_band = category_band(&frame, category_count);
    let dataset_band = category_band / series_count as f64;
    let half_width = dataset_band * 0.45;
    let group_offset = -(series_count as f64 - 1.0) / 2.0;
    for (series_index, series) in spec.series.iter().enumerate() {
        for category_index in 0..spec.categories.len() {
            let Some(group) = series.violin_samples.get(category_index) else {
                continue;
            };
            let samples = finite_samples(group);
            if samples.is_empty() {
                continue;
            }
            let category = category_center(&frame, category_index, category_count)
                + (group_offset + series_index as f64) * dataset_band;
            if let Some(d) = body_path(&frame, category, half_width, series.stroke_width, &samples)
            {
                scene.items.push(clipped_path(
                    &frame,
                    d,
                    Some(series.fill_at(category_index)),
                    Some(series.stroke_at(category_index)),
                    series.stroke_width,
                ));
            }
            add_markers(
                &mut scene.items,
                &frame,
                series,
                category_index,
                category,
                &samples,
            );
        }
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TEST_FONT;
    use crate::frontend::chartjs;
    use crate::ir::Color;
    use crate::scene::Prim;

    fn parse(json: &str) -> ChartSpec {
        chartjs::parse(json, false).expect("violin input parses")
    }

    fn measurer() -> TextMeasurer<'static> {
        TextMeasurer::new(TEST_FONT).unwrap()
    }

    fn body_paths(scene: &Scene) -> Vec<(&str, Color)> {
        scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Path {
                    d,
                    fill: Some(fill),
                    ..
                }
                | Prim::ClippedPath {
                    d,
                    fill: Some(fill),
                    ..
                } if d.matches("L ").count() > 100 => Some((d.as_str(), *fill)),
                _ => None,
            })
            .collect()
    }

    fn path_points(d: &str) -> Vec<(f64, f64)> {
        let mut tokens = d.split_whitespace();
        let mut points = Vec::new();
        while let Some(command) = tokens.next() {
            match command {
                "M" | "L" => {
                    let x = tokens.next().unwrap().parse::<f64>().unwrap();
                    let y = tokens.next().unwrap().parse::<f64>().unwrap();
                    points.push((x, y));
                }
                "Z" => {}
                other => panic!("unexpected path token {other}"),
            }
        }
        points
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 0.02, "{a} != {b}");
    }

    #[test]
    fn violin_body_path_is_closed_and_symmetric() {
        let spec = parse(
            r#"{"type":"violin","data":{"labels":["A"],"datasets":[{"data":[[1,2,3,4,5]]}]}}"#,
        );
        let scene = build(&spec, &measurer());
        let (d, _) = body_paths(&scene)
            .into_iter()
            .next()
            .expect("filled body path");
        assert!(d.ends_with('Z'), "body path must be closed: {d}");
        let points = path_points(d);
        assert_eq!(points.len(), 200, "100 points on each side");
        let center = (points
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min)
            + points
                .iter()
                .map(|point| point.0)
                .fold(f64::NEG_INFINITY, f64::max))
            / 2.0;
        for i in 0..100 {
            let mirrored = points[199 - i];
            close(points[i].0 + mirrored.0, center * 2.0);
            close(points[i].1, mirrored.1);
        }
    }

    #[test]
    fn violin_datasets_use_separate_category_slots() {
        let spec = parse(
            r##"{"type":"violin","data":{"labels":["A"],"datasets":[
            {"label":"one","data":[[1,2,3,4,5]],"backgroundColor":"#ff0000"},
            {"label":"two","data":[[2,3,4,5,6]],"backgroundColor":"#0000ff"}
        ]}}"##,
        );
        let scene = build(&spec, &measurer());
        let bodies = body_paths(&scene);
        assert_eq!(bodies.len(), 2);
        assert_ne!(bodies[0].1, bodies[1].1);
        let centers = bodies
            .iter()
            .map(|(d, _)| {
                let points = path_points(d);
                let lo = points
                    .iter()
                    .map(|point| point.0)
                    .fold(f64::INFINITY, f64::min);
                let hi = points
                    .iter()
                    .map(|point| point.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                (lo + hi) / 2.0
            })
            .collect::<Vec<_>>();
        assert!(
            centers[0] < centers[1],
            "dataset slot centers should preserve order: {centers:?}"
        );
    }

    #[test]
    fn violin_mean_and_median_markers_follow_value_axis_orientation() {
        for (chart_type, horizontal) in [("violin", false), ("horizontalViolin", true)] {
            let json = format!(
                r#"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[1,2,3,4,6]]}}]}}}}"#
            );
            let spec = parse(&json);
            let measurer = measurer();
            let frame = compute_frame(&spec, &measurer);
            let scene = build(&spec, &measurer);
            let mean = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
                    _ => None,
                })
                .expect("mean circle");
            let category_center = if horizontal {
                (frame.plot_top + frame.plot_bottom) / 2.0
            } else {
                (frame.plot_left + frame.plot_right) / 2.0
            };
            let expected_value = frame.value_scale.map(3.2);
            if horizontal {
                close(mean.0, expected_value);
                close(mean.1, category_center);
            } else {
                close(mean.0, category_center);
                close(mean.1, expected_value);
            }
            let median = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::Path { d, .. } | Prim::ClippedPath { d, .. }
                        if d.matches("L ").count() == 3 =>
                    {
                        Some(path_points(d))
                    }
                    _ => None,
                })
                .expect("median diamond");
            let median_center = (
                median.iter().map(|point| point.0).sum::<f64>() / median.len() as f64,
                median.iter().map(|point| point.1).sum::<f64>() / median.len() as f64,
            );
            let expected_median = frame.value_scale.map(3.0);
            if horizontal {
                close(median_center.0, expected_median);
                close(median_center.1, category_center);
            } else {
                close(median_center.0, category_center);
                close(median_center.1, expected_median);
            }
        }
    }

    #[test]
    fn horizontal_violin_median_aligns_with_log_and_temporal_axis_gridlines() {
        let cases = [
            r#"{"type":"horizontalViolin","data":{"labels":["A"],"datasets":[{"data":[[1,10,100,10,10]]}]},"options":{"scales":{"x":{"type":"logarithmic","min":1,"max":100}}}}"#,
            r#"{"type":"horizontalViolin","data":{"labels":["A"],"datasets":[{"data":[[1704067200000,1706745600000,1706745600000,1706745600000,1711929600000]]}]},"options":{"scales":{"x":{"type":"time","min":1704067200000,"max":1711929600000,"time":{"unit":"month"}}}}}"#,
        ];

        for json in cases {
            let spec = parse(json);
            let expected_scale = if json.contains("logarithmic") {
                crate::ir::ScaleKind::Logarithmic
            } else {
                crate::ir::ScaleKind::Time
            };
            assert_eq!(spec.x_axis.scale_kind, expected_scale, "{json}");
            let measurer = measurer();
            let frame = compute_frame(&spec, &measurer);
            if spec.x_axis.scale_kind == crate::ir::ScaleKind::Logarithmic {
                close(
                    frame.value_scale.map(10.0),
                    (frame.plot_left + frame.plot_right) / 2.0,
                );
            }
            let scene = build(&spec, &measurer);
            let median_x = scene
                .items
                .iter()
                .find_map(|item| match item {
                    Prim::ClippedPath { d, .. } if d.matches("L ").count() == 3 => {
                        let points = path_points(d);
                        Some(points.iter().map(|point| point.0).sum::<f64>() / points.len() as f64)
                    }
                    _ => None,
                })
                .expect("median path");
            let aligned = scene.items.iter().any(|item| match item {
                Prim::Line { x1, y1, x2, y2, .. }
                    if (*x1 - *x2).abs() < 0.01
                        && (*y1 - frame.plot_top).abs() < 0.01
                        && (*y2 - frame.plot_bottom).abs() < 0.01 =>
                {
                    (*x1 - median_x).abs() < 0.02
                }
                _ => false,
            });
            assert!(
                aligned,
                "median {median_x} should align with a value-axis gridline: {json}"
            );
        }
    }

    #[test]
    fn violin_value_domain_covers_observations_in_both_orientations() {
        for chart_type in ["violin", "horizontalViolin"] {
            let json = format!(
                r#"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[-12,3,97]]}}]}}}}"#
            );
            let spec = parse(&json);
            let frame = compute_frame(&spec, &measurer());
            assert!(
                frame.ticks.min <= -12.0,
                "minimum {} excludes data",
                frame.ticks.min
            );
            assert!(
                frame.ticks.max >= 97.0,
                "maximum {} excludes data",
                frame.ticks.max
            );
        }
    }

    #[test]
    fn violin_hard_value_bounds_clip_geometry_in_both_orientations() {
        for (chart_type, axis) in [("violin", "y"), ("horizontalViolin", "x")] {
            let json = format!(
                r##"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[-12,3,97]]}}]}},"options":{{"scales":{{"{axis}":{{"min":0,"max":10}}}}}}}}"##
            );
            let spec = parse(&json);
            let measurer = measurer();
            let frame = compute_frame(&spec, &measurer);
            let scene = build(&spec, &measurer);
            close(frame.ticks.min, 0.0);
            close(frame.ticks.max, 10.0);
            let margin = stroke_clip_margin(spec.series[0].stroke_width);
            for (d, _) in body_paths(&scene) {
                for (x, y) in path_points(d) {
                    assert!((frame.plot_left - margin..=frame.plot_right + margin).contains(&x));
                    assert!((frame.plot_top - margin..=frame.plot_bottom + margin).contains(&y));
                }
            }
        }
    }

    #[test]
    fn violin_hard_bounds_clip_strokes_and_skip_fully_outside_groups() {
        for (chart_type, axis) in [("violin", "y"), ("horizontalViolin", "x")] {
            let crossing_json = format!(
                r##"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[-12,3,17]],"backgroundColor":"#ff0000","borderColor":"#ff0000","borderWidth":8}}]}},"options":{{"scales":{{"{axis}":{{"min":0,"max":10}}}}}}}}"##
            );
            let spec = parse(&crossing_json);
            let frame = compute_frame(&spec, &measurer());
            let scene = build(&spec, &measurer());
            let fill = spec.series[0].fill_at(0);
            let stroke = spec.series[0].stroke_at(0);
            let mut series_primitives = 0;
            for item in &scene.items {
                match item {
                    Prim::ClippedPath {
                        d,
                        fill: Some(item_fill),
                        stroke: Some(item_stroke),
                        clip,
                        stroke_width,
                        ..
                    } if *item_fill == fill && *item_stroke == stroke => {
                        series_primitives += 1;
                        close(clip.x, frame.plot_left);
                        close(clip.y, frame.plot_top);
                        close(clip.w, frame.plot_right - frame.plot_left);
                        close(clip.h, frame.plot_bottom - frame.plot_top);
                        let margin = stroke_clip_margin(*stroke_width);
                        for (x, y) in path_points(d) {
                            assert!(
                                (frame.plot_left - margin..=frame.plot_right + margin).contains(&x)
                            );
                            assert!(
                                (frame.plot_top - margin..=frame.plot_bottom + margin).contains(&y)
                            );
                        }
                    }
                    Prim::Circle {
                        cx,
                        cy,
                        r,
                        fill: item_fill,
                        stroke: item_stroke,
                        stroke_width,
                    } if *item_fill == fill && *item_stroke == stroke => {
                        series_primitives += 1;
                        let outer = *r + *stroke_width / 2.0;
                        assert!(*cx - outer >= frame.plot_left - 0.02);
                        assert!(*cx + outer <= frame.plot_right + 0.02);
                        assert!(*cy - outer >= frame.plot_top - 0.02);
                        assert!(*cy + outer <= frame.plot_bottom + 0.02);
                    }
                    _ => {}
                }
            }
            assert_eq!(series_primitives, 3, "chart={chart_type}");
            let outside_json = format!(
                r##"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[100,110,120]],"backgroundColor":"#ff0000","borderColor":"#ff0000","borderWidth":8}}]}},"options":{{"scales":{{"{axis}":{{"min":0,"max":10}}}}}}}}"##
            );
            let outside_spec = parse(&outside_json);
            let outside_scene = build(&outside_spec, &measurer());
            let outside_fill = outside_spec.series[0].fill_at(0);
            let outside_stroke = outside_spec.series[0].stroke_at(0);
            assert!(
                !outside_scene.items.iter().any(|item| match item {
                    Prim::ClippedPath {
                        fill: Some(item_fill),
                        stroke: Some(item_stroke),
                        ..
                    } => *item_fill == outside_fill && *item_stroke == outside_stroke,
                    Prim::Circle {
                        fill: item_fill,
                        stroke: item_stroke,
                        ..
                    } => *item_fill == outside_fill && *item_stroke == outside_stroke,
                    _ => false,
                }),
                "fully outside group must not leave violin marks: chart={chart_type}"
            );
        }
    }

    #[test]
    fn violin_markers_are_kept_when_body_is_outside_a_hard_bound() {
        for (chart_type, horizontal) in [("violin", false), ("horizontalViolin", true)] {
            let axis = if horizontal { "x" } else { "y" };
            let json = format!(
                r##"{{"type":"{chart_type}","data":{{"labels":["A","B"],"datasets":[{{"data":[[-0.0002,-0.0001],[100,110]]}}]}},"options":{{"scales":{{"{axis}":{{"min":0,"max":110}}}}}}}}"##
            );
            let spec = parse(&json);
            let frame = compute_frame(&spec, &measurer());
            let scene = build(&spec, &measurer());
            let first_category = category_center(&frame, 0, 2);
            let first_group_paths = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Prim::ClippedPath { d, .. } => {
                        let points = path_points(d);
                        let category_mean = points
                            .iter()
                            .map(|(x, y)| if horizontal { *y } else { *x })
                            .sum::<f64>()
                            / points.len() as f64;
                        ((category_mean - first_category).abs() < 0.5).then_some(points.len())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                first_group_paths.iter().any(|count| *count > 100),
                "retain geometry for a body stroke intersecting the plot: {chart_type}"
            );
            assert!(
                first_group_paths
                    .iter()
                    .filter(|count| **count < 100)
                    .count()
                    >= 2,
                "mean and median markers should survive near a hard bound: {chart_type}"
            );
        }
    }

    #[test]
    fn violin_keeps_stroke_geometry_that_alone_reaches_a_hard_bound() {
        for (chart_type, axis) in [("violin", "y"), ("horizontalViolin", "x")] {
            let json = format!(
                r##"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[-0.5,-0.49]],"borderWidth":8}}]}},"options":{{"scales":{{"{axis}":{{"min":0,"max":100}}}}}}}}"##
            );
            let spec = parse(&json);
            let frame = compute_frame(&spec, &measurer());
            let scene = build(&spec, &measurer());
            let bodies = body_paths(&scene);
            assert_eq!(bodies.len(), 1, "stroke intersects plot: {chart_type}");
            let margin = stroke_clip_margin(spec.series[0].stroke_width);
            for (x, y) in path_points(bodies[0].0) {
                assert!((frame.plot_left - margin..=frame.plot_right + margin).contains(&x));
                assert!((frame.plot_top - margin..=frame.plot_bottom + margin).contains(&y));
            }
        }
    }

    #[test]
    fn violin_geometry_margin_tracks_large_finite_stroke_widths() {
        close(stroke_clip_margin(4_000_000.0), 8_000_001.0);
        for (chart_type, axis) in [("violin", "y"), ("horizontalViolin", "x")] {
            let json = format!(
                r##"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[-200000,-199999]],"borderWidth":4000000}}]}},"options":{{"scales":{{"{axis}":{{"min":0,"max":100}}}}}}}}"##
            );
            let spec = parse(&json);
            let scene = build(&spec, &measurer());
            assert_eq!(body_paths(&scene).len(), 1, "chart={chart_type}");
        }
    }

    #[test]
    fn violin_median_uses_exact_plot_clip_for_thick_mitered_stroke() {
        let spec = parse(
            r##"{"type":"horizontalViolin","data":{"labels":["A"],"datasets":[{"data":[[0.5,1,1,1,2]],"borderWidth":8}]},"options":{"scales":{"x":{"min":0,"max":100}}}}"##,
        );
        let frame = compute_frame(&spec, &measurer());
        let scene = build(&spec, &measurer());
        let median = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::ClippedPath {
                    d,
                    stroke_width,
                    clip,
                    ..
                } if d.matches("L ").count() == 3 => {
                    Some((*stroke_width, clip.x, clip.y, clip.w, clip.h))
                }
                _ => None,
            })
            .expect("median path is clipped");
        close(median.0, 8.0);
        close(median.1, frame.plot_left);
        close(median.2, frame.plot_top);
        close(median.3, frame.plot_right - frame.plot_left);
        close(median.4, frame.plot_bottom - frame.plot_top);
    }

    #[test]
    fn violin_zero_iqr_group_keeps_observed_value_range() {
        for (chart_type, horizontal) in [("violin", false), ("horizontalViolin", true)] {
            let json = format!(
                r#"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[[0,0,0,0,100]]}}]}}}}"#
            );
            let spec = parse(&json);
            let frame = compute_frame(&spec, &measurer());
            let scene = build(&spec, &measurer());
            let (body, _) = body_paths(&scene).into_iter().next().expect("violin body");
            let points = path_points(body);
            let observed_min = frame.value_scale.map(0.0);
            let observed_max = frame.value_scale.map(100.0);
            let coordinates = points
                .iter()
                .map(|(x, y)| if horizontal { *x } else { *y })
                .collect::<Vec<_>>();
            let expected_clip_inset = spec.series[0].stroke_width / 2.0 + 0.02;
            assert!(
                coordinates
                    .iter()
                    .any(|value| (value - observed_min).abs() < expected_clip_inset)
            );
            assert!(
                coordinates
                    .iter()
                    .any(|value| (value - observed_max).abs() < expected_clip_inset)
            );
        }
    }

    #[test]
    fn violin_singleton_and_constant_groups_remain_finite_in_both_orientations() {
        for chart_type in ["violin", "horizontalViolin"] {
            for data in ["[5]", "[3,3,3,3]"] {
                let json = format!(
                    r#"{{"type":"{chart_type}","data":{{"labels":["A"],"datasets":[{{"data":[{data}]}}]}}}}"#
                );
                let spec = parse(&json);
                let measurer = measurer();
                let frame = compute_frame(&spec, &measurer);
                let scene = build(&spec, &measurer);
                let bodies = body_paths(&scene);
                assert_eq!(bodies.len(), 1, "chart={chart_type}, data={data}");
                assert!(
                    path_points(bodies[0].0)
                        .iter()
                        .all(|(x, y)| x.is_finite() && y.is_finite())
                );
                let mean = scene
                    .items
                    .iter()
                    .find_map(|item| match item {
                        Prim::Circle { cx, cy, r, .. } => Some((*cx, *cy, *r)),
                        _ => None,
                    })
                    .expect("visible mean marker");
                assert!(mean.2 > 0.0, "mean marker has positive radius");
                if chart_type == "horizontalViolin" {
                    assert!(mean.0 > frame.plot_left && mean.0 < frame.plot_right);
                } else {
                    assert!(mean.1 > frame.plot_top && mean.1 < frame.plot_bottom);
                }
            }
        }
    }
}
