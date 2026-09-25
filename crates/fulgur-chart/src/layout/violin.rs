//! Violin chart layout.

use crate::ir::{AxisSpec, ChartKind, ChartSpec};
use crate::layout::common::{self, Frame};
use crate::scale::{LinearScale, NiceTicks, ValueScale};
use crate::scene::{Prim, Scene};
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
    let (domain_min, domain_max) = common::value_domain(spec, &spec.x_axis);
    let ticks = common::configured_axis_ticks(domain_min, domain_max, &spec.x_axis);
    let value_scale = ValueScale::Linear(LinearScale::new(
        ticks.min,
        ticks.max,
        layout.plot_left,
        layout.plot_right,
    ));
    ViolinFrame {
        horizontal: true,
        scene_width: spec.width,
        scene_height: spec.height,
        plot_left: layout.plot_left,
        plot_right: layout.plot_right,
        plot_top: layout.plot_top,
        plot_bottom: layout.plot_bottom,
        ticks,
        value_scale,
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
    (scale * 0.01)
        .max(MIN_FALLBACK_BANDWIDTH)
        .min(f64::MAX / 8.0)
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
        (
            fallback,
            (mean - radius).max(-f64::MAX),
            (mean + radius).min(f64::MAX),
        )
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

fn clipped_value(value: f64, ticks: &NiceTicks) -> f64 {
    common::clip_axis_value(value, ticks)
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

fn body_path(frame: &ViolinFrame, center: f64, half_width: f64, samples: &[f64]) -> String {
    let densities = density_samples(samples, &frame.ticks);
    let mut points = Vec::with_capacity(DENSITY_POSITIONS * 2);
    for &(value, density) in &densities {
        let value_pos = frame.value_scale.map(clipped_value(value, &frame.ticks));
        let half = half_width * density.clamp(0.0, 1.0);
        points.push(if frame.horizontal {
            (value_pos, center - half)
        } else {
            (center + half, value_pos)
        });
    }
    for &(value, density) in densities.iter().rev() {
        let value_pos = frame.value_scale.map(clipped_value(value, &frame.ticks));
        let half = half_width * density.clamp(0.0, 1.0);
        points.push(if frame.horizontal {
            (value_pos, center + half)
        } else {
            (center - half, value_pos)
        });
    }
    path_from_points(&points)
}

fn diamond_path(frame: &ViolinFrame, category: f64, value: f64) -> String {
    let value = frame.value_scale.map(clipped_value(value, &frame.ticks));
    let (center_x, center_y) = if frame.horizontal {
        (value, category)
    } else {
        (category, value)
    };
    let value_radius = DIAMOND_VALUE_RADIUS;
    let category_radius = DIAMOND_CATEGORY_RADIUS;
    let mut points = if frame.horizontal {
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
    for (x, y) in &mut points {
        *x = x.clamp(frame.plot_left, frame.plot_right);
        *y = y.clamp(frame.plot_top, frame.plot_bottom);
    }
    path_from_points(&points)
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
    let mean = frame
        .value_scale
        .map(clipped_value(stable_mean(samples), &frame.ticks));
    let (cx, cy) = if frame.horizontal {
        (mean, category)
    } else {
        (category, mean)
    };
    let radius = MARKER_RADIUS
        .min((cx - frame.plot_left).max(0.0))
        .min((frame.plot_right - cx).max(0.0))
        .min((cy - frame.plot_top).max(0.0))
        .min((frame.plot_bottom - cy).max(0.0));
    if radius > 0.0 && radius.is_finite() {
        items.push(Prim::Circle {
            cx,
            cy,
            r: radius,
            fill,
            stroke,
            stroke_width: series.stroke_width,
        });
    }
    items.push(Prim::Path {
        d: diamond_path(frame, category, median(samples)),
        fill: Some(fill),
        stroke: Some(stroke),
        stroke_width: series.stroke_width,
    });
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
            let d = body_path(&frame, category, half_width, &samples);
            scene.items.push(Prim::Path {
                d,
                fill: Some(series.fill_at(category_index)),
                stroke: Some(series.stroke_at(category_index)),
                stroke_width: series.stroke_width,
            });
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
                    Prim::Path { d, .. } if d.matches("L ").count() == 3 => Some(path_points(d)),
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
            for (d, _) in body_paths(&scene) {
                for (x, y) in path_points(d) {
                    assert!((frame.plot_left..=frame.plot_right).contains(&x));
                    assert!((frame.plot_top..=frame.plot_bottom).contains(&y));
                }
            }
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
                let scene = build(&spec, &measurer());
                let bodies = body_paths(&scene);
                assert_eq!(bodies.len(), 1, "chart={chart_type}, data={data}");
                assert!(
                    path_points(bodies[0].0)
                        .iter()
                        .all(|(x, y)| x.is_finite() && y.is_finite())
                );
            }
        }
    }
}
