//! Scene layout for the Vega-Lite `boxplot` composite mark.

use crate::ir::{
    AxisSpec, ChartKind, ChartSpec, Color, VegaBoxPlotData, VegaBoxPlotGroup, VegaBoxPlotOrient,
    VegaBoxPlotPartStyle,
};
use crate::scale::{LinearScale, NiceTicks, nice_ticks};
use crate::scene::{Anchor, ClipRect, Prim, Scene};
use crate::text::TextMeasurer;

#[derive(Clone, Debug)]
pub(crate) struct VegaBoxPlotFrame {
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
    pub value_ticks: NiceTicks,
    pub value_min: f64,
    pub value_max: f64,
}

fn data(spec: &ChartSpec) -> &VegaBoxPlotData {
    let ChartKind::VegaBoxPlot(data) = &spec.kind else {
        unreachable!("Vega-Lite boxplot layout only accepts ChartKind::VegaBoxPlot")
    };
    data
}

fn value_axis<'a>(spec: &'a ChartSpec, data: &VegaBoxPlotData) -> &'a AxisSpec {
    match data.orient {
        VegaBoxPlotOrient::Vertical => &spec.y_axis,
        VegaBoxPlotOrient::Horizontal => &spec.x_axis,
    }
}

fn data_domain(data: &VegaBoxPlotData) -> (f64, f64) {
    data.groups
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), group| {
            (
                low.min(group.summary.data_min),
                high.max(group.summary.data_max),
            )
        })
}

pub(crate) fn compute_frame(spec: &ChartSpec, m: &TextMeasurer<'_>) -> VegaBoxPlotFrame {
    let data = data(spec);
    let (data_min, data_max) = data_domain(data);
    let axis = value_axis(spec, data);
    let domain_min = axis.min.unwrap_or(data_min);
    let domain_max = axis.max.unwrap_or(data_max);
    let ticks = crate::layout::common::apply_hard_axis_bounds(
        nice_ticks(domain_min, domain_max, axis.ticks.count.unwrap_or(7)),
        axis,
    );
    let label_font = spec.theme.font_size;
    let max_value_label = ticks
        .ticks
        .iter()
        .map(|tick| m.width(&crate::num::fmt_num(*tick), label_font as f32) as f64)
        .fold(0.0_f64, f64::max);
    let max_category_label = data
        .categories
        .iter()
        .map(|label| m.width(label, label_font as f32) as f64)
        .fold(0.0_f64, f64::max);
    let has_title = spec.title.as_ref().is_some_and(|title| !title.is_empty());
    let has_legend = spec.legend != crate::ir::LegendPos::None;
    let top = 14.0
        + if has_title { 20.0 } else { 0.0 }
        + if has_legend { label_font + 10.0 } else { 0.0 };
    let (left, bottom) = match data.orient {
        VegaBoxPlotOrient::Vertical => (max_value_label + 18.0, label_font + 18.0),
        VegaBoxPlotOrient::Horizontal => (
            if data.has_category {
                max_category_label + 18.0
            } else {
                14.0
            },
            label_font + 18.0,
        ),
    };
    let right = 14.0;
    let plot_left = left.min((spec.width - 2.0).max(0.0));
    let plot_right = (spec.width - right).max(plot_left + 1.0);
    let plot_top = top.min((spec.height - 2.0).max(0.0));
    let plot_bottom = (spec.height - bottom).max(plot_top + 1.0);
    VegaBoxPlotFrame {
        plot_left,
        plot_right,
        plot_top,
        plot_bottom,
        value_min: ticks.min,
        value_max: ticks.max,
        value_ticks: ticks,
    }
}

fn value_scale(frame: &VegaBoxPlotFrame, orient: VegaBoxPlotOrient) -> LinearScale {
    match orient {
        VegaBoxPlotOrient::Vertical => LinearScale::new(
            frame.value_min,
            frame.value_max,
            frame.plot_bottom,
            frame.plot_top,
        ),
        VegaBoxPlotOrient::Horizontal => LinearScale::new(
            frame.value_min,
            frame.value_max,
            frame.plot_left,
            frame.plot_right,
        ),
    }
}

