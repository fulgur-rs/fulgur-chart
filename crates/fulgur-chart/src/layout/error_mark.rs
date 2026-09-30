//! Shared axis frame and errorbar Scene geometry for Vega-Lite composite marks.

use crate::ir::{
    AxisSpec, AxisTitleAlign, ChartKind, ChartSpec, Color, ErrorBandInterpolation, ErrorMarkData,
    ErrorMarkKind, ErrorPartStyle, ErrorPosition, ErrorRangePoint, LegendPos, ScaleKind,
};
use crate::num::fmt_num;
use crate::scale::{LinearScale, NiceTicks, ValueScale};
use crate::scene::{Anchor, ClipRect, Prim, Scene};
use crate::temporal::{TemporalScale, TemporalTick};
use crate::text::TextMeasurer;
use std::collections::HashMap;
use std::fmt::Write;

const OUTER_PAD: f64 = 8.0;
const TITLE_BAND: f64 = 28.0;
const TITLE_FONT: f64 = 16.0;
const LABEL_FONT_RATIO: f64 = 0.35;
const AXIS_LABEL_PAD: f64 = 10.0;
const X_LABEL_BAND: f64 = 22.0;
const AXIS_TITLE_BAND: f64 = 20.0;
const DEFAULT_RULE_WIDTH: f64 = 1.5;
const DEFAULT_TICK_SIZE: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorAxis {
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorAxisKind {
    Category,
    Linear,
    Logarithmic,
    Temporal,
    FullAxis,
}

#[derive(Clone, Debug)]
pub(crate) struct ErrorAxisInfo {
    pub(crate) kind: ErrorAxisKind,
    pub(crate) labels: Vec<String>,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
    pub(crate) ticks: Vec<f64>,
    pub(crate) temporal_ticks: Vec<TemporalTick>,
    pub(crate) nice_ticks: NiceTicks,
    temporal_values: Vec<i64>,
}

#[derive(Clone, Debug)]
enum AxisMapper {
    Category { count: usize },
    FullAxis,
    Continuous(ValueScale),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MappedErrorRange {
    /// Mapped independent coordinate: x for vertical bands, y for horizontal bands.
    independent: f64,
    lower: f64,
    upper: f64,
    horizontal: bool,
}

#[derive(Clone, Debug)]
struct MappedErrorBand {
    representative: ErrorRangePoint,
    ranges: Vec<MappedErrorRange>,
}

#[derive(Clone, Debug)]
pub(crate) struct ErrorMarkFrame {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) plot_left: f64,
    pub(crate) plot_right: f64,
    pub(crate) plot_top: f64,
    pub(crate) plot_bottom: f64,
    legend_left: f64,
    pub(crate) x: ErrorAxisInfo,
    pub(crate) y: ErrorAxisInfo,
    x_mapper: AxisMapper,
    y_mapper: AxisMapper,
}

impl ErrorMarkFrame {
    pub(crate) fn map_position(
        &self,
        axis: ErrorAxis,
        position: ErrorPosition,
    ) -> Result<f64, String> {
        let (info, mapper, start, end) = match axis {
            ErrorAxis::X => (&self.x, &self.x_mapper, self.plot_left, self.plot_right),
            ErrorAxis::Y => (&self.y, &self.y_mapper, self.plot_top, self.plot_bottom),
        };
        let pixel = match (mapper, position) {
            (AxisMapper::FullAxis, ErrorPosition::FullAxis) => (start + end) / 2.0,
            (AxisMapper::Category { count }, ErrorPosition::Category(index)) if index < *count => {
                start + (index as f64 + 0.5) * (end - start) / (*count).max(1) as f64
            }
            (
                AxisMapper::Continuous(ValueScale::Linear(scale)),
                ErrorPosition::Quantitative(value),
            ) => scale.map(value),
            (
                AxisMapper::Continuous(ValueScale::Log { inner, .. }),
                ErrorPosition::Quantitative(value),
            ) if value > 0.0 => inner.map(value.log10()),
            (
                AxisMapper::Continuous(ValueScale::Temporal(scale)),
                ErrorPosition::Temporal(value),
            ) => scale.map_millis(value),
            _ => {
                return Err(format!(
                    "error mark position does not match {:?} axis ({:?})",
                    axis, info.kind
                ));
            }
        };
        if pixel.is_finite() {
            Ok(pixel)
        } else {
            Err(format!(
                "error mark position on {axis:?} axis is non-finite"
            ))
        }
    }

    pub(crate) fn map_rule_position(
        &self,
        axis: ErrorAxis,
        position: crate::ir::VegaRulePosition,
    ) -> Result<f64, String> {
        use crate::ir::VegaRulePosition;
        match position {
            VegaRulePosition::FullAxisStart => Ok(match axis {
                ErrorAxis::X => self.plot_left,
                ErrorAxis::Y => self.plot_top,
            }),
            VegaRulePosition::FullAxisCenter => self.map_position(axis, ErrorPosition::FullAxis),
            VegaRulePosition::FullAxisEnd => Ok(match axis {
                ErrorAxis::X => self.plot_right,
                ErrorAxis::Y => self.plot_bottom,
            }),
            VegaRulePosition::Category(index) => {
                self.map_position(axis, ErrorPosition::Category(index))
            }
            VegaRulePosition::Quantitative(value) => {
                self.map_position(axis, ErrorPosition::Quantitative(value))
            }
            VegaRulePosition::Temporal(millis) => {
                self.map_position(axis, ErrorPosition::Temporal(millis))
            }
        }
    }

    fn full_axis_extent(&self, axis: ErrorAxis) -> (f64, f64) {
        match axis {
            ErrorAxis::X => (self.plot_left, self.plot_right),
            ErrorAxis::Y => (self.plot_top, self.plot_bottom),
        }
    }

    fn mapper(&self, axis: ErrorAxis) -> &AxisMapper {
        match axis {
            ErrorAxis::X => &self.x_mapper,
            ErrorAxis::Y => &self.y_mapper,
        }
    }
}

#[derive(Clone, Debug)]
struct AxisDomain {
    info: ErrorAxisInfo,
    mapper_kind: AxisMapperKind,
}

#[derive(Clone, Copy, Debug)]
enum AxisMapperKind {
    Category(usize),
    FullAxis,
    Continuous,
}

fn error_data(spec: &ChartSpec) -> &ErrorMarkData {
    let ChartKind::ErrorMark(data) = &spec.kind else {
        unreachable!("error-mark layout only accepts ChartKind::ErrorMark")
    };
    data
}

fn is_measured_axis(data: &ErrorMarkData, axis: ErrorAxis) -> bool {
    matches!(
        (data.orient, axis),
        (crate::ir::ErrorMarkOrient::Vertical, ErrorAxis::Y)
            | (crate::ir::ErrorMarkOrient::Horizontal, ErrorAxis::X)
    )
}

fn axis_domain(spec: &ChartSpec, axis: ErrorAxis) -> AxisDomain {
    let data = error_data(spec);
    let positions = if is_measured_axis(data, axis) {
        data.ranges
            .iter()
            .flat_map(|range| [range.lower, range.upper])
            .map(ErrorPosition::Quantitative)
            .collect::<Vec<_>>()
    } else {
        data.ranges
            .iter()
            .map(|range| range.position)
            .collect::<Vec<_>>()
    };
    axis_domain_for_positions(spec, axis, &positions, &spec.categories)
}

fn axis_domain_for_positions(
    spec: &ChartSpec,
    axis: ErrorAxis,
    positions: &[ErrorPosition],
    category_labels: &[String],
) -> AxisDomain {
    let axis_spec = match axis {
        ErrorAxis::X => &spec.x_axis,
        ErrorAxis::Y => &spec.y_axis,
    };
    let first = positions
        .first()
        .copied()
        .unwrap_or(ErrorPosition::FullAxis);
    match first {
        ErrorPosition::Category(_) => {
            let count = category_labels.len().max(1);
            AxisDomain {
                info: ErrorAxisInfo {
                    kind: ErrorAxisKind::Category,
                    labels: category_labels.to_vec(),
                    min: None,
                    max: None,
                    ticks: Vec::new(),
                    temporal_ticks: Vec::new(),
                    nice_ticks: NiceTicks {
                        min: 0.0,
                        max: count as f64,
                        step: 1.0,
                        ticks: (0..count).map(|index| index as f64).collect(),
                    },
                    temporal_values: Vec::new(),
                },
                mapper_kind: AxisMapperKind::Category(count),
            }
        }
        ErrorPosition::FullAxis => AxisDomain {
            info: ErrorAxisInfo {
                kind: ErrorAxisKind::FullAxis,
                labels: Vec::new(),
                min: Some(0.0),
                max: Some(1.0),
                ticks: Vec::new(),
                temporal_ticks: Vec::new(),
                nice_ticks: NiceTicks {
                    min: 0.0,
                    max: 1.0,
                    step: 1.0,
                    ticks: Vec::new(),
                },
                temporal_values: Vec::new(),
            },
            mapper_kind: AxisMapperKind::FullAxis,
        },
        ErrorPosition::Temporal(_) => {
            let mut values = positions
                .iter()
                .filter_map(|position| match position {
                    ErrorPosition::Temporal(value) => Some(*value),
                    _ => None,
                })
                .collect::<Vec<_>>();
            values.sort_unstable();
            values.dedup();
            let data_min = values.first().copied().unwrap_or(0) as f64;
            let data_max = values.last().copied().unwrap_or(1) as f64;
            let (min, max) =
                crate::layout::common::resolve_temporal_domain(axis_spec, data_min, data_max);
            let ticks = crate::layout::common::temporal_axis_ticks(
                axis_spec,
                min as i64,
                max as i64,
                if axis == ErrorAxis::X {
                    spec.width
                } else {
                    spec.height
                },
            );
            let nice_ticks = NiceTicks {
                min,
                max,
                step: 0.0,
                ticks: ticks.iter().map(|tick| tick.unix_millis as f64).collect(),
            };
            AxisDomain {
                info: ErrorAxisInfo {
                    kind: ErrorAxisKind::Temporal,
                    labels: ticks.iter().map(|tick| tick.label.clone()).collect(),
                    min: Some(min),
                    max: Some(max),
                    ticks: nice_ticks.ticks.clone(),
                    temporal_ticks: ticks,
                    nice_ticks,
                    temporal_values: values,
                },
                mapper_kind: AxisMapperKind::Continuous,
            }
        }
        ErrorPosition::Quantitative(_) => {
            let values = positions
                .iter()
                .filter_map(|position| match position {
                    ErrorPosition::Quantitative(value) => Some(*value),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let finite_min = values
                .iter()
                .copied()
                .filter(|value| value.is_finite())
                .fold(f64::INFINITY, f64::min);
            let finite_max = values
                .iter()
                .copied()
                .filter(|value| value.is_finite())
                .fold(f64::NEG_INFINITY, f64::max);
            let (nice_ticks, kind) = if axis_spec.scale_kind == ScaleKind::Logarithmic {
                let (min, max) = crate::layout::common::log_axis_domain_from_extrema(
                    axis_spec,
                    values
                        .iter()
                        .copied()
                        .filter(|value| value.is_finite() && *value > 0.0)
                        .fold(f64::INFINITY, f64::min),
                    values
                        .iter()
                        .copied()
                        .filter(|value| value.is_finite() && *value > 0.0)
                        .fold(f64::NEG_INFINITY, f64::max),
                    false,
                );
                let (ticks, _) = crate::scale::log_axis_ticks(min, max);
                (ticks, ErrorAxisKind::Logarithmic)
            } else {
                let (min, max) =
                    crate::layout::common::resolve_axis_domain(axis_spec, finite_min, finite_max);
                (
                    crate::layout::common::configured_axis_ticks(min, max, axis_spec),
                    ErrorAxisKind::Linear,
                )
            };
            AxisDomain {
                info: ErrorAxisInfo {
                    kind,
                    labels: Vec::new(),
                    min: Some(nice_ticks.min),
                    max: Some(nice_ticks.max),
                    ticks: nice_ticks.ticks.clone(),
                    temporal_ticks: Vec::new(),
                    nice_ticks,
                    temporal_values: Vec::new(),
                },
                mapper_kind: AxisMapperKind::Continuous,
            }
        }
    }
}

fn build_mapper(domain: &AxisDomain, start: f64, end: f64, axis: &AxisSpec) -> AxisMapper {
    match domain.mapper_kind {
        AxisMapperKind::Category(count) => AxisMapper::Category { count },
        AxisMapperKind::FullAxis => AxisMapper::FullAxis,
        AxisMapperKind::Continuous => {
            let ticks = &domain.info.nice_ticks;
            let scale = if domain.info.kind == ErrorAxisKind::Temporal {
                ValueScale::Temporal(TemporalScale::with_domain(
                    axis.scale_kind,
                    &domain.info.temporal_values,
                    ticks.min as i64,
                    ticks.max as i64,
                    start,
                    end,
                ))
            } else if axis.scale_kind == ScaleKind::Logarithmic {
                ValueScale::Log {
                    inner: LinearScale::new(ticks.min.log10(), ticks.max.log10(), start, end),
                    floor: ticks.min,
                }
            } else {
                ValueScale::Linear(LinearScale::new(ticks.min, ticks.max, start, end))
            };
            AxisMapper::Continuous(scale)
        }
    }
}

fn axis_tick_labels(info: &ErrorAxisInfo, axis: &AxisSpec) -> Vec<String> {
    match info.kind {
        ErrorAxisKind::Category => info.labels.clone(),
        ErrorAxisKind::Temporal => info
            .temporal_ticks
            .iter()
            .map(|tick| tick.label.clone())
            .collect(),
        ErrorAxisKind::Linear | ErrorAxisKind::Logarithmic => info
            .ticks
            .iter()
            .map(|tick| crate::layout::common::format_axis_tick(axis, *tick))
            .collect(),
        ErrorAxisKind::FullAxis => Vec::new(),
    }
}

fn has_legend(spec: &ChartSpec) -> bool {
    matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && spec.series.iter().any(|series| !series.name.is_empty())
}

/// Computes axis domains, chart frame margins, and plot-space coordinate mappings.
pub(crate) fn compute_frame(spec: &ChartSpec, m: &TextMeasurer) -> ErrorMarkFrame {
    compute_frame_for_domains(
        spec,
        m,
        axis_domain(spec, ErrorAxis::X),
        axis_domain(spec, ErrorAxis::Y),
    )
}

/// Builds the same axis frame for marks that supply typed positions on both axes.
pub(crate) fn compute_frame_for_positions(
    spec: &ChartSpec,
    m: &TextMeasurer,
    x_positions: &[ErrorPosition],
    x_categories: &[String],
    y_positions: &[ErrorPosition],
    y_categories: &[String],
) -> ErrorMarkFrame {
    compute_frame_for_domains(
        spec,
        m,
        axis_domain_for_positions(spec, ErrorAxis::X, x_positions, x_categories),
        axis_domain_for_positions(spec, ErrorAxis::Y, y_positions, y_categories),
    )
}

fn compute_frame_for_domains(
    spec: &ChartSpec,
    m: &TextMeasurer,
    x_domain: AxisDomain,
    y_domain: AxisDomain,
) -> ErrorMarkFrame {
    let y_labels = axis_tick_labels(&y_domain.info, &spec.y_axis);
    let font = spec.theme.font_size;
    let y_axis_label_width = y_labels
        .iter()
        .map(|label| m.width(label, font as f32) as f64)
        .fold(0.0, f64::max);
    let x_axis_title_height = spec
        .x_axis
        .title
        .as_ref()
        .map(|_| AXIS_TITLE_BAND)
        .unwrap_or(0.0);
    let y_axis_title_width = spec
        .y_axis
        .title
        .as_ref()
        .map(|title| title.font_size.unwrap_or(font * 1.1) + 6.0)
        .unwrap_or(0.0);
    let y_tick_margin = if spec.y_axis.grid.draw_ticks {
        spec.y_axis.grid.tick_length.max(0.0)
    } else {
        0.0
    };
    let x_tick_margin = if spec.x_axis.grid.draw_ticks {
        spec.x_axis.grid.tick_length.max(0.0)
    } else {
        0.0
    };
    let legend = has_legend(spec);
    let legend_title = crate::layout::common::legend_title(spec);
    let legend_font = crate::layout::common::legend_label_font_size(&spec.legend_options, font);
    let legend_height = crate::layout::common::legend_horizontal_band_height(
        &spec.legend_options,
        font,
        legend_title.is_some(),
    );
    let mut legend_names = spec
        .series
        .iter()
        .map(|series| series.name.clone())
        .collect::<Vec<_>>();
    legend_names.extend(legend_title.map(str::to_owned));
    let legend_vertical_width =
        if legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
            crate::layout::common::legend_band_width_vertical_styled(
                m,
                &legend_names,
                legend_font,
                &spec.legend_options,
            )
        } else {
            0.0
        };
    let legend_left = if legend && spec.legend == LegendPos::Left {
        legend_vertical_width
    } else {
        0.0
    };
    let legend_right = if legend && spec.legend == LegendPos::Right {
        legend_vertical_width
    } else {
        0.0
    };
    let legend_top = if legend && spec.legend == LegendPos::Top {
        legend_height
    } else {
        0.0
    };
    let legend_bottom = if legend && spec.legend == LegendPos::Bottom {
        legend_height
    } else {
        0.0
    };
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let x_labels_band = if matches!(x_domain.info.kind, ErrorAxisKind::FullAxis) {
        0.0
    } else {
        X_LABEL_BAND
    };
    let y_labels_width = if matches!(y_domain.info.kind, ErrorAxisKind::FullAxis) {
        0.0
    } else {
        y_axis_label_width + AXIS_LABEL_PAD + y_axis_title_width + y_tick_margin
    };
    let plot_left = (OUTER_PAD + y_labels_width + legend_left).min(spec.width);
    let plot_right = (spec.width - OUTER_PAD - legend_right).max(plot_left);
    let plot_top = (OUTER_PAD + title_band + legend_top).min(spec.height);
    let plot_bottom = (spec.height
        - OUTER_PAD
        - x_labels_band
        - x_axis_title_height
        - x_tick_margin
        - legend_bottom)
        .max(plot_top);
    let x_mapper = build_mapper(&x_domain, plot_left, plot_right, &spec.x_axis);
    let y_mapper = build_mapper(&y_domain, plot_bottom, plot_top, &spec.y_axis);
    let mut x_info = x_domain.info;
    let mut y_info = y_domain.info;
    // Temporal tick generation depends on the final plot extent; regenerate labels at that scale.
    if x_info.kind == ErrorAxisKind::Temporal {
        let ticks = crate::layout::common::temporal_axis_ticks(
            &spec.x_axis,
            x_info.nice_ticks.min as i64,
            x_info.nice_ticks.max as i64,
            (plot_right - plot_left).max(1.0),
        );
        x_info.labels = ticks.iter().map(|tick| tick.label.clone()).collect();
        x_info.ticks = ticks.iter().map(|tick| tick.unix_millis as f64).collect();
        x_info.temporal_ticks = ticks;
    }
    if y_info.kind == ErrorAxisKind::Temporal {
        let ticks = crate::layout::common::temporal_axis_ticks(
            &spec.y_axis,
            y_info.nice_ticks.min as i64,
            y_info.nice_ticks.max as i64,
            (plot_bottom - plot_top).max(1.0),
        );
        y_info.labels = ticks.iter().map(|tick| tick.label.clone()).collect();
        y_info.ticks = ticks.iter().map(|tick| tick.unix_millis as f64).collect();
        y_info.temporal_ticks = ticks;
    }
    ErrorMarkFrame {
        width: spec.width,
        height: spec.height,
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
        legend_left,
        x: x_info,
        y: y_info,
        x_mapper,
        y_mapper,
    }
}

fn map_continuous(mapper: &AxisMapper, position: ErrorPosition) -> Option<f64> {
    match (mapper, position) {
        (AxisMapper::Continuous(ValueScale::Linear(scale)), ErrorPosition::Quantitative(value)) => {
            Some(scale.map(value))
        }
        (
            AxisMapper::Continuous(ValueScale::Log { inner, .. }),
            ErrorPosition::Quantitative(value),
        ) if value > 0.0 => Some(inner.map(value.log10())),
        (AxisMapper::Continuous(ValueScale::Temporal(scale)), ErrorPosition::Temporal(value)) => {
            Some(scale.map_millis(value))
        }
        _ => None,
    }
}

fn map_position_with_clip(
    frame: &ErrorMarkFrame,
    axis: ErrorAxis,
    position: ErrorPosition,
    clip: bool,
) -> Result<f64, String> {
    if let ErrorPosition::Quantitative(value) = position {
        let mapper = frame.mapper(axis);
        let value = if clip && matches!(mapper, AxisMapper::Continuous(ValueScale::Linear(_))) {
            let info = match axis {
                ErrorAxis::X => &frame.x,
                ErrorAxis::Y => &frame.y,
            };
            let min = info
                .min
                .ok_or_else(|| format!("error mark {axis:?} axis has no lower domain bound"))?;
            let max = info
                .max
                .ok_or_else(|| format!("error mark {axis:?} axis has no upper domain bound"))?;
            // Keep coordinates finite without moving out-of-domain endpoints onto the plot edge.
            // Segment/path clipping then removes caps and boundaries that lie wholly outside.
            let span = max - min;
            let pad = if span.is_finite() && span > 0.0 {
                span
            } else {
                min.abs().max(max.abs()).max(1.0) / 4.0
            };
            let padded_min = min - pad;
            let padded_max = max + pad;
            value.clamp(
                if padded_min.is_finite() {
                    padded_min
                } else {
                    -f64::MAX
                },
                if padded_max.is_finite() {
                    padded_max
                } else {
                    f64::MAX
                },
            )
        } else {
            value
        };
        let mapped = map_continuous(mapper, ErrorPosition::Quantitative(value))
            .ok_or_else(|| format!("error mark position does not match {axis:?} axis"))?;
        if !mapped.is_finite() {
            return Err(format!(
                "error mark position on {axis:?} axis maps to a non-finite coordinate"
            ));
        }
        Ok(mapped)
    } else {
        frame.map_position(axis, position)
    }
}

fn draw_axis(items: &mut Vec<Prim>, spec: &ChartSpec, frame: &ErrorMarkFrame, axis: ErrorAxis) {
    let (info, axis_spec) = match axis {
        ErrorAxis::X => (&frame.x, &spec.x_axis),
        ErrorAxis::Y => (&frame.y, &spec.y_axis),
    };
    if info.kind == ErrorAxisKind::FullAxis {
        return;
    }
    let is_x = axis == ErrorAxis::X;
    let mut tick_positions = Vec::new();
    match info.kind {
        ErrorAxisKind::Category => {
            for (index, label) in info.labels.iter().enumerate() {
                let position = match axis {
                    ErrorAxis::X => {
                        frame.plot_left
                            + (index as f64 + 0.5) * (frame.plot_right - frame.plot_left)
                                / info.labels.len().max(1) as f64
                    }
                    ErrorAxis::Y => {
                        frame.plot_top
                            + (index as f64 + 0.5) * (frame.plot_bottom - frame.plot_top)
                                / info.labels.len().max(1) as f64
                    }
                };
                tick_positions.push((position, label.clone()));
            }
        }
        ErrorAxisKind::Linear | ErrorAxisKind::Logarithmic | ErrorAxisKind::Temporal => {
            let values = &info.ticks;
            let labels = axis_tick_labels(info, axis_spec);
            for (index, value) in values.iter().enumerate() {
                let position = match axis {
                    ErrorAxis::X => {
                        map_continuous(&frame.x_mapper, ErrorPosition::Quantitative(*value))
                            .or_else(|| {
                                map_continuous(
                                    &frame.x_mapper,
                                    ErrorPosition::Temporal(*value as i64),
                                )
                            })
                    }
                    ErrorAxis::Y => {
                        map_continuous(&frame.y_mapper, ErrorPosition::Quantitative(*value))
                            .or_else(|| {
                                map_continuous(
                                    &frame.y_mapper,
                                    ErrorPosition::Temporal(*value as i64),
                                )
                            })
                    }
                };
                if let Some(position) = position
                    && let Some(label) = labels.get(index)
                {
                    tick_positions.push((position, label.clone()));
                }
            }
        }
        ErrorAxisKind::FullAxis => return,
    }
    let grid = &axis_spec.grid;
    let grid_color = grid.color.unwrap_or(spec.theme.grid_color);
    let ink = spec.theme.text_color;
    let font = spec.theme.font_size;
    for (position, label) in tick_positions {
        if grid.display {
            items.push(Prim::Line {
                x1: if is_x { position } else { frame.plot_left },
                y1: if is_x { frame.plot_top } else { position },
                x2: if is_x { position } else { frame.plot_right },
                y2: if is_x { frame.plot_bottom } else { position },
                stroke: grid_color,
                stroke_width: grid.line_width,
                dash: Vec::new(),
            });
        }
        if is_x {
            items.push(Prim::Text {
                x: position,
                y: frame.plot_bottom + X_LABEL_BAND * 0.7,
                size: font,
                anchor: Anchor::Middle,
                fill: ink,
                content: label,
                rotate_deg: None,
            });
            if grid.draw_ticks {
                items.push(Prim::Line {
                    x1: position,
                    y1: frame.plot_bottom,
                    x2: position,
                    y2: frame.plot_bottom + grid.tick_length,
                    stroke: grid.resolved_tick_color(spec.theme.grid_color),
                    stroke_width: grid.resolved_tick_width(),
                    dash: Vec::new(),
                });
            }
        } else {
            items.push(Prim::Text {
                x: frame.plot_left - 6.0,
                y: position + font * LABEL_FONT_RATIO,
                size: font,
                anchor: Anchor::End,
                fill: ink,
                content: label,
                rotate_deg: None,
            });
            if grid.draw_ticks {
                items.push(Prim::Line {
                    x1: frame.plot_left - grid.tick_length,
                    y1: position,
                    x2: frame.plot_left,
                    y2: position,
                    stroke: grid.resolved_tick_color(spec.theme.grid_color),
                    stroke_width: grid.resolved_tick_width(),
                    dash: Vec::new(),
                });
            }
        }
    }
    let border = &axis_spec.border;
    if border.display {
        let color = border.color.unwrap_or(ink);
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: if is_x {
                frame.plot_bottom
            } else {
                frame.plot_top
            },
            x2: if is_x {
                frame.plot_right
            } else {
                frame.plot_left
            },
            y2: frame.plot_bottom,
            stroke: color,
            stroke_width: border.width,
            dash: border.dash.clone(),
        });
    }
    if let Some(title) = &axis_spec.title {
        let size = title.font_size.unwrap_or(font * 1.1);
        let color = title.color.unwrap_or(ink);
        if is_x {
            let anchor = match title.align {
                AxisTitleAlign::Start => Anchor::Start,
                AxisTitleAlign::Center => Anchor::Middle,
                AxisTitleAlign::End => Anchor::End,
            };
            let x = match title.align {
                AxisTitleAlign::Start => frame.plot_left,
                AxisTitleAlign::Center => (frame.plot_left + frame.plot_right) / 2.0,
                AxisTitleAlign::End => frame.plot_right,
            };
            let y = frame.plot_bottom
                + if frame.x.kind == ErrorAxisKind::FullAxis {
                    0.0
                } else {
                    X_LABEL_BAND
                }
                + size * 0.9;
            items.push(Prim::Text {
                x,
                y,
                size,
                anchor,
                fill: color,
                content: title.text.clone(),
                rotate_deg: None,
            });
        } else {
            let (y, anchor) = match title.align {
                AxisTitleAlign::Start => (frame.plot_bottom, Anchor::Start),
                AxisTitleAlign::End => (frame.plot_top, Anchor::End),
                AxisTitleAlign::Center => {
                    ((frame.plot_top + frame.plot_bottom) / 2.0, Anchor::Middle)
                }
            };
            items.push(Prim::Text {
                x: OUTER_PAD + frame.legend_left + size / 2.0,
                y,
                size,
                anchor,
                fill: color,
                content: title.text.clone(),
                rotate_deg: Some(-90.0),
            });
        }
    }
}

