//! sankey レイアウト(Phase 3 で本実装)。
use crate::ir::ChartSpec;
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    Scene {
        width: spec.width,
        height: spec.height,
        items: vec![],
    }
}
