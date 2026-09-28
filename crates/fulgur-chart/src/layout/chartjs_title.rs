use crate::guard::DEFAULT_MAX_DIMENSION_PX;
use crate::ir::{ChartJsTitle, ChartJsTitleAlign, ChartJsTitlePosition, ChartSpec, SizeMode};
use crate::scene::{Anchor, ClipRect, Prim, StyledText};

const MAX_TITLE_THICKNESS: f64 = DEFAULT_MAX_DIMENSION_PX;
const MAX_TOTAL_INSET: f64 = DEFAULT_MAX_DIMENSION_PX * 2.0;
const MAX_OUTPUT_DIMENSION: f64 = DEFAULT_MAX_DIMENSION_PX * 3.0;

#[derive(Clone, Debug)]
#[allow(dead_code)] // All four margins are part of the shared layout contract and inspection API.
pub(crate) struct ChartJsTitleLayout {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub scene_width: f64,
    pub scene_height: f64,
    pub text_items: Vec<Prim>,
}

#[derive(Clone, Copy, Default)]
struct Insets {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Insets {
    fn get(self, position: ChartJsTitlePosition) -> f64 {
        match position {
            ChartJsTitlePosition::Top => self.top,
            ChartJsTitlePosition::Right => self.right,
            ChartJsTitlePosition::Bottom => self.bottom,
            ChartJsTitlePosition::Left => self.left,
        }
    }