fn draw_chart_title(items: &mut Vec<Prim>, spec: &ChartSpec, frame: &ErrorMarkFrame) {
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: (frame.plot_left + frame.plot_right) / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: spec.theme.text_color,
            content: title.clone(),
            rotate_deg: None,
        });
    }
}

fn draw_legend(items: &mut Vec<Prim>, spec: &ChartSpec, frame: &ErrorMarkFrame, m: &TextMeasurer) {
    if !has_legend(spec) {
        return;
    }
    let entries = spec
        .series
        .iter()
        .filter(|series| !series.name.is_empty())
        .map(|series| {
            let color = series
                .fill
                .first()
                .copied()
                .unwrap_or(spec.theme.text_color);
            (series.name.clone(), color)
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return;
    }
    let title = crate::layout::common::legend_title(spec);
    if matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let center_y = if spec.legend == LegendPos::Top {
            OUTER_PAD
                + if spec.title.is_some() {
                    TITLE_BAND
                } else {
                    0.0
                }
                + crate::layout::common::legend_horizontal_band_height(
                    &spec.legend_options,
                    spec.theme.font_size,
                    title.is_some(),
                ) / 2.0
        } else {
            frame.height
                - OUTER_PAD
                - crate::layout::common::legend_horizontal_band_height(
                    &spec.legend_options,
                    spec.theme.font_size,
                    title.is_some(),
                ) / 2.0
        };
        crate::layout::common::draw_horizontal_legend(
            items,
            &entries,
            title,
            frame.width,
            center_y,
            spec.theme.font_size,
            spec.theme.text_color,
            m,
            &spec.legend_options,
        );
    } else {
        let x = if spec.legend == LegendPos::Left {
            OUTER_PAD
        } else {
            frame.plot_right + OUTER_PAD
        };
        crate::layout::common::draw_vertical_legend_styled(
            items,
            &entries,
            title,
            x,
            frame.plot_top,
            frame.plot_bottom,
            spec.theme.text_color,
            spec.theme.font_size,
            &spec.legend_options,
        );
    }
}