fn category_center(
    data: &VegaBoxPlotData,
    frame: &VegaBoxPlotFrame,
    group_index: usize,
    category: Option<usize>,
) -> f64 {
    let orient = data.orient;
    if !data.has_category {
        return match orient {
            VegaBoxPlotOrient::Vertical => (frame.plot_left + frame.plot_right) / 2.0,
            VegaBoxPlotOrient::Horizontal => (frame.plot_top + frame.plot_bottom) / 2.0,
        };
    }
    let category_index = category.unwrap_or(0);
    let count = data.categories.len().max(1);
    let (start, span) = match orient {
        VegaBoxPlotOrient::Vertical => (
            frame.plot_left,
            (frame.plot_right - frame.plot_left) / count as f64,
        ),
        VegaBoxPlotOrient::Horizontal => (
            frame.plot_top,
            (frame.plot_bottom - frame.plot_top) / count as f64,
        ),
    };
    let same_category = data
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.category_index == Some(category_index))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let slot = same_category
        .iter()
        .position(|index| *index == group_index)
        .unwrap_or(0);
    let slots = same_category.len().max(1);
    let side_by_side = span * 0.68;
    start
        + (category_index as f64 + 0.5) * span
        + ((slot as f64 + 0.5) / slots as f64 - 0.5) * side_by_side
}

fn category_band(data: &VegaBoxPlotData, frame: &VegaBoxPlotFrame) -> f64 {
    let count = data.categories.len().max(1) as f64;
    match data.orient {
        VegaBoxPlotOrient::Vertical => (frame.plot_right - frame.plot_left) / count,
        VegaBoxPlotOrient::Horizontal => (frame.plot_bottom - frame.plot_top) / count,
    }
}

fn component_color(
    color: Color,
    global_opacity: f64,
    group_opacity: Option<f64>,
    part: &VegaBoxPlotPartStyle,
) -> Color {
    let opacity = part
        .opacity
        .unwrap_or_else(|| group_opacity.unwrap_or(global_opacity));
    Color {
        a: (color.a as f64 * opacity).clamp(0.0, 1.0) as f32,
        ..color
    }
}

fn mapped_value(frame: &VegaBoxPlotFrame, data: &VegaBoxPlotData, value: f64) -> f64 {
    value_scale(frame, data.orient).map(value)
}