    fn add(&mut self, position: ChartJsTitlePosition, thickness: f64) {
        let slot = match position {
            ChartJsTitlePosition::Top => &mut self.top,
            ChartJsTitlePosition::Right => &mut self.right,
            ChartJsTitlePosition::Bottom => &mut self.bottom,
            ChartJsTitlePosition::Left => &mut self.left,
        };
        *slot = bounded_add(*slot, thickness, MAX_TOTAL_INSET);
    }
}

pub(crate) fn has_visible_chartjs_titles(spec: &ChartSpec) -> bool {
    spec.chartjs_title
        .as_ref()
        .is_some_and(|title| title.display)
        || spec
            .chartjs_subtitle
            .as_ref()
            .is_some_and(|subtitle| subtitle.display)
}

/// Resolve title boxes and the viewport left for the chart scene.
pub(crate) fn chartjs_title_layout(
    spec: &ChartSpec,
    base_scene_width: f64,
    base_scene_height: f64,
) -> Option<ChartJsTitleLayout> {
    let titles = [spec.chartjs_title.as_ref(), spec.chartjs_subtitle.as_ref()]
        .into_iter()
        .flatten()
        .filter(|title| title.display)
        .collect::<Vec<_>>();
    if titles.is_empty() {
        return None;
    }

    let mut insets = Insets::default();
    for title in &titles {
        insets.add(title.position, title_thickness(title));
    }

    let base_width = safe_dimension(base_scene_width);
    let base_height = safe_dimension(base_scene_height);
    let canvas_width = safe_dimension(spec.width);
    let canvas_height = safe_dimension(spec.height);
    let (scene_width, scene_height, viewport_width, viewport_height) =
        if matches!(spec.size_mode, SizeMode::Canvas) {
            (
                canvas_width,
                canvas_height,
                (base_width - insets.left - insets.right).max(0.0),
                (base_height - insets.top - insets.bottom).max(0.0),
            )
        } else {
            (
                bounded_output_sum(base_width, insets.left, insets.right),
                bounded_output_sum(base_height, insets.top, insets.bottom),
                base_width,
                base_height,
            )
        };

    let layout = ChartJsTitleLayout {
        left: insets.left,
        top: insets.top,
        right: insets.right,
        bottom: insets.bottom,
        viewport_width,
        viewport_height,
        scene_width,
        scene_height,
        text_items: Vec::with_capacity(titles.len()),
    };

    let mut used = Insets::default();
    let mut text_items = Vec::with_capacity(titles.len());
    for title in titles {
        let thickness = title_thickness(title);
        let offset = used.get(title.position);
        used.add(title.position, thickness);
        text_items.push(title_group(title, thickness, offset, &layout));
    }

    Some(ChartJsTitleLayout {
        text_items,
        ..layout
    })
}

pub(crate) fn chart_view_spec(spec: &ChartSpec, layout: &ChartJsTitleLayout) -> ChartSpec {
    let mut child = spec.clone();
    if matches!(spec.size_mode, SizeMode::Canvas) {
        child.width = layout.viewport_width;
        child.height = layout.viewport_height;
    }
    child.chartjs_title = None;
    child.chartjs_subtitle = None;
    child
}

fn title_thickness(title: &ChartJsTitle) -> f64 {
    let line_height = resolved_line_height(title);
    let line_count = title.text.len() as f64;
    let text_height = if line_count == 0.0 {
        0.0
    } else {
        bounded_product(line_count, line_height, MAX_TITLE_THICKNESS)
    };
    let padding = bounded_add(
        bounded_metric(title.padding.top),
        bounded_metric(title.padding.bottom),
        MAX_TITLE_THICKNESS,
    );
    bounded_add(text_height, padding, MAX_TITLE_THICKNESS)
}

fn title_group(
    title: &ChartJsTitle,
    thickness: f64,
    offset: f64,
    layout: &ChartJsTitleLayout,
) -> Prim {
    let horizontal = matches!(
        title.position,
        ChartJsTitlePosition::Top | ChartJsTitlePosition::Bottom
    );
    let (alignment_origin, alignment_extent) = match (horizontal, title.full_size) {
        (true, true) => (0.0, layout.scene_width),
        (true, false) => (layout.left, layout.viewport_width),
        (false, true) => (0.0, layout.scene_height),
        (false, false) => (layout.top, layout.viewport_height),
    };

    let (translate_x, translate_y, clip_width, clip_height) = match title.position {
        ChartJsTitlePosition::Top => (alignment_origin, offset, alignment_extent, thickness),
        ChartJsTitlePosition::Bottom => (
            alignment_origin,
            (layout.scene_height - offset - thickness).max(0.0),
            alignment_extent,
            thickness,
        ),
        ChartJsTitlePosition::Left => (offset, alignment_origin, thickness, alignment_extent),
        ChartJsTitlePosition::Right => (
            (layout.scene_width - offset - thickness).max(0.0),
            alignment_origin,
            thickness,
            alignment_extent,
        ),
    };

    let font_size = bounded_metric(title.font_size);
    let line_height = resolved_line_height(title);
    let baseline_offset =
        line_height / 2.0 + font_size * crate::layout::common::TEXT_BASELINE_RATIO;
    let padding_top = bounded_metric(title.padding.top);
    let padding_bottom = bounded_metric(title.padding.bottom);
    let rotate_deg = match title.position {
        ChartJsTitlePosition::Left => Some(-90.0),
        ChartJsTitlePosition::Right => Some(90.0),
        ChartJsTitlePosition::Top | ChartJsTitlePosition::Bottom => None,
    };
    let (anchor, aligned_coordinate) = if horizontal {
        horizontal_alignment(title.align, alignment_extent)
    } else {
        vertical_alignment(title.align, title.position, alignment_extent)
    };

    let children = title
        .text
        .iter()
        .enumerate()
        .map(|(index, content)| {
            let line_start = bounded_add(
                padding_top,
                bounded_product(index as f64, line_height, MAX_TITLE_THICKNESS),
                MAX_TITLE_THICKNESS,
            );
            let vertical_line_center = bounded_add(
                padding_top,
                bounded_product(index as f64 + 0.5, line_height, MAX_TITLE_THICKNESS),
                MAX_TITLE_THICKNESS,
            );
            let (x, y) = if horizontal {
                (aligned_coordinate, line_start + baseline_offset)
            } else {
                let x = match title.position {
                    ChartJsTitlePosition::Left => vertical_line_center,
                    ChartJsTitlePosition::Right => {
                        (thickness - padding_bottom - vertical_line_center).max(0.0)
                    }
                    ChartJsTitlePosition::Top | ChartJsTitlePosition::Bottom => unreachable!(),
                };
                (x, aligned_coordinate)
            };
            Prim::StyledText(Box::new(StyledText {
                x,
                y,
                size: font_size,
                anchor,
                fill: title.color,
                content: content.clone(),
                rotate_deg,
                font_family: title.font_family.clone(),
                font_weight: title.font_weight.clone(),
                font_style: title.font_style.clone(),
            }))
        })
        .collect();

    Prim::Group {
        translate_x,
        translate_y,
        clip: Some(Box::new(ClipRect {
            x: 0.0,
            y: 0.0,
            w: clip_width,
            h: clip_height,
        })),
        children,
    }
}

fn horizontal_alignment(align: ChartJsTitleAlign, extent: f64) -> (Anchor, f64) {
    match align {
        ChartJsTitleAlign::Start => (Anchor::Start, 0.0),
        ChartJsTitleAlign::Center => (Anchor::Middle, extent / 2.0),
        ChartJsTitleAlign::End => (Anchor::End, extent),
    }
}

fn vertical_alignment(
    align: ChartJsTitleAlign,
    position: ChartJsTitlePosition,
    extent: f64,
) -> (Anchor, f64) {
    match (position, align) {
        (ChartJsTitlePosition::Right, ChartJsTitleAlign::Start) => (Anchor::Start, 0.0),
        (ChartJsTitlePosition::Right, ChartJsTitleAlign::Center) => (Anchor::Middle, extent / 2.0),
        (ChartJsTitlePosition::Right, ChartJsTitleAlign::End) => (Anchor::End, extent),
        (ChartJsTitlePosition::Left, ChartJsTitleAlign::Start) => (Anchor::Start, extent),
        (ChartJsTitlePosition::Left, ChartJsTitleAlign::Center) => (Anchor::Middle, extent / 2.0),
        (ChartJsTitlePosition::Left, ChartJsTitleAlign::End) => (Anchor::End, 0.0),
        (ChartJsTitlePosition::Top | ChartJsTitlePosition::Bottom, _) => unreachable!(),
    }
}

fn resolved_line_height(title: &ChartJsTitle) -> f64 {
    let font_size = bounded_metric(title.font_size);
    let line_height = if title.line_height.is_finite() && title.line_height > 0.0 {
        title.line_height
    } else {
        font_size * 1.2
    };
    bounded_metric(line_height)
}

fn bounded_metric(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, DEFAULT_MAX_DIMENSION_PX)
    } else {
        DEFAULT_MAX_DIMENSION_PX
    }
}