fn resolved_color(spec: &ChartSpec, range: &ErrorRangePoint, style: &ErrorPartStyle) -> Color {
    let base = style
        .stroke
        .or(style.fill)
        .or_else(|| {
            spec.series
                .get(range.series_index)
                .and_then(|series| series.stroke.first().or(series.fill.first()).copied())
        })
        .unwrap_or(spec.theme.text_color);
    Color {
        a: base.a * style.opacity.unwrap_or(error_data(spec).style.opacity) as f32,
        ..base
    }
}

fn clip_segment(
    start: (f64, f64),
    end: (f64, f64),
    frame: &ErrorMarkFrame,
) -> Option<((f64, f64), (f64, f64))> {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }
    let mut enter = 0.0_f64;
    let mut leave = 1.0_f64;
    for (p, q) in [
        (-dx, start.0 - frame.plot_left),
        (dx, frame.plot_right - start.0),
        (-dy, start.1 - frame.plot_top),
        (dy, frame.plot_bottom - start.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            if ratio > leave {
                return None;
            }
            enter = enter.max(ratio);
        } else {
            if ratio < enter {
                return None;
            }
            leave = leave.min(ratio);
        }
    }
    if enter > leave {
        return None;
    }
    Some((
        (start.0 + enter * dx, start.1 + enter * dy),
        (start.0 + leave * dx, start.1 + leave * dy),
    ))
}

