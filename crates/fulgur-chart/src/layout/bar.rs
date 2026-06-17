//! bar チャートのレイアウト: ChartSpec → Scene。
//! 縦棒のみ（horizontal は後続タスク）。決定的に組み立て、NaN/Inf/panic を出さない。

use crate::font::DEFAULT_FONT;
use crate::ir::ChartSpec;
use crate::scene::{Prim, Scene};
use crate::text::TextMeasurer;

/// band 内のグループ幅比。
const GROUP_RATIO: f64 = 0.8;
/// band 左右パディング比。
const BAND_PAD_RATIO: f64 = 0.1;
/// bar 幅の塗り比。
const BAR_FILL_RATIO: f64 = 0.9;

pub fn build(spec: &ChartSpec) -> Scene {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = super::common::compute(spec, &m);

    let mut items: Vec<Prim> = Vec::new();
    super::common::draw_frame(&mut items, spec, &frame, &m);

    // bar 本体: カテゴリ band 内に系列グループの矩形を重ねる。
    let n = spec.categories.len().max(1);
    let band_w = super::common::band_width(&frame, n);
    let s = spec.series.len().max(1);
    let group_w = band_w * GROUP_RATIO;
    let bar_w = group_w / s as f64;

    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let baseline_y = frame.ys.map(base_v);

    for i in 0..spec.categories.len() {
        let band_left = super::common::category_center(&frame, i, n) - band_w / 2.0;

        for (sidx, ser) in spec.series.iter().enumerate() {
            let bx = band_left + band_w * BAND_PAD_RATIO + sidx as f64 * bar_w;
            let v = ser.values.get(i).copied().unwrap_or(0.0);
            let vy = frame.ys.map(v);
            let y_top = vy.min(baseline_y);
            let h = (vy - baseline_y).abs();
            items.push(Prim::Rect {
                x: bx,
                y: y_top,
                w: (bar_w * BAR_FILL_RATIO).max(0.0),
                h,
                fill: ser.fill_at(i),
            });
        }
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
