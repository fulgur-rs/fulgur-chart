//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;
pub mod boxplot;
pub mod common;
pub mod gauge;
pub mod line;
pub mod matrix;
pub mod mixed;
pub mod pie;
pub mod polar_area;
pub mod progress;
pub mod radar;
pub mod scatter;
pub mod sparkline;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

pub fn build_scene(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut scene = match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec, m),
        ChartKind::Line => line::build(spec, m),
        ChartKind::Pie { .. } => pie::build(spec, m),
        ChartKind::PolarArea => polar_area::build(spec, m),
        // bubble は scatter と同じレイアウト。半径だけ point.r を使う(scatter.rs 内で分岐)。
        ChartKind::Scatter | ChartKind::Bubble => scatter::build(spec, m),
        ChartKind::Radar => radar::build(spec, m),
        ChartKind::Mixed => mixed::build(spec, m),
        ChartKind::Matrix { .. } => matrix::build(spec, m),
        ChartKind::Progress => progress::build(spec, m),
        ChartKind::BoxPlot => boxplot::build(spec, m),
        ChartKind::Sparkline => sparkline::build(spec, m),
        ChartKind::RadialGauge { .. } | ChartKind::Gauge { .. } => gauge::build(spec, m),
        ChartKind::OutlabeledPie { .. } => panic!("OutlabeledPie renderer not yet implemented"),
    };

    // テーマ背景色: 指定時のみ最背面(index 0)へ全面矩形を挿入する。
    if let Some(fill) = spec.theme.background {
        scene.items.insert(
            0,
            Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: spec.width,
                h: spec.height,
                fill,
            },
        );
    }

    scene
}