fn push_segment(
    items: &mut Vec<Prim>,
    segment: ((f64, f64), (f64, f64)),
    color: Color,
    width: f64,
    dash: &[f64],
    clip: bool,
    frame: &ErrorMarkFrame,
) {
    let (start, end) = segment;
    if ![start.0, start.1, end.0, end.1, width]
        .iter()
        .all(|value| value.is_finite())
    {
        return;
    }
    let (start, end) = if clip {
        let Some(segment) = clip_segment(start, end, frame) else {
            return;
        };
        segment
    } else {
        (start, end)
    };
    if ![start.0, start.1, end.0, end.1]
        .iter()
        .all(|value| value.is_finite())
    {
        return;
    }
    items.push(Prim::Line {
        x1: start.0,
        y1: start.1,
        x2: end.0,
        y2: end.1,
        stroke: color,
        stroke_width: width,
        dash: dash.to_vec(),
    });
}

fn draw_errorbars(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    frame: &ErrorMarkFrame,
) -> Result<(), String> {
    let data = error_data(spec);
    if data.kind != ErrorMarkKind::ErrorBar {
        return Ok(());
    }
    let measure_axis = if data.orient == crate::ir::ErrorMarkOrient::Vertical {
        ErrorAxis::Y
    } else {
        ErrorAxis::X
    };
    let cross_axis = if measure_axis == ErrorAxis::X {
        ErrorAxis::Y
    } else {
        ErrorAxis::X
    };
    let clip = data.style.clip;
    for range in &data.ranges {
        let cross = match frame.map_position(cross_axis, range.position) {
            Ok(value) => value,
            Err(_) if matches!(frame.mapper(cross_axis), AxisMapper::FullAxis) => {
                let (start, end) = frame.full_axis_extent(cross_axis);
                (start + end) / 2.0
            }
            Err(error) => return Err(error),
        };
        let mut start_value = map_position_with_clip(
            frame,
            measure_axis,
            ErrorPosition::Quantitative(range.lower),
            clip,
        )?;
        let mut end_value = map_position_with_clip(
            frame,
            measure_axis,
            ErrorPosition::Quantitative(range.upper),
            clip,
        )?;
        if range.lower == range.upper {
            start_value -= 0.5;
            end_value += 0.5;
        }
        let (start, end) = match measure_axis {
            ErrorAxis::X => ((start_value, cross), (end_value, cross)),
            ErrorAxis::Y => ((cross, start_value), (cross, end_value)),
        };
        if data.style.rule.visible {
            push_segment(
                items,
                (start, end),
                resolved_color(spec, range, &data.style.rule),
                data.style.rule.stroke_width.unwrap_or(DEFAULT_RULE_WIDTH),
                &data.style.rule.stroke_dash,
                clip,
                frame,
            );
        }
        if data.style.ticks.visible {
            let half_size = data.style.ticks.size.unwrap_or(DEFAULT_TICK_SIZE) / 2.0;
            let cap_color = resolved_color(spec, range, &data.style.ticks);
            let cap_width = data.style.ticks.stroke_width.unwrap_or(DEFAULT_RULE_WIDTH);
            for (x, y) in [start, end] {
                let cap = if measure_axis == ErrorAxis::X {
                    ((x, y - half_size), (x, y + half_size))
                } else {
                    ((x - half_size, y), (x + half_size, y))
                };
                push_segment(
                    items,
                    cap,
                    cap_color,
                    cap_width,
                    &data.style.ticks.stroke_dash,
                    clip,
                    frame,
                );
            }
        }
    }
    Ok(())
}

fn error_position_order(left: ErrorPosition, right: ErrorPosition) -> std::cmp::Ordering {
    match (left, right) {
        (ErrorPosition::FullAxis, ErrorPosition::FullAxis) => std::cmp::Ordering::Equal,
        (ErrorPosition::Category(left), ErrorPosition::Category(right)) => left.cmp(&right),
        (ErrorPosition::Quantitative(left), ErrorPosition::Quantitative(right)) => {
            left.total_cmp(&right)
        }
        (ErrorPosition::Temporal(left), ErrorPosition::Temporal(right)) => left.cmp(&right),
        (left, right) => error_position_kind(left).cmp(&error_position_kind(right)),
    }
}

fn error_position_kind(position: ErrorPosition) -> u8 {
    match position {
        ErrorPosition::FullAxis => 0,
        ErrorPosition::Category(_) => 1,
        ErrorPosition::Quantitative(_) => 2,
        ErrorPosition::Temporal(_) => 3,
    }
}

fn mapped_errorbands(
    spec: &ChartSpec,
    frame: &ErrorMarkFrame,
) -> Result<Vec<MappedErrorBand>, String> {
    let data = error_data(spec);
    if data.kind != ErrorMarkKind::ErrorBand {
        return Ok(Vec::new());
    }
    let horizontal = data.orient == crate::ir::ErrorMarkOrient::Horizontal;
    let measure_axis = if horizontal {
        ErrorAxis::X
    } else {
        ErrorAxis::Y
    };
    let independent_axis = if horizontal {
        ErrorAxis::Y
    } else {
        ErrorAxis::X
    };
    let mut groups: Vec<(usize, Option<String>, Vec<ErrorRangePoint>)> = Vec::new();
    let mut group_indices = HashMap::new();
    for range in &data.ranges {
        let key = (range.series_index, range.detail.clone());
        let index = if let Some(index) = group_indices.get(&key) {
            *index
        } else {
            let index = groups.len();
            group_indices.insert(key.clone(), index);
            groups.push((key.0, key.1, Vec::new()));
            index
        };
        groups[index].2.push(range.clone());
    }

    let mut mapped_groups = Vec::with_capacity(groups.len());
    for (series_index, _detail, mut ranges) in groups {
        ranges.sort_by(|left, right| error_position_order(left.position, right.position));
        let representative = ranges[0].clone();
        let full_axis = matches!(ranges[0].position, ErrorPosition::FullAxis);
        if full_axis && ranges.len() != 1 {
            return Err(
                "an errorband without an independent axis must have one range per group".into(),
            );
        }

        let mut mapped = Vec::with_capacity(if full_axis { 2 } else { ranges.len() });
        for range in &ranges {
            let independent =
                map_position_with_clip(frame, independent_axis, range.position, data.style.clip)?;
            let lower = map_position_with_clip(
                frame,
                measure_axis,
                ErrorPosition::Quantitative(range.lower),
                data.style.clip,
            )?;
            let upper = map_position_with_clip(
                frame,
                measure_axis,
                ErrorPosition::Quantitative(range.upper),
                data.style.clip,
            )?;
            if ![independent, lower, upper]
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(format!(
                    "errorband range for series {series_index} maps to a non-finite coordinate"
                ));
            }
            mapped.push(MappedErrorRange {
                independent,
                lower,
                upper,
                horizontal,
            });
        }

        if full_axis {
            let (start, end) = frame.full_axis_extent(independent_axis);
            let only = mapped[0];
            mapped.clear();
            mapped.push(MappedErrorRange {
                independent: start,
                ..only
            });
            mapped.push(MappedErrorRange {
                independent: end,
                ..only
            });
        }
        mapped_groups.push(MappedErrorBand {
            representative,
            ranges: mapped,
        });
    }
    Ok(mapped_groups)
}