fn bounded_add(left: f64, right: f64, maximum: f64) -> f64 {
    let total = left + right;
    if total.is_finite() {
        total.min(maximum)
    } else {
        maximum
    }
}

fn bounded_product(left: f64, right: f64, maximum: f64) -> f64 {
    let product = left * right;
    if product.is_finite() {
        product.min(maximum)
    } else {
        maximum
    }
}

fn safe_dimension(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, MAX_OUTPUT_DIMENSION)
    } else {
        MAX_OUTPUT_DIMENSION
    }
}

fn bounded_output_sum(base: f64, first: f64, second: f64) -> f64 {
    bounded_add(
        bounded_add(base, first, MAX_OUTPUT_DIMENSION),
        second,
        MAX_OUTPUT_DIMENSION,
    )
}

#[cfg(test)]
trait TestPrimTranslate {
    fn translate_x(&self) -> Option<f64>;
    fn translate_y(&self) -> Option<f64>;
}

#[cfg(test)]
impl TestPrimTranslate for Prim {
    fn translate_x(&self) -> Option<f64> {
        match self {
            Prim::Group { translate_x, .. } => Some(*translate_x),
            _ => None,
        }
    }

    fn translate_y(&self) -> Option<f64> {
        match self {
            Prim::Group { translate_y, .. } => Some(*translate_y),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::chartjs;
    use crate::ir::{ChartJsTitlePosition, SizeMode};
    use crate::scene::{Anchor, Prim, StyledText};
    use serde_json::{Value, json};

    fn chart_spec(width: f64, height: f64, title: Value, subtitle: Value) -> ChartSpec {
        let input = json!({
            "type":"bar",
            "width":width,
            "height":height,
            "data":{"labels":["A"],"datasets":[{"data":[1]}]},
            "options":{"plugins":{"title":title,"subtitle":subtitle}}
        });
        chartjs::parse(&input.to_string(), false).unwrap()
    }

    fn text_group(layout: &ChartJsTitleLayout, index: usize) -> (&Prim, &StyledText) {
        let group = &layout.text_items[index];
        let Prim::Group { children, .. } = group else {
            panic!("title boxes must be clipped groups");
        };
        let Prim::StyledText(text) = &children[0] else {
            panic!("title lines must use styled text");
        };
        (group, text)
    }

    #[test]
    fn chartjs_title_layout_reserves_each_side_and_stacks_same_side_boxes() {
        let mut spec = chart_spec(
            200.0,
            120.0,
            json!({
                "display":true,"text":"Title","position":"top",
                "font":{"size":14,"lineHeight":1.25},
                "padding":{"top":8,"bottom":4}
            }),
            json!({
                "display":true,"text":"Subtitle","position":"top",
                "font":{"size":10,"lineHeight":1.2},"padding":0
            }),
        );

        let stacked = chartjs_title_layout(&spec, 200.0, 120.0).unwrap();
        assert_eq!(stacked.top, 41.5);
        assert_eq!(stacked.left, 0.0);
        assert_eq!(stacked.right, 0.0);
        assert_eq!(stacked.bottom, 0.0);
        assert_eq!(stacked.viewport_width, 200.0);
        assert_eq!(stacked.viewport_height, 78.5);
        assert_eq!(
            text_group(&stacked, 0).0,
            &Prim::Group {
                translate_x: 0.0,
                translate_y: 0.0,
                clip: Some(Box::new(crate::scene::ClipRect {
                    x: 0.0,
                    y: 0.0,
                    w: 200.0,
                    h: 29.5
                })),
                children: vec![Prim::StyledText(Box::new(
                    text_group(&stacked, 0).1.clone()
                ))],
            }
        );
        assert_eq!(
            text_group(&stacked, 1).0,
            &Prim::Group {
                translate_x: 0.0,
                translate_y: 29.5,
                clip: Some(Box::new(crate::scene::ClipRect {
                    x: 0.0,
                    y: 0.0,
                    w: 200.0,
                    h: 12.0
                })),
                children: vec![Prim::StyledText(Box::new(
                    text_group(&stacked, 1).1.clone()
                ))],
            }
        );

        spec.chartjs_subtitle.as_mut().unwrap().position = ChartJsTitlePosition::Right;
        let sides = chartjs_title_layout(&spec, 200.0, 120.0).unwrap();
        assert_eq!(sides.top, 29.5);
        assert_eq!(sides.right, 12.0);
        assert_eq!(sides.viewport_width, 188.0);
        assert_eq!(text_group(&sides, 0).0.translate_y(), Some(0.0));
        assert_eq!(text_group(&sides, 1).0.translate_x(), Some(188.0));
    }

    #[test]
    fn chartjs_title_layout_uses_full_canvas_or_viewport_alignment_bounds() {
        let mut spec = chart_spec(
            200.0,
            120.0,
            json!({
                "display":true,"text":"Title","position":"left","align":"start",
                "font":{"size":14,"lineHeight":1.25},"padding":{"top":8,"bottom":4},
                "fullSize":false
            }),
            json!({
                "display":true,"text":"Subtitle","position":"bottom",
                "font":{"size":10,"lineHeight":1.2},"padding":0
            }),
        );

        let viewport = chartjs_title_layout(&spec, 200.0, 120.0).unwrap();
        assert_eq!(viewport.bottom, 12.0);
        assert_eq!(viewport.viewport_height, 108.0);
        assert_eq!(text_group(&viewport, 0).1.anchor, Anchor::Start);
        assert_eq!(text_group(&viewport, 0).1.y, 108.0);

        spec.chartjs_title.as_mut().unwrap().full_size = true;
        let full = chartjs_title_layout(&spec, 200.0, 120.0).unwrap();
        assert_eq!(text_group(&full, 0).1.y, 120.0);
    }

    #[test]
    fn chartjs_title_layout_positions_rotated_text_on_all_four_sides() {
        let mut spec = chart_spec(
            200.0,
            120.0,
            json!({"display":true,"text":["First","Second"],"position":"top"}),
            json!({"display":false}),
        );
        for (position, rotation) in [
            (ChartJsTitlePosition::Top, None),
            (ChartJsTitlePosition::Bottom, None),
            (ChartJsTitlePosition::Left, Some(-90.0)),
            (ChartJsTitlePosition::Right, Some(90.0)),
        ] {
            spec.chartjs_title.as_mut().unwrap().position = position;
            let layout = chartjs_title_layout(&spec, 200.0, 120.0).unwrap();
            for line_index in 0..2 {
                let (_, text) = text_group(&layout, 0);
                assert_eq!(text.rotate_deg, rotation);
                assert!(text.content == "First" || text.content == "Second");
                let Prim::Group { children, .. } = &layout.text_items[0] else {
                    unreachable!();
                };
                assert_eq!(children.len(), 2);
                let Prim::StyledText(line) = &children[line_index] else {
                    unreachable!();
                };
                assert_eq!(line.rotate_deg, rotation);
            }
        }
    }

    #[test]
    fn chartjs_title_layout_clamps_viewport_when_boxes_exhaust_canvas() {
        let spec = chart_spec(
            30.0,
            20.0,
            json!({"display":true,"text":"Title","position":"left","font":{"size":14,"lineHeight":1.25},"padding":{"top":8,"bottom":4}}),
            json!({"display":true,"text":"Subtitle","position":"right","font":{"size":10,"lineHeight":1.2},"padding":3}),
        );
        let layout = chartjs_title_layout(&spec, 30.0, 20.0).unwrap();
        assert_eq!(layout.viewport_width, 0.0);
        assert_eq!(layout.viewport_height, 20.0);
        assert!(layout.viewport_width >= 0.0 && layout.viewport_height >= 0.0);
    }

    #[test]
    fn chartjs_title_layout_preserves_plot_area_size_by_expanding_scene() {
        let mut spec = chart_spec(
            200.0,
            120.0,
            json!({"display":true,"text":"Title","position":"top","font":{"size":14,"lineHeight":1.25},"padding":{"top":8,"bottom":4}}),
            json!({"display":true,"text":"Subtitle","position":"right","font":{"size":10,"lineHeight":1.2},"padding":0}),
        );
        spec.size_mode = SizeMode::PlotArea;
        let layout = chartjs_title_layout(&spec, 100.0, 80.0).unwrap();
        assert_eq!(layout.viewport_width, 100.0);
        assert_eq!(layout.viewport_height, 80.0);
        assert_eq!(layout.scene_width, 112.0);
        assert_eq!(layout.scene_height, 109.5);
    }

    #[test]
    fn chartjs_title_layout_distinguishes_empty_string_from_empty_array() {
        let empty_string = chart_spec(
            200.0,
            120.0,
            json!({"display":true,"text":"","font":{"size":14,"lineHeight":1.25},"padding":{"top":8,"bottom":4}}),
            json!({"display":false}),
        );
        let empty_array = chart_spec(
            200.0,
            120.0,
            json!({"display":true,"text":[],"font":{"size":14,"lineHeight":1.25},"padding":{"top":8,"bottom":4}}),
            json!({"display":false}),
        );
        let string_layout = chartjs_title_layout(&empty_string, 200.0, 120.0).unwrap();
        let array_layout = chartjs_title_layout(&empty_array, 200.0, 120.0).unwrap();
        assert_eq!(string_layout.top, 29.5);
        assert_eq!(array_layout.top, 12.0);
        assert_eq!(text_group(&string_layout, 0).1.content, "");
        let Prim::Group { children, .. } = &array_layout.text_items[0] else {
            unreachable!();
        };
        assert!(children.is_empty());
    }

    #[test]
    fn chartjs_title_layout_multiline_thickness_is_line_count_times_line_height_plus_padding() {
        let spec = chart_spec(
            200.0,
            120.0,
            json!({"display":true,"text":["one","two","three"],"font":{"size":14,"lineHeight":1.25},"padding":{"top":8,"bottom":4}}),
            json!({"display":false}),
        );
        let layout = chartjs_title_layout(&spec, 200.0, 120.0).unwrap();
        assert_eq!(layout.top, 64.5);
        let Prim::Group { children, .. } = &layout.text_items[0] else {
            unreachable!();
        };
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn chartjs_title_layout_extreme_metrics_keep_layout_values_finite() {
        let spec = chart_spec(
            500.0,
            300.0,
            json!({"display":true,"text":["one","two","three"],"font":{"size":32768,"lineHeight":"32768px"},"padding":32768}),
            json!({"display":true,"text":"Subtitle","position":"bottom","font":{"size":32768,"lineHeight":"32768px"},"padding":32768}),
        );
        let layout = chartjs_title_layout(&spec, 500.0, 300.0).unwrap();
        for value in [
            layout.left,
            layout.top,
            layout.right,
            layout.bottom,
            layout.viewport_width,
            layout.viewport_height,
            layout.scene_width,
            layout.scene_height,
        ] {
            assert!(value.is_finite(), "layout value must be finite: {value}");
        }
        assert!(layout.top <= crate::guard::DEFAULT_MAX_DIMENSION_PX);
        assert!(layout.bottom <= crate::guard::DEFAULT_MAX_DIMENSION_PX);
    }
}
