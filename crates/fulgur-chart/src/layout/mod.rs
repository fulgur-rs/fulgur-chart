//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;
pub mod common;
pub mod line;
pub mod pie;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build_scene(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec, m),
        ChartKind::Line => line::build(spec, m),
        ChartKind::Pie { .. } => pie::build(spec, m),
    }
}
