//! Treemap チャートのレイアウト (squarified)。Task 3 で本実装に置き換える。

use crate::ir::ChartSpec;
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build(_spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    // 暫定スタブ。Task 3 で squarify + 描画を実装する。
    Scene {
        width: _spec.width,
        height: _spec.height,
        items: vec![],
    }
}
