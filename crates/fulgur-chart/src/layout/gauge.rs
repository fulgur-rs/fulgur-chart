//! gauge / radialGauge チャートのレイアウト: ChartSpec → Scene。
//! 軸なし。決定的に組み立て、NaN/Inf/panic を出さない。
//! すべての弧は standalone な空白区切り M/L/A/Z トークンで生成する
//! (raster_direct::parse_path_data 不変条件。pie.rs / progress.rs と同様)。

use super::common::{OUTER_PAD, TITLE_FONT};
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

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
