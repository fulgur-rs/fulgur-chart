//! Vega-Lite `mark: "rect"` (ヒートマップ) のレイアウト。
//! Task 5 でセル・軸ラベル・タイトルを描画する。現状は最小 Scene を返す stub。

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
