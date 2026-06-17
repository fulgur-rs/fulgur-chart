//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;
pub mod common;
pub mod line;
pub mod pie;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

pub fn build_scene(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut scene = match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec, m),
        ChartKind::Line => line::build(spec, m),
        ChartKind::Pie { .. } => pie::build(spec, m),
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