fn add_line(
    items: &mut Vec<Prim>,
    start: (f64, f64),
    end: (f64, f64),
    color: Color,
    width: f64,
    dash: &[f64],
) {
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

fn component_width(part: &VegaBoxPlotPartStyle, group: &VegaBoxPlotGroup, default: f64) -> f64 {
    part.size.or(group.size).unwrap_or(default).max(0.0)
}

fn draw_axes(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    data: &VegaBoxPlotData,
    frame: &VegaBoxPlotFrame,
) {
    let ink = spec.theme.text_color;
    let value_axis = value_axis(spec, data);
    let grid = value_axis.grid.color.unwrap_or(spec.theme.grid_color);
    let font = spec.theme.font_size;
    let scale = value_scale(frame, data.orient);
    for &tick in &frame.value_ticks.ticks {
        let label = crate::num::fmt_num(tick);
        match data.orient {
            VegaBoxPlotOrient::Vertical => {
                let y = scale.map(tick);
                if value_axis.grid.display {
                    add_line(
                        items,
                        (frame.plot_left, y),
                        (frame.plot_right, y),
                        grid,
                        1.0,
                        &[],
                    );
                }
                items.push(Prim::Text {
                    x: frame.plot_left - 6.0,
                    y: y + font * 0.35,
                    size: font,
                    anchor: Anchor::End,
                    fill: ink,
                    content: label,
                    rotate_deg: None,
                });
            }
            VegaBoxPlotOrient::Horizontal => {
                let x = scale.map(tick);
                if value_axis.grid.display {
                    add_line(
                        items,
                        (x, frame.plot_top),
                        (x, frame.plot_bottom),
                        grid,
                        1.0,
                        &[],
                    );
                }
                items.push(Prim::Text {
                    x,
                    y: frame.plot_bottom + font + 5.0,
                    size: font,
                    anchor: Anchor::Middle,
                    fill: ink,
                    content: label,
                    rotate_deg: None,
                });
            }
        }
    }
    match data.orient {
        VegaBoxPlotOrient::Vertical => {
            add_line(
                items,
                (frame.plot_left, frame.plot_bottom),
                (frame.plot_right, frame.plot_bottom),
                ink,
                1.0,
                &[],
            );
            for (index, label) in data.categories.iter().enumerate() {
                let x = frame.plot_left
                    + (index as f64 + 0.5) * (frame.plot_right - frame.plot_left)
                        / data.categories.len() as f64;
                items.push(Prim::Text {
                    x,
                    y: frame.plot_bottom + font + 5.0,
                    size: font,
                    anchor: Anchor::Middle,
                    fill: ink,
                    content: label.clone(),
                    rotate_deg: None,
                });
            }
        }
        VegaBoxPlotOrient::Horizontal => {
            add_line(
                items,
                (frame.plot_left, frame.plot_top),
                (frame.plot_left, frame.plot_bottom),
                ink,
                1.0,
                &[],
            );
            for (index, label) in data.categories.iter().enumerate() {
                let y = frame.plot_top
                    + (index as f64 + 0.5) * (frame.plot_bottom - frame.plot_top)
                        / data.categories.len() as f64;
                items.push(Prim::Text {
                    x: frame.plot_left - 6.0,
                    y: y + font * 0.35,
                    size: font,
                    anchor: Anchor::End,
                    fill: ink,
                    content: label.clone(),
                    rotate_deg: None,
                });
            }
        }
    }
    if let Some(title) = spec.title.as_ref().filter(|title| !title.is_empty()) {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: 18.0,
            size: 14.0,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }
}

fn color_legend_groups(
    spec: &ChartSpec,
    data: &VegaBoxPlotData,
    shared_categories: Option<&[String]>,
) -> Vec<(String, Color)> {
    if let Some(categories) = shared_categories {
        return categories
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let color = data
                    .groups
                    .iter()
                    .find(|group| group.color_label.as_deref() == Some(label.as_str()))
                    .map(|group| group.color)
                    .or_else(|| {
                        let palette = &spec.theme.palette;
                        (!palette.is_empty()).then(|| palette[index % palette.len()])
                    })
                    .unwrap_or(
                        crate::palette::VEGALITE_PALETTE
                            [index % crate::palette::VEGALITE_PALETTE.len()],
                    );
                (label.clone(), color)
            })
            .collect();
    }

    let mut groups = Vec::<(&str, Color)>::new();
    for group in &data.groups {
        let Some(label) = group.color_label.as_deref() else {
            continue;
        };
        if !groups.iter().any(|(known, _)| *known == label) {
            groups.push((label, group.color));
        }
    }
    groups
        .into_iter()
        .map(|(label, color)| (label.to_owned(), color))
        .collect()
}

fn draw_legend(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    data: &VegaBoxPlotData,
    m: &TextMeasurer<'_>,
    shared_categories: Option<&[String]>,
) {
    let groups = color_legend_groups(spec, data, shared_categories);
    if groups.is_empty() {
        return;
    }
    let font = spec.theme.font_size;
    let y = if spec.title.is_some() { 34.0 } else { 14.0 };
    let mut x = 14.0;
    for (label, color) in groups {
        items.push(Prim::Rect {
            x,
            y,
            w: 9.0,
            h: 9.0,
            fill: color,
        });
        x += 13.0;
        let width = m.width(&label, font as f32) as f64;
        items.push(Prim::Text {
            x,
            y: y + font * 0.8,
            size: font,
            anchor: Anchor::Start,
            fill: spec.theme.text_color,
            content: label,
            rotate_deg: None,
        });
        x += width + 18.0;
    }
}

