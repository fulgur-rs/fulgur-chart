//! progress チャートのレイアウト: ChartSpec → Scene。
//! 軸なしの水平塗りつぶしバー。決定的に組み立て、NaN/Inf/panic を出さない。

use super::common::{OUTER_PAD, TITLE_BAND, TITLE_FONT};
use crate::ir::ChartSpec;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let mut items: Vec<Prim> = Vec::new();

    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }
    let _ = TITLE_BAND; // 後続タスクで使用

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
