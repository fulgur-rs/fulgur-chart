//! IR(ChartSpec) → Scene のレイアウト。チャート種別ごとに分岐。

pub mod bar;

use crate::ir::{ChartKind, ChartSpec};
use crate::scene::Scene;

pub fn build_scene(spec: &ChartSpec) -> Scene {
    match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec),
        ChartKind::Line => todo!("Task 13 で実装"),
        ChartKind::Pie { .. } => todo!("Task 14 で実装"),
    }
}
