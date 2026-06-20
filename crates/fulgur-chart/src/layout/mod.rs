//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;
pub mod common;
pub mod line;
pub mod matrix;
pub mod mixed;
pub mod pie;
pub mod progress;
pub mod radar;
pub mod scatter;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

pub fn build_scene(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut scene = match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec, m),
        ChartKind::Line => line::build(spec, m),
        ChartKind::Pie { .. } => pie::build(spec, m),
        // bubble は scatter と同じレイアウト。半径だけ point.r を使う(scatter.rs 内で分岐)。
        ChartKind::Scatter | ChartKind::Bubble => scatter::build(spec, m),
        ChartKind::Radar => radar::build(spec, m),
        ChartKind::Mixed => mixed::build(spec, m),
        ChartKind::Matrix { .. } => matrix::build(spec, m),
        ChartKind::Progress => progress::build(spec, m),
        // BoxPlot のレイアウトは Task 2 で実装予定。現時点では空シーンを返す。
        ChartKind::BoxPlot => crate::scene::Scene {
            width: spec.width,
            height: spec.height,
            items: vec![],
        },
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