fn boundary_samples(
    points: &[(f64, f64)],
    interpolation: ErrorBandInterpolation,
    tension: f64,
) -> Vec<(f64, f64)> {
    match interpolation {
        ErrorBandInterpolation::BasisOpen => return basis_open_samples(points),
        ErrorBandInterpolation::BasisClosed => return basis_closed_samples(points),
        ErrorBandInterpolation::CardinalOpen => return cardinal_open_samples(points, tension),
        ErrorBandInterpolation::CardinalClosed => {
            return cardinal_closed_samples(points, tension);
        }
        _ => {}
    }
    if points.len() < 2 {
        return points.to_vec();
    }
    match interpolation {
        ErrorBandInterpolation::Linear | ErrorBandInterpolation::LinearClosed => points.to_vec(),
        ErrorBandInterpolation::Step => step_samples(points, StepInterpolation::Middle),
        ErrorBandInterpolation::StepBefore => step_samples(points, StepInterpolation::Before),
        ErrorBandInterpolation::StepAfter => step_samples(points, StepInterpolation::After),
        ErrorBandInterpolation::Monotone => {
            if points[0].0 <= points[points.len() - 1].0 {
                crate::layout::monotone::monotone_samples(points, 8)
            } else {
                let reversed = points.iter().copied().rev().collect::<Vec<_>>();
                crate::layout::monotone::monotone_samples(&reversed, 8)
                    .into_iter()
                    .rev()
                    .collect()
            }
        }
        ErrorBandInterpolation::Basis => basis_samples(points),
        ErrorBandInterpolation::Bundle => bundle_samples(points, tension),
        ErrorBandInterpolation::Cardinal => cardinal_samples(points, tension),
        ErrorBandInterpolation::BasisOpen
        | ErrorBandInterpolation::BasisClosed
        | ErrorBandInterpolation::CardinalOpen
        | ErrorBandInterpolation::CardinalClosed => unreachable!("handled above"),
    }
}

fn basis_open_samples(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    const SAMPLES_PER_SEGMENT: usize = 8;
    if points.len() < 3 {
        return Vec::new();
    }
    let weighted = |p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)| {
        (
            (p0.0 + 4.0 * p1.0 + p2.0) / 6.0,
            (p0.1 + 4.0 * p1.1 + p2.1) / 6.0,
        )
    };
    let mut current = weighted(points[0], points[1], points[2]);
    let mut sampled = vec![current];
    for index in 3..points.len() {
        let p1 = points[index - 2];
        let p2 = points[index - 1];
        let p3 = points[index];
        let control1 = ((2.0 * p1.0 + p2.0) / 3.0, (2.0 * p1.1 + p2.1) / 3.0);
        let control2 = ((p1.0 + 2.0 * p2.0) / 3.0, (p1.1 + 2.0 * p2.1) / 3.0);
        let end = weighted(p1, p2, p3);
        append_cubic_samples(
            &mut sampled,
            current,
            control1,
            control2,
            end,
            SAMPLES_PER_SEGMENT,
        );
        current = end;
    }
    sampled
}

fn basis_closed_samples(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    const SAMPLES_PER_SEGMENT: usize = 8;
    match points.len() {
        0 => return Vec::new(),
        1 => return points.to_vec(),
        2 => {
            let (p0, p1) = (points[0], points[1]);
            return vec![
                ((p0.0 + 2.0 * p1.0) / 3.0, (p0.1 + 2.0 * p1.1) / 3.0),
                ((p1.0 + 2.0 * p0.0) / 3.0, (p1.1 + 2.0 * p0.1) / 3.0),
            ];
        }
        _ => {}
    }
    let weighted = |p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)| {
        (
            (p0.0 + 4.0 * p1.0 + p2.0) / 6.0,
            (p0.1 + 4.0 * p1.1 + p2.1) / 6.0,
        )
    };
    let count = points.len();
    let mut current = weighted(points[count - 1], points[0], points[1]);
    let mut sampled = vec![current];
    for index in 0..count {
        let p0 = points[index];
        let p1 = points[(index + 1) % count];
        let p2 = points[(index + 2) % count];
        let control1 = ((2.0 * p0.0 + p1.0) / 3.0, (2.0 * p0.1 + p1.1) / 3.0);
        let control2 = ((p0.0 + 2.0 * p1.0) / 3.0, (p0.1 + 2.0 * p1.1) / 3.0);
        let end = weighted(p0, p1, p2);
        append_cubic_samples(
            &mut sampled,
            current,
            control1,
            control2,
            end,
            SAMPLES_PER_SEGMENT,
        );
        current = end;
    }
    sampled
}

fn cardinal_open_samples(points: &[(f64, f64)], tension: f64) -> Vec<(f64, f64)> {
    const SAMPLES_PER_SEGMENT: usize = 8;
    if points.len() < 4 {
        return Vec::new();
    }
    let k = (1.0 - tension) / 6.0;
    let mut sampled = vec![points[1]];
    for index in 1..points.len() - 2 {
        let p0 = points[index - 1];
        let p1 = points[index];
        let p2 = points[index + 1];
        let p3 = points[index + 2];
        let control1 = (p1.0 + k * (p2.0 - p0.0), p1.1 + k * (p2.1 - p0.1));
        let control2 = (p2.0 + k * (p1.0 - p3.0), p2.1 + k * (p1.1 - p3.1));
        append_cubic_samples(
            &mut sampled,
            p1,
            control1,
            control2,
            p2,
            SAMPLES_PER_SEGMENT,
        );
    }
    sampled
}

fn cardinal_closed_samples(points: &[(f64, f64)], tension: f64) -> Vec<(f64, f64)> {
    const SAMPLES_PER_SEGMENT: usize = 8;
    match points.len() {
        0 => return Vec::new(),
        1 => return vec![points[0], points[0]],
        2 => return vec![points[0], points[1], points[0]],
        _ => {}
    }
    let count = points.len();
    let k = (1.0 - tension) / 6.0;
    let mut sampled = vec![points[0]];
    for index in 0..count {
        let p0 = points[(index + count - 1) % count];
        let p1 = points[index];
        let p2 = points[(index + 1) % count];
        let p3 = points[(index + 2) % count];
        let control1 = (p1.0 + k * (p2.0 - p0.0), p1.1 + k * (p2.1 - p0.1));
        let control2 = (p2.0 + k * (p1.0 - p3.0), p2.1 + k * (p1.1 - p3.1));
        append_cubic_samples(
            &mut sampled,
            p1,
            control1,
            control2,
            p2,
            SAMPLES_PER_SEGMENT,
        );
    }
    sampled
}

#[derive(Clone, Copy)]
enum StepInterpolation {
    Middle,
    Before,
    After,
}

fn step_samples(points: &[(f64, f64)], interpolation: StepInterpolation) -> Vec<(f64, f64)> {
    let mut sampled = vec![points[0]];
    for pair in points.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        match interpolation {
            StepInterpolation::Middle => {
                let middle = (start.0 + end.0) / 2.0;
                sampled.extend([(middle, start.1), (middle, end.1)]);
            }
            StepInterpolation::Before => sampled.push((start.0, end.1)),
            StepInterpolation::After => sampled.push((end.0, start.1)),
        }
        sampled.push(end);
    }
    sampled
}

fn bundle_samples(points: &[(f64, f64)], tension: f64) -> Vec<(f64, f64)> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let last_index = (points.len() - 1) as f64;
    let first = points[0];
    let last = points[points.len() - 1];
    let blended = points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let t = index as f64 / last_index;
            let straight = (
                first.0 + (last.0 - first.0) * t,
                first.1 + (last.1 - first.1) * t,
            );
            (
                straight.0 * (1.0 - tension) + point.0 * tension,
                straight.1 * (1.0 - tension) + point.1 * tension,
            )
        })
        .collect::<Vec<_>>();
    basis_samples(&blended)
}

