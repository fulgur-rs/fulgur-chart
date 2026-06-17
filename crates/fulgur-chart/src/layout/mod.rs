//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;
pub mod common;
pub mod line;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::Scene;

pub fn build_scene(spec: &ChartSpec) -> Scene {
    match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec),
        ChartKind::Line => line::build(spec),
        ChartKind::Pie { .. } => todo!("Task 14 で実装"),
    }
}