fn draw_group(
    items: &mut Vec<Prim>,
    data: &VegaBoxPlotData,
    frame: &VegaBoxPlotFrame,
    group: &VegaBoxPlotGroup,
    group_index: usize,
) {
    let black = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 1.0,
    };
    let white = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 1.0,
    };
    let center = category_center(data, frame, group_index, group.category_index);
    let band = category_band(data, frame);
    let default_width = if data.has_category {
        (band * 0.68
            / data
                .groups
                .iter()
                .filter(|candidate| candidate.category_index == group.category_index)
                .count()
                .max(1) as f64)
            .min(48.0)
    } else {
        40.0
    };
    let style = &data.style;
    let width = group.size.unwrap_or(default_width);
    if let (Some(low), Some(high)) = (group.summary.whisker_low, group.summary.whisker_high)
        && style.rule_part.visible
    {
        let color = component_color(
            style
                .rule_part
                .stroke
                .or(style.rule_part.fill)
                .unwrap_or(black),
            style.opacity,
            group.opacity,
            &style.rule_part,
        );
        let stroke_width = style.rule_part.stroke_width.unwrap_or(1.0);
        let dash = &style.rule_part.stroke_dash;
        let point = |value| {
            let value = mapped_value(frame, data, value);
            match data.orient {
                VegaBoxPlotOrient::Vertical => (center, value),
                VegaBoxPlotOrient::Horizontal => (value, center),
            }
        };
        if style.box_part.visible {
            let q1 = group.summary.q1;
            let q3 = group.summary.q3;
            if low < q1 {
                add_line(items, point(low), point(q1), color, stroke_width, dash);
            }
            if q3 < high {
                add_line(items, point(q3), point(high), color, stroke_width, dash);
            }
        } else if low < high {
            add_line(items, point(low), point(high), color, stroke_width, dash);
        }
    }
    if style.box_part.visible {
        let fill = component_color(
            style.box_part.fill.unwrap_or(group.color),
            style.opacity,
            group.opacity,
            &style.box_part,
        );
        let width = component_width(&style.box_part, group, default_width);
        let q1 = mapped_value(frame, data, group.summary.q1);
        let q3 = mapped_value(frame, data, group.summary.q3);
        let (x, y, w, h) = match data.orient {
            VegaBoxPlotOrient::Vertical => (center - width / 2.0, q3, width, q1 - q3),
            VegaBoxPlotOrient::Horizontal => (q1, center - width / 2.0, q3 - q1, width),
        };
        items.push(Prim::Rect {
            x,
            y,
            w: w.max(0.0),
            h: h.max(0.0),
            fill,
        });
        if let Some(box_stroke) = style.box_part.stroke {
            let stroke = component_color(box_stroke, style.opacity, group.opacity, &style.box_part);
            let stroke_width = style.box_part.stroke_width.unwrap_or(1.0);
            let dash = &style.box_part.stroke_dash;
            add_line(items, (x, y), (x + w, y), stroke, stroke_width, dash);
            add_line(
                items,
                (x + w, y),
                (x + w, y + h),
                stroke,
                stroke_width,
                dash,
            );
            add_line(
                items,
                (x + w, y + h),
                (x, y + h),
                stroke,
                stroke_width,
                dash,
            );
            add_line(items, (x, y + h), (x, y), stroke, stroke_width, dash);
        }
    }
    if style.median_part.visible {
        let color = component_color(
            style
                .median_part
                .stroke
                .or(style.median_part.fill)
                .unwrap_or(white),
            style.opacity,
            group.opacity,
            &style.median_part,
        );
        let median = mapped_value(frame, data, group.summary.median);
        let tick_width = component_width(&style.median_part, group, width);
        let (x1, y1, x2, y2) = match data.orient {
            VegaBoxPlotOrient::Vertical => (
                center - tick_width / 2.0,
                median,
                center + tick_width / 2.0,
                median,
            ),
            VegaBoxPlotOrient::Horizontal => (
                median,
                center - tick_width / 2.0,
                median,
                center + tick_width / 2.0,
            ),
        };
        add_line(
            items,
            (x1, y1),
            (x2, y2),
            color,
            style.median_part.stroke_width.unwrap_or(1.0),
            &style.median_part.stroke_dash,
        );
    }
    if let (Some(low), Some(high)) = (group.summary.whisker_low, group.summary.whisker_high)
        && style.ticks_part.visible
    {
        let color = component_color(
            style
                .ticks_part
                .stroke
                .or(style.ticks_part.fill)
                .unwrap_or(black),
            style.opacity,
            group.opacity,
            &style.ticks_part,
        );
        let cap = component_width(&style.ticks_part, group, width * 0.55);
        for endpoint in [low, high] {
            let value = mapped_value(frame, data, endpoint);
            let (x1, y1, x2, y2) = match data.orient {
                VegaBoxPlotOrient::Vertical => {
                    (center - cap / 2.0, value, center + cap / 2.0, value)
                }
                VegaBoxPlotOrient::Horizontal => {
                    (value, center - cap / 2.0, value, center + cap / 2.0)
                }
            };
            add_line(
                items,
                (x1, y1),
                (x2, y2),
                color,
                style.ticks_part.stroke_width.unwrap_or(1.0),
                &style.ticks_part.stroke_dash,
            );
        }
    }
    if style.outliers_part.visible {
        let transparent = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        };
        let fill = component_color(
            style.outliers_part.fill.unwrap_or(transparent),
            style.opacity,
            group.opacity,
            &style.outliers_part,
        );
        let stroke = component_color(
            style.outliers_part.stroke.unwrap_or(group.color),
            style.opacity,
            group.opacity,
            &style.outliers_part,
        );
        let size = style.outliers_part.size.unwrap_or(30.0);
        let radius = (size / std::f64::consts::PI).sqrt();
        let stroke_width = style.outliers_part.stroke_width.unwrap_or(1.0);
        for &outlier in &group.summary.outliers {
            let value = mapped_value(frame, data, outlier);
            let (cx, cy) = match data.orient {
                VegaBoxPlotOrient::Vertical => (center, value),
                VegaBoxPlotOrient::Horizontal => (value, center),
            };
            if style.outliers_part.stroke_dash.is_empty() {
                items.push(Prim::Circle {
                    cx,
                    cy,
                    r: radius,
                    fill,
                    stroke,
                    stroke_width,
                });
            } else {
                items.push(Prim::Circle {
                    cx,
                    cy,
                    r: radius,
                    fill,
                    stroke: transparent,
                    stroke_width: 0.0,
                });
                if stroke_width > 0.0 {
                    items.push(Prim::StyledPath {
                        d: circle_outline_path(cx, cy, radius),
                        stroke,
                        stroke_width,
                        dash: style.outliers_part.stroke_dash.clone(),
                        dash_offset: 0.0,
                    });
                }
            }
        }
    }
}