fn cardinal_samples(points: &[(f64, f64)], tension: f64) -> Vec<(f64, f64)> {
    const SAMPLES_PER_SEGMENT: usize = 8;
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut sampled = vec![points[0]];
    for index in 0..points.len() - 1 {
        let p0 = points[index.saturating_sub(1)];
        let p1 = points[index];
        let p2 = points[index + 1];
        let p3 = points[(index + 2).min(points.len() - 1)];
        let tangent_scale = (1.0 - tension) / 6.0;
        let cp1 = (
            p1.0 + (p2.0 - p0.0) * tangent_scale,
            p1.1 + (p2.1 - p0.1) * tangent_scale,
        );
        let cp2 = (
            p2.0 - (p3.0 - p1.0) * tangent_scale,
            p2.1 - (p3.1 - p1.1) * tangent_scale,
        );
        for step in 1..=SAMPLES_PER_SEGMENT {
            sampled.push(cubic_point(
                p1,
                cp1,
                cp2,
                p2,
                step as f64 / SAMPLES_PER_SEGMENT as f64,
            ));
        }
    }
    sampled
}

fn basis_samples(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    const SAMPLES_PER_SEGMENT: usize = 8;
    if points.len() < 3 {
        return points.to_vec();
    }
    let first = points[0];
    let second = points[1];
    let mut current = (
        (5.0 * first.0 + second.0) / 6.0,
        (5.0 * first.1 + second.1) / 6.0,
    );
    let mut sampled = vec![first, current];
    for index in 2..points.len() {
        let p0 = points[index - 2];
        let p1 = points[index - 1];
        let p2 = points[index];
        let control1 = ((2.0 * p0.0 + p1.0) / 3.0, (2.0 * p0.1 + p1.1) / 3.0);
        let control2 = ((p0.0 + 2.0 * p1.0) / 3.0, (p0.1 + 2.0 * p1.1) / 3.0);
        let end = (
            (p0.0 + 4.0 * p1.0 + p2.0) / 6.0,
            (p0.1 + 4.0 * p1.1 + p2.1) / 6.0,
        );
        append_cubic_samples(
            &mut sampled,
            current,
            control1,
            control2,
            end,
            SAMPLES_PER_SEGMENT,
        );
        current = end;
    }
    let penultimate = points[points.len() - 2];
    let last = points[points.len() - 1];
    let control1 = (
        (2.0 * penultimate.0 + last.0) / 3.0,
        (2.0 * penultimate.1 + last.1) / 3.0,
    );
    let control2 = (
        (penultimate.0 + 2.0 * last.0) / 3.0,
        (penultimate.1 + 2.0 * last.1) / 3.0,
    );
    let end = (
        (penultimate.0 + 5.0 * last.0) / 6.0,
        (penultimate.1 + 5.0 * last.1) / 6.0,
    );
    append_cubic_samples(
        &mut sampled,
        current,
        control1,
        control2,
        end,
        SAMPLES_PER_SEGMENT,
    );
    sampled.push(last);
    sampled
}

fn append_cubic_samples(
    output: &mut Vec<(f64, f64)>,
    start: (f64, f64),
    control1: (f64, f64),
    control2: (f64, f64),
    end: (f64, f64),
    steps: usize,
) {
    for step in 1..=steps {
        output.push(cubic_point(
            start,
            control1,
            control2,
            end,
            step as f64 / steps as f64,
        ));
    }
}

fn cubic_point(
    p0: (f64, f64),
    cp1: (f64, f64),
    cp2: (f64, f64),
    p1: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let one_minus_t = 1.0 - t;
    let a = one_minus_t * one_minus_t * one_minus_t;
    let b = 3.0 * one_minus_t * one_minus_t * t;
    let c = 3.0 * one_minus_t * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * cp1.0 + c * cp2.0 + d * p1.0,
        a * p0.1 + b * cp1.1 + c * cp2.1 + d * p1.1,
    )
}

fn errorband_paths(
    ranges: &[MappedErrorRange],
    interpolation: ErrorBandInterpolation,
    tension: f64,
) -> Result<Option<(String, String, String)>, String> {
    if ranges.len() < 2
        || ranges
            .iter()
            .any(|range| !range.horizontal.eq(&ranges[0].horizontal))
    {
        return Ok(None);
    }
    let upper = ranges
        .iter()
        .map(|range| (range.independent, range.upper))
        .collect::<Vec<_>>();
    let lower = ranges
        .iter()
        .map(|range| (range.independent, range.lower))
        .collect::<Vec<_>>();
    let upper = boundary_samples(&upper, interpolation, tension);
    let lower = boundary_samples(&lower, interpolation, tension);
    if upper.len() < 2 || lower.len() < 2 {
        return Ok(None);
    }
    if upper
        .iter()
        .chain(&lower)
        .any(|point| !point.0.is_finite() || !point.1.is_finite())
    {
        return Err("errorband interpolation produced a non-finite coordinate".into());
    }

    let convert = |point: (f64, f64)| {
        if ranges[0].horizontal {
            (point.1, point.0)
        } else {
            point
        }
    };
    let polygon_points = upper
        .iter()
        .copied()
        .chain(lower.iter().copied().rev())
        .map(convert)
        .collect::<Vec<_>>();
    let polygon = path_from_points(&polygon_points, true)
        .ok_or_else(|| "errorband polygon contains a non-finite coordinate".to_string())?;
    let close_boundaries = matches!(
        interpolation,
        ErrorBandInterpolation::LinearClosed
            | ErrorBandInterpolation::BasisClosed
            | ErrorBandInterpolation::CardinalClosed
    );
    let upper = path_from_points(
        &upper.iter().copied().map(convert).collect::<Vec<_>>(),
        close_boundaries,
    )
    .ok_or_else(|| "errorband upper boundary contains a non-finite coordinate".to_string())?;
    let lower = path_from_points(
        &lower.iter().copied().map(convert).collect::<Vec<_>>(),
        close_boundaries,
    )
    .ok_or_else(|| "errorband lower boundary contains a non-finite coordinate".to_string())?;
    Ok(Some((polygon, upper, lower)))
}

fn path_from_points(points: &[(f64, f64)], close: bool) -> Option<String> {
    let first = *points.first()?;
    if !first.0.is_finite() || !first.1.is_finite() {
        return None;
    }
    let mut path = String::new();
    write!(path, "M {} {}", fmt_num(first.0), fmt_num(first.1)).ok()?;
    for &(x, y) in &points[1..] {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        write!(path, " L {} {}", fmt_num(x), fmt_num(y)).ok()?;
    }
    if close {
        path.push_str(" Z");
    }
    Some(path)
}

fn errorband_color(
    spec: &ChartSpec,
    range: &ErrorRangePoint,
    style: &ErrorPartStyle,
    fill: bool,
) -> Color {
    let series = spec.series.get(range.series_index);
    let base = if fill {
        style.fill.or(style.stroke).or_else(|| {
            series.and_then(|series| series.fill.first().or(series.stroke.first()).copied())
        })
    } else {
        style.stroke.or(style.fill).or_else(|| {
            series.and_then(|series| series.stroke.first().or(series.fill.first()).copied())
        })
    }
    .unwrap_or(spec.theme.text_color);
    Color {
        a: base.a * style.opacity.unwrap_or(error_data(spec).style.opacity) as f32,
        ..base
    }
}

fn errorband_clip(frame: &ErrorMarkFrame) -> Box<ClipRect> {
    Box::new(ClipRect {
        x: frame.plot_left,
        y: frame.plot_top,
        w: (frame.plot_right - frame.plot_left).max(0.0),
        h: (frame.plot_bottom - frame.plot_top).max(0.0),
    })
}

fn push_errorband_fill(
    items: &mut Vec<Prim>,
    path: String,
    fill: Color,
    clip: bool,
    frame: &ErrorMarkFrame,
) {
    if clip {
        items.push(Prim::ClippedPath {
            d: path,
            fill: Some(fill),
            stroke: None,
            stroke_width: 0.0,
            clip: errorband_clip(frame),
        });
    } else {
        items.push(Prim::Path {
            d: path,
            fill: Some(fill),
            stroke: None,
            stroke_width: 0.0,
        });
    }
}

fn push_errorband_border(
    items: &mut Vec<Prim>,
    path: String,
    stroke: Color,
    width: f64,
    dash: &[f64],
    clip: bool,
    frame: &ErrorMarkFrame,
) {
    if dash.is_empty() {
        if clip {
            items.push(Prim::ClippedPath {
                d: path,
                fill: None,
                stroke: Some(stroke),
                stroke_width: width,
                clip: errorband_clip(frame),
            });
        } else {
            items.push(Prim::Path {
                d: path,
                fill: None,
                stroke: Some(stroke),
                stroke_width: width,
            });
        }
        return;
    }

    let styled = Prim::StyledPath {
        d: path,
        stroke,
        stroke_width: width,
        dash: dash.to_vec(),
        dash_offset: 0.0,
    };
    if clip {
        items.push(Prim::Group {
            translate_x: 0.0,
            translate_y: 0.0,
            clip: Some(errorband_clip(frame)),
            children: vec![styled],
        });
    } else {
        items.push(styled);
    }
}

