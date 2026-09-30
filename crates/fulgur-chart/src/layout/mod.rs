//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;
pub mod boxplot;
pub(crate) mod chartjs_title;
pub mod common;
mod decimate;
pub(crate) mod error_mark;
pub mod gauge;
pub mod geoshape;
pub mod line;
pub mod matrix;
pub mod mixed;
pub mod monotone;
pub mod outlabeled_pie;
pub mod pie;
pub mod polar_area;
pub mod progress;
pub mod radar;
pub mod sankey;
pub mod scatter;
pub mod sparkline;
pub mod treemap;
pub(crate) mod vega_boxplot;
pub(crate) mod vega_composition;
pub mod vega_rect;
pub(crate) mod vega_rule;
pub(crate) mod vega_text;
pub(crate) mod vega_tick;
pub mod violin;
pub mod wordcloud;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

pub fn build_scene(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    build_scene_checked(spec, m).expect("chart layout failed")
}

/// Fallible layout path for renderers that can report projection/layout errors to callers.
pub fn build_scene_checked(spec: &ChartSpec, m: &TextMeasurer) -> Result<Scene, String> {
    build_scene_checked_with_limits(spec, m, &crate::guard::InputLimits::default())
}

/// Fallible layout path that applies caller-provided GeoShape projection limits.
pub fn build_scene_checked_with_limits(
    spec: &ChartSpec,
    m: &TextMeasurer,
    limits: &crate::guard::InputLimits,
) -> Result<Scene, String> {
    crate::guard::validate_vega_text(spec, limits)?;
    crate::guard::validate_vega_image(spec, limits)?;
    let mut scene = if !chartjs_title::has_visible_chartjs_titles(spec) {
        build_chart_scene(spec, m, limits)?
    } else {
        let (base_scene, layout) = if matches!(spec.size_mode, crate::ir::SizeMode::Canvas) {
            let layout = chartjs_title::chartjs_title_layout(spec, spec.width, spec.height)
                .ok_or_else(|| "visible Chart.js title produces no layout".to_string())?;
            let child_spec = chartjs_title::chart_view_spec(spec, &layout);
            (build_chart_scene(&child_spec, m, limits)?, layout)
        } else {
            let mut child_spec = spec.clone();
            child_spec.chartjs_title = None;
            child_spec.chartjs_subtitle = None;
            let base_scene = build_chart_scene(&child_spec, m, limits)?;
            let layout =
                chartjs_title::chartjs_title_layout(spec, base_scene.width, base_scene.height)
                    .ok_or_else(|| "visible Chart.js title produces no layout".to_string())?;
            (base_scene, layout)
        };

        let mut items = Vec::with_capacity(1 + layout.text_items.len());
        items.push(Prim::Group {
            translate_x: layout.left,
            translate_y: layout.top,
            clip: Some(Box::new(crate::scene::ClipRect {
                x: 0.0,
                y: 0.0,
                w: layout.viewport_width,
                h: layout.viewport_height,
            })),
            children: base_scene.items,
        });
        items.extend(layout.text_items);
        Scene {
            width: layout.scene_width,
            height: layout.scene_height,
            items,
        }
    };

    // テーマ背景色: 指定時のみ最背面(index 0)へ全面矩形を挿入する。
    if let Some(fill) = spec.theme.background {
        scene.items.insert(
            0,
            Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: scene.width,
                h: scene.height,
                fill,
            },
        );
    }

    Ok(scene)
}

/// Builds a Vega-Lite unit Scene and reports the trailing top-level mark item count for composite
/// marks whose axes and data geometry use the same primitive types.
pub(crate) fn build_scene_checked_with_layer_marks(
    spec: &ChartSpec,
    m: &TextMeasurer,
    limits: &crate::guard::InputLimits,
    shared_color_categories: Option<&[String]>,
) -> Result<(Scene, Option<usize>), String> {
    crate::guard::validate_vega_image(spec, limits)?;
    let parts = match &spec.kind {
        ChartKind::ErrorMark(_) => Some(error_mark::build_checked_with_layer_parts(
            spec,
            m,
            limits.max_categorical_primitives,
        )?),
        ChartKind::VegaBoxPlot(_) => Some(vega_boxplot::build_checked_with_layer_parts(
            spec,
            m,
            limits,
            shared_color_categories,
        )?),
        ChartKind::VegaRule(_) => Some(vega_rule::build_checked_with_layer_parts(spec, m, limits)?),
        ChartKind::VegaTick(_) => Some(vega_tick::build_checked_with_layer_parts(spec, m, limits)?),
        _ => None,
    };
    let Some((mut scene, mark_count)) = parts else {
        return Ok((build_scene_checked_with_limits(spec, m, limits)?, None));
    };
    if let Some(fill) = spec.theme.background {
        scene.items.insert(
            0,
            Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: scene.width,
                h: scene.height,
                fill,
            },
        );
    }
    Ok((scene, Some(mark_count)))
}

fn build_chart_scene(
    spec: &ChartSpec,
    m: &TextMeasurer,
    limits: &crate::guard::InputLimits,
) -> Result<Scene, String> {
    let scene = match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec, m),
        ChartKind::Line { .. } | ChartKind::Trail => line::build(spec, m),
        ChartKind::Pie { .. } => pie::build(spec, m),
        ChartKind::PolarArea => polar_area::build(spec, m),
        // scatter/bubble/square は同じレイアウト。マーカー形状・サイズは scatter.rs で分岐。
        ChartKind::Scatter | ChartKind::Bubble | ChartKind::Square | ChartKind::VegaImage(_) => {
            scatter::build(spec, m)
        }
        ChartKind::VegaText(_) => vega_text::build(spec, m),
        ChartKind::VegaTick(_) => vega_tick::build_checked(spec, m, limits)?,
        ChartKind::VegaRule(_) => vega_rule::build_checked(spec, m, limits)?,
        ChartKind::Radar => radar::build(spec, m),
        ChartKind::Mixed => mixed::build(spec, m),
        ChartKind::Matrix { .. } => matrix::build(spec, m),
        ChartKind::VegaRect { .. } => vega_rect::build(spec, m),
        ChartKind::GeoShape { .. } => {
            geoshape::build_with_primitive_limit(spec, m, limits.max_geo_primitives)?
        }
        ChartKind::ErrorMark(_) => {
            error_mark::build_checked(spec, m, limits.max_categorical_primitives)?
        }
        ChartKind::VegaBoxPlot(_) => vega_boxplot::build_checked(spec, m, limits)?,
        ChartKind::VegaComposition(_) => vega_composition::build_checked(spec, m, limits)?,
        ChartKind::Progress => progress::build(spec, m),
        ChartKind::BoxPlot => boxplot::build(spec, m),
        ChartKind::Violin { .. } => violin::build(spec, m),
        ChartKind::Sparkline => sparkline::build(spec, m),
        ChartKind::RadialGauge { .. } | ChartKind::Gauge { .. } => gauge::build(spec, m),
        ChartKind::OutlabeledPie { .. } => outlabeled_pie::build(spec, m),
        ChartKind::Treemap => treemap::build(spec, m),
        ChartKind::WordCloud { .. } => wordcloud::build(spec, m),
        ChartKind::Sankey { .. } => sankey::build(spec, m),
    };
    Ok(scene)
}