fn circle_outline_path(cx: f64, cy: f64, radius: f64) -> String {
    let control = radius * 0.552_284_749_830_793_6;
    let num = crate::num::fmt_num;
    format!(
        "M {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} Z",
        num(cx + radius),
        num(cy),
        num(cx + radius),
        num(cy + control),
        num(cx + control),
        num(cy + radius),
        num(cx),
        num(cy + radius),
        num(cx - control),
        num(cy + radius),
        num(cx - radius),
        num(cy + control),
        num(cx - radius),
        num(cy),
        num(cx - radius),
        num(cy - control),
        num(cx - control),
        num(cy - radius),
        num(cx),
        num(cy - radius),
        num(cx + control),
        num(cy - radius),
        num(cx + radius),
        num(cy - control),
        num(cx + radius),
        num(cy),
    )
}

fn ensure_finite_geometry(items: &[Prim]) -> Result<(), String> {
    for item in items {
        let finite = match item {
            Prim::Rect { x, y, w, h, .. } => [*x, *y, *w, *h].into_iter().all(f64::is_finite),
            Prim::Line {
                x1,
                y1,
                x2,
                y2,
                stroke_width,
                dash,
                ..
            } => [*x1, *y1, *x2, *y2, *stroke_width]
                .into_iter()
                .chain(dash.iter().copied())
                .all(f64::is_finite),
            Prim::Circle {
                cx,
                cy,
                r,
                stroke_width,
                ..
            } => [*cx, *cy, *r, *stroke_width]
                .into_iter()
                .all(f64::is_finite),
            Prim::Text {
                x,
                y,
                size,
                rotate_deg,
                ..
            } => [Some(*x), Some(*y), Some(*size), *rotate_deg]
                .into_iter()
                .flatten()
                .all(f64::is_finite),
            Prim::Group {
                translate_x,
                translate_y,
                clip,
                children,
            } => {
                [*translate_x, *translate_y].into_iter().all(f64::is_finite)
                    && clip.as_deref().is_none_or(|clip| {
                        [clip.x, clip.y, clip.w, clip.h]
                            .into_iter()
                            .all(f64::is_finite)
                    })
                    && ensure_finite_geometry(children).is_ok()
            }
            _ => true,
        };
        if !finite {
            return Err("Vega-Lite boxplot generated non-finite geometry".into());
        }
    }
    Ok(())
}