fn draw_errorbands(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    frame: &ErrorMarkFrame,
    groups: &[MappedErrorBand],
) -> Result<(), String> {
    let data = error_data(spec);
    if data.kind != ErrorMarkKind::ErrorBand {
        return Ok(());
    }
    for group in groups {
        let Some((polygon, upper, lower)) =
            errorband_paths(&group.ranges, data.style.interpolation, data.style.tension)?
        else {
            continue;
        };
        if data.style.band.visible {
            let outline = data.style.band.stroke.is_some().then(|| polygon.clone());
            push_errorband_fill(
                items,
                polygon,
                errorband_color(spec, &group.representative, &data.style.band, true),
                data.style.clip,
                frame,
            );
            if let Some(outline) = outline {
                push_errorband_border(
                    items,
                    outline,
                    errorband_color(spec, &group.representative, &data.style.band, false),
                    data.style.band.stroke_width.unwrap_or(DEFAULT_RULE_WIDTH),
                    &data.style.band.stroke_dash,
                    data.style.clip,
                    frame,
                );
            }
        }
        if data.style.borders.visible {
            let color = errorband_color(spec, &group.representative, &data.style.borders, false);
            let width = data
                .style
                .borders
                .stroke_width
                .unwrap_or(DEFAULT_RULE_WIDTH);
            for path in [upper, lower] {
                push_errorband_border(
                    items,
                    path,
                    color,
                    width,
                    &data.style.borders.stroke_dash,
                    data.style.clip,
                    frame,
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn build_axes_scene(
    spec: &ChartSpec,
    m: &TextMeasurer,
    frame: &ErrorMarkFrame,
) -> Scene {
    let mut items = Vec::new();
    draw_chart_title(&mut items, spec, frame);
    draw_axis(&mut items, spec, frame, ErrorAxis::X);
    draw_axis(&mut items, spec, frame, ErrorAxis::Y);
    draw_legend(&mut items, spec, frame, m);
    Scene {
        width: frame.width,
        height: frame.height,
        items,
    }
}

fn build_with_bands_parts(
    spec: &ChartSpec,
    m: &TextMeasurer,
    frame: ErrorMarkFrame,
    bands: Vec<MappedErrorBand>,
) -> Result<(Scene, usize), String> {
    let mut scene = build_axes_scene(spec, m, &frame);
    let mark_start = scene.items.len();
    draw_errorbars(&mut scene.items, spec, &frame)?;
    draw_errorbands(&mut scene.items, spec, &frame, &bands)?;
    let mark_count = scene.items.len() - mark_start;
    Ok((scene, mark_count))
}

fn build_with_bands(
    spec: &ChartSpec,
    m: &TextMeasurer,
    frame: ErrorMarkFrame,
    bands: Vec<MappedErrorBand>,
) -> Result<Scene, String> {
    build_with_bands_parts(spec, m, frame, bands).map(|(scene, _)| scene)
}

/// Builds one error-mark Scene without applying the shared outer theme background pass.
#[cfg(test)]
pub(crate) fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let frame = compute_frame(spec, m);
    let bands = mapped_errorbands(spec, &frame).expect("errorband coordinates must be valid");
    build_with_bands(spec, m, frame, bands).expect("errorband geometry must be finite")
}

pub(crate) fn build_checked(
    spec: &ChartSpec,
    m: &TextMeasurer,
    primitive_limit: usize,
) -> Result<Scene, String> {
    crate::guard::validate_error_mark(spec, primitive_limit)?;
    let frame = compute_frame(spec, m);
    let bands = mapped_errorbands(spec, &frame)?;
    build_with_bands(spec, m, frame, bands)
}

pub(crate) fn build_checked_with_layer_parts(
    spec: &ChartSpec,
    m: &TextMeasurer,
    primitive_limit: usize,
) -> Result<(Scene, usize), String> {
    crate::guard::validate_error_mark(spec, primitive_limit)?;
    let frame = compute_frame(spec, m);
    let bands = mapped_errorbands(spec, &frame)?;
    build_with_bands_parts(spec, m, frame, bands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::TEST_FONT;
    use crate::frontend::vegalite;

    fn parse(json: &str) -> ChartSpec {
        vegalite::parse(json, true).unwrap()
    }

    fn measurer() -> TextMeasurer<'static> {
        TextMeasurer::new(TEST_FONT).unwrap()
    }

    #[test]
    fn error_mark_frame_maps_category_temporal_quantitative_positions() {
        let category = parse(
            r##"{"mark":"errorbar","data":{"values":[{"x":"A","lo":2,"hi":5}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let category_frame = compute_frame(&category, &measurer());
        let category_x = category_frame
            .map_position(ErrorAxis::X, ErrorPosition::Category(0))
            .unwrap();
        assert!(
            (category_x - (category_frame.plot_left + category_frame.plot_right) / 2.0).abs()
                < 1e-6
        );

        let temporal = parse(
            r##"{"mark":"errorbar","data":{"values":[{"x":"2026-01-01T00:00:00Z","lo":2,"hi":5},{"x":"2026-01-03T00:00:00Z","lo":3,"hi":6}]},"encoding":{"x":{"field":"x","type":"temporal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let temporal_frame = compute_frame(&temporal, &measurer());
        let first = temporal_frame
            .map_position(ErrorAxis::X, ErrorPosition::Temporal(1_767_225_600_000))
            .unwrap();
        let last = temporal_frame
            .map_position(ErrorAxis::X, ErrorPosition::Temporal(1_767_398_400_000))
            .unwrap();
        assert!(last > first);

        let quantitative = parse(
            r##"{"mark":"errorbar","data":{"values":[{"x":10,"lo":2,"hi":5},{"x":20,"lo":3,"hi":6}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let quantitative_frame = compute_frame(&quantitative, &measurer());
        let first = quantitative_frame
            .map_position(ErrorAxis::X, ErrorPosition::Quantitative(10.0))
            .unwrap();
        let last = quantitative_frame
            .map_position(ErrorAxis::X, ErrorPosition::Quantitative(20.0))
            .unwrap();
        assert!(last > first);
    }

    #[test]
    fn error_mark_degenerate_ranges_remain_renderable() {
        let spec = parse(
            r##"{"mark":{"type":"errorbar","color":"red"},"data":{"values":[{"x":"A","lo":5,"hi":5}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let scene = build(&spec, &measurer());
        let rule = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    ..
                } if stroke.r == 255 && stroke.g == 0 && stroke.b == 0 => {
                    Some((*x1, *y1, *x2, *y2))
                }
                _ => None,
            })
            .expect("degenerate range emits a visible rule");
        assert!(rule.1.is_finite() && rule.3.is_finite());
        assert_ne!(rule.1, rule.3);
    }

    #[test]
    fn error_mark_errorbar_vertical_horizontal_ranges_map_to_axis_endpoints() {
        let vertical = parse(
            r##"{"mark":{"type":"errorbar","color":"red"},"data":{"values":[{"x":"A","lo":2,"hi":8}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let vertical_frame = compute_frame(&vertical, &measurer());
        let y_lower = vertical_frame
            .map_position(ErrorAxis::Y, ErrorPosition::Quantitative(2.0))
            .unwrap();
        let y_upper = vertical_frame
            .map_position(ErrorAxis::Y, ErrorPosition::Quantitative(8.0))
            .unwrap();
        assert!(
            y_lower > y_upper,
            "the y axis increases upward in data space"
        );
        let vertical_scene = build(&vertical, &measurer());
        let vertical_rule = red_rule(&vertical_scene);
        assert_eq!(vertical_rule.0, vertical_rule.2);
        assert!((vertical_rule.1 - y_lower).abs() < 1e-8);
        assert!((vertical_rule.3 - y_upper).abs() < 1e-8);

        let horizontal = parse(
            r##"{"mark":{"type":"errorbar","orient":"horizontal","color":"red"},"data":{"values":[{"y":"A","lo":2,"hi":8}]},"encoding":{"x":{"field":"lo","type":"quantitative"},"x2":{"field":"hi"},"y":{"field":"y","type":"nominal"}}}"##,
        );
        let horizontal_frame = compute_frame(&horizontal, &measurer());
        let x_lower = horizontal_frame
            .map_position(ErrorAxis::X, ErrorPosition::Quantitative(2.0))
            .unwrap();
        let x_upper = horizontal_frame
            .map_position(ErrorAxis::X, ErrorPosition::Quantitative(8.0))
            .unwrap();
        assert!(x_lower < x_upper);
        let horizontal_scene = build(&horizontal, &measurer());
        let horizontal_rule = red_rule(&horizontal_scene);
        assert!((horizontal_rule.0 - x_lower).abs() < 1e-8);
        assert!((horizontal_rule.2 - x_upper).abs() < 1e-8);
        assert_eq!(horizontal_rule.1, horizontal_rule.3);
    }

    fn red_rule(scene: &Scene) -> (f64, f64, f64, f64) {
        scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    ..
                } if stroke.r == 255 && stroke.g == 0 && stroke.b == 0 => {
                    Some((*x1, *y1, *x2, *y2))
                }
                _ => None,
            })
            .expect("error rule")
    }

    #[test]
    fn error_mark_log_values_fail_and_hard_bounds_clip() {
        let mut log = parse(
            r##"{"mark":"errorbar","data":{"values":[{"x":"A","lo":-1,"hi":5}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        log.y_axis.scale_kind = ScaleKind::Logarithmic;
        assert!(build_checked(&log, &measurer(), 100).is_err());

        let mut log_independent = parse(
            r##"{"mark":"errorbar","data":{"values":[{"x":0,"lo":2,"hi":5}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        log_independent.x_axis.scale_kind = ScaleKind::Logarithmic;
        assert!(build_checked(&log_independent, &measurer(), 100).is_err());

        let mut clipped = parse(
            r##"{"width":320,"height":220,"mark":{"type":"errorbar","color":"red"},"data":{"values":[{"x":"A","lo":-5,"hi":10}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        clipped.y_axis.min = Some(0.0);
        clipped.y_axis.max = Some(5.0);
        let frame = compute_frame(&clipped, &measurer());
        let scene = build(&clipped, &measurer());
        let rule = scene
            .items
            .iter()
            .find_map(|item| match item {
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    ..
                } if stroke.r == 255 && stroke.g == 0 && stroke.b == 0 => {
                    Some((*x1, *y1, *x2, *y2))
                }
                _ => None,
            })
            .expect("hard-bounded range remains visible");
        let (rule_top, rule_bottom) = if rule.1 <= rule.3 {
            (rule.1, rule.3)
        } else {
            (rule.3, rule.1)
        };
        assert!((rule_top - frame.plot_top).abs() < 1e-8);
        assert!((rule_bottom - frame.plot_bottom).abs() < 1e-8);
    }

    #[test]
    fn errorbar_extreme_endpoint_clips_before_pixel_mapping() {
        let mut spec = parse(
            r##"{"width":320,"height":220,"mark":{"type":"errorbar","color":"red"},"data":{"values":[{"x":"A","lo":1,"hi":1e308}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        spec.y_axis.min = Some(1.0);
        spec.y_axis.max = Some(2.0);
        let frame = compute_frame(&spec, &measurer());

        let scene = build_checked(&spec, &measurer(), 100)
            .expect("clipping a finite endpoint to hard bounds should remain renderable");
        let rule = red_rule(&scene);
        let (rule_top, rule_bottom) = if rule.1 <= rule.3 {
            (rule.1, rule.3)
        } else {
            (rule.3, rule.1)
        };
        assert!((rule_top - frame.plot_top).abs() < 1e-8);
        assert!((rule_bottom - frame.plot_bottom).abs() < 1e-8);

        if let ChartKind::ErrorMark(data) = &mut spec.kind {
            data.style.clip = false;
        }
        assert!(
            build_checked(&spec, &measurer(), 100).is_err(),
            "an un-clipped endpoint that maps to infinity must return an error"
        );
    }

    #[test]
    fn errorbar_clipping_does_not_pin_an_out_of_domain_tick_to_plot_edge() {
        let mut spec = parse(
            r##"{"width":320,"height":220,"mark":{"type":"errorbar","color":"blue","ticks":{"stroke":"red"}},"data":{"values":[{"x":"A","lo":1,"hi":10}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        spec.y_axis.min = Some(0.0);
        spec.y_axis.max = Some(5.0);
        let frame = compute_frame(&spec, &measurer());
        let scene = build_checked(&spec, &measurer(), 100).unwrap();
        let horizontal_red_lines = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    ..
                } if stroke.r == 255
                    && stroke.g == 0
                    && stroke.b == 0
                    && (x1 - x2).abs() > 0.0
                    && (y1 - y2).abs() < f64::EPSILON =>
                {
                    Some((*x1, *y1, *x2, *y2))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(
            horizontal_red_lines
                .iter()
                .any(|line| { line.1 > frame.plot_top && line.1 < frame.plot_bottom })
        );
        assert!(
            horizontal_red_lines
                .iter()
                .all(|line| (line.1 - frame.plot_top).abs() >= 1e-8)
        );
    }

    #[test]
    fn error_mark_y_axis_title_moves_right_of_left_legend() {
        let mut spec = parse(
            r##"{"width":360,"height":240,"mark":"errorbar","data":{"values":[{"x":"Mon","lo":1,"hi":2,"site":"Alpha"},{"x":"Tue","lo":2,"hi":3,"site":"Beta"}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"},"color":{"field":"site","type":"nominal"}}}"##,
        );
        spec.legend = LegendPos::Left;
        let scene = build(&spec, &measurer());
        let y_title = scene.items.iter().find_map(|item| match item {
            Prim::Text {
                x,
                size,
                content,
                rotate_deg: Some(-90.0),
                ..
            } if content == "lo" => Some((*x, *size)),
            _ => None,
        });
        let (title_x, title_size) = y_title.expect("y-axis title");
        assert!(title_x > OUTER_PAD + title_size / 2.0);

        spec.legend = LegendPos::None;
        let scene = build(&spec, &measurer());
        let no_legend_title_x = scene.items.iter().find_map(|item| match item {
            Prim::Text {
                x,
                content,
                rotate_deg: Some(-90.0),
                ..
            } if content == "lo" => Some(*x),
            _ => None,
        });
        assert!((no_legend_title_x.unwrap() - (OUTER_PAD + title_size / 2.0)).abs() < 1e-8);
    }

    #[test]
    fn errorband_extreme_endpoint_clips_before_pixel_mapping() {
        let mut spec = parse(
            r##"{"width":320,"height":220,"mark":"errorband","data":{"values":[{"x":0,"lo":1,"hi":1e308},{"x":1,"lo":1,"hi":1e308}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        spec.y_axis.min = Some(1.0);
        spec.y_axis.max = Some(2.0);

        let scene = build_checked(&spec, &measurer(), 100)
            .expect("clipping finite errorband endpoints to hard bounds should render");
        assert!(
            scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::ClippedPath { .. }))
        );
    }

    #[test]
    fn errorbar_1d_uses_plot_center_on_the_other_axis() {
        let spec = parse(
            r##"{"mark":{"type":"errorbar","orient":"vertical","color":"red"},"data":{"values":[{"lo":2,"hi":8}]},"encoding":{"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let frame = compute_frame(&spec, &measurer());
        let x = frame
            .map_position(ErrorAxis::X, ErrorPosition::FullAxis)
            .unwrap();
        assert!((x - (frame.plot_left + frame.plot_right) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn errorbar_ticks_are_optional_and_use_part_styles() {
        let spec = parse(
            r##"{"mark":{"type":"errorbar","color":"red","ticks":{"color":"green","size":12}},"data":{"values":[{"x":"A","lo":2,"hi":8}]},"encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"lo","type":"quantitative"},"y2":{"field":"hi"}}}"##,
        );
        let scene = build(&spec, &measurer());
        let green = Color {
            r: 0,
            g: 128,
            b: 0,
            a: 1.0,
        };
        let caps = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    ..
                } if *stroke == green => Some((*x1, *y1, *x2, *y2)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(caps.len(), 2);
        assert!(caps.iter().all(|(x1, _, x2, _)| (x1 - x2).abs() > 0.0));
    }

    #[test]
    fn errorband_open_and_closed_interpolations_preserve_curve_end_conditions() {
        let upper = vec![(0.0, 1.0), (1.0, 4.0), (2.0, 2.0), (3.0, 5.0), (4.0, 3.0)];
        let lower = upper.iter().map(|(x, y)| (*x, y - 1.0)).collect::<Vec<_>>();
        let ranges = upper
            .iter()
            .zip(&lower)
            .map(|((independent, upper), (_, lower))| MappedErrorRange {
                independent: *independent,
                lower: *lower,
                upper: *upper,
                horizontal: false,
            })
            .collect::<Vec<_>>();
        let short_ranges = ranges[..3].to_vec();
        for interpolation in [
            ErrorBandInterpolation::BasisOpen,
            ErrorBandInterpolation::CardinalOpen,
        ] {
            assert!(
                errorband_paths(&short_ranges, interpolation, 0.25)
                    .unwrap()
                    .is_none(),
                "{interpolation:?} should produce no path with too few control points"
            );
        }

        for (open, closed) in [
            (
                ErrorBandInterpolation::Linear,
                ErrorBandInterpolation::LinearClosed,
            ),
            (
                ErrorBandInterpolation::Basis,
                ErrorBandInterpolation::BasisClosed,
            ),
            (
                ErrorBandInterpolation::Cardinal,
                ErrorBandInterpolation::CardinalClosed,
            ),
        ] {
            let (_, open_upper, _) = errorband_paths(&ranges, open, 0.25).unwrap().unwrap();
            let (_, closed_upper, _) = errorband_paths(&ranges, closed, 0.25).unwrap().unwrap();
            assert!(!open_upper.ends_with('Z'), "{open:?} must remain open");
            assert!(
                closed_upper.ends_with('Z'),
                "{closed:?} must close its boundary"
            );
            assert_ne!(open_upper, closed_upper, "{closed:?} must wrap the curve");
        }

        let basis_open = boundary_samples(&upper, ErrorBandInterpolation::BasisOpen, 0.25);
        let basis_open_start = (
            (upper[0].0 + 4.0 * upper[1].0 + upper[2].0) / 6.0,
            (upper[0].1 + 4.0 * upper[1].1 + upper[2].1) / 6.0,
        );
        let basis_open_end = (
            (upper[2].0 + 4.0 * upper[3].0 + upper[4].0) / 6.0,
            (upper[2].1 + 4.0 * upper[3].1 + upper[4].1) / 6.0,
        );
        assert_eq!(basis_open.first(), Some(&basis_open_start));
        assert_eq!(basis_open.last(), Some(&basis_open_end));

        let cardinal_open = boundary_samples(&upper, ErrorBandInterpolation::CardinalOpen, 0.25);
        let k = (1.0 - 0.25) / 6.0;
        let (p0, p1, p2, p3) = (upper[0], upper[1], upper[2], upper[3]);
        let cp1 = (p1.0 + k * (p2.0 - p0.0), p1.1 + k * (p2.1 - p0.1));
        let cp2 = (p2.0 + k * (p1.0 - p3.0), p2.1 + k * (p1.1 - p3.1));
        let expected_first_sample = cubic_point(p1, cp1, cp2, p2, 1.0 / 8.0);
        assert_eq!(cardinal_open.first(), Some(&p1));
        assert_eq!(cardinal_open.get(1), Some(&expected_first_sample));
        let cardinal_closed =
            boundary_samples(&upper, ErrorBandInterpolation::CardinalClosed, 0.25);
        assert_eq!(cardinal_closed.first(), cardinal_closed.last());
    }
}