pub(crate) fn build_checked(
    spec: &ChartSpec,
    m: &TextMeasurer<'_>,
    limits: &crate::guard::InputLimits,
) -> Result<Scene, String> {
    build_checked_with_layer_parts(spec, m, limits, None).map(|(scene, _)| scene)
}

pub(crate) fn build_checked_with_layer_parts(
    spec: &ChartSpec,
    m: &TextMeasurer<'_>,
    limits: &crate::guard::InputLimits,
    shared_color_categories: Option<&[String]>,
) -> Result<(Scene, usize), String> {
    crate::guard::validate_vega_boxplot(spec, limits)?;
    let data = data(spec);
    let frame = compute_frame(spec, m);
    if ![
        frame.plot_left,
        frame.plot_right,
        frame.plot_top,
        frame.plot_bottom,
        frame.value_min,
        frame.value_max,
    ]
    .into_iter()
    .all(f64::is_finite)
    {
        return Err("Vega-Lite boxplot frame must be finite".into());
    }
    let mut items = Vec::new();
    draw_axes(&mut items, spec, data, &frame);
    draw_legend(&mut items, spec, data, m, shared_color_categories);
    let mut marks = Vec::new();
    for (index, group) in data.groups.iter().enumerate() {
        draw_group(&mut marks, data, &frame, group, index);
    }
    let mark_count = if data.style.clip {
        items.push(Prim::Group {
            translate_x: 0.0,
            translate_y: 0.0,
            clip: Some(Box::new(ClipRect {
                x: frame.plot_left,
                y: frame.plot_top,
                w: frame.plot_right - frame.plot_left,
                h: frame.plot_bottom - frame.plot_top,
            })),
            children: marks,
        });
        1
    } else {
        let mark_count = marks.len();
        items.extend(marks);
        mark_count
    };
    ensure_finite_geometry(&items)?;
    Ok((
        Scene {
            width: spec.width,
            height: spec.height,
            items,
        },
        mark_count,
    ))
}
