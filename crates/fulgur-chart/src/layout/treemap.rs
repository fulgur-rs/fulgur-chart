//! Treemap チャートのレイアウト。階層データを squarified アルゴリズム
//! (Bruls/Huizing/van Wijk) でネストした矩形に分割し、深さに応じた色で塗る。

use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{ChartSpec, Color, TreeNode};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// 隣接矩形間の隙間 (px)。各セルをこの分だけ内側へ縮める。
const SPACING: f64 = 2.0;
/// depth ごとに白へ寄せる比率 (上限あり)。
const DEPTH_LIGHTEN: f64 = 0.18;
const DEPTH_LIGHTEN_MAX: f64 = 0.6;
/// キャプション帯やラベルのパディング (px)。
const PAD: f64 = 3.0;

const WHITE: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 1.0,
};

/// レイアウト用の矩形 (左上原点)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TreemapRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn lerp_color(lo: Color, hi: Color, t: f64) -> Color {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    Color {
        r: (lo.r as f64 + (hi.r as f64 - lo.r as f64) * t).round() as u8,
        g: (lo.g as f64 + (hi.g as f64 - lo.g as f64) * t).round() as u8,
        b: (lo.b as f64 + (hi.b as f64 - lo.b as f64) * t).round() as u8,
        a: lo.a + (hi.a - lo.a) * t as f32,
    }
}

/// depth に応じて base 色を白へ寄せる。depth 0 は base そのもの。
fn lighten(base: Color, depth: usize) -> Color {
    let t = (depth as f64 * DEPTH_LIGHTEN).min(DEPTH_LIGHTEN_MAX);
    lerp_color(base, WHITE, t)
}

/// 背景色の輝度からコントラストの取れる文字色 (濃灰 or 白) を選ぶ。
fn text_on(bg: Color) -> Color {
    let lum = 0.299 * bg.r as f64 + 0.587 * bg.g as f64 + 0.114 * bg.b as f64;
    if lum > 140.0 {
        Color {
            r: 60,
            g: 60,
            b: 60,
            a: 1.0,
        }
    } else {
        WHITE
    }
}

/// `s` を `max_w` 以内に収める。収まらなければ末尾を削り "…" を付す。
/// "…" 単体でも収まらなければ None (描画しない)。
fn truncate_to_width(s: &str, max_w: f64, font: f64, m: &TextMeasurer) -> Option<String> {
    if max_w <= 0.0 || s.is_empty() {
        return None;
    }
    if m.width(s, font as f32) as f64 <= max_w {
        return Some(s.to_string());
    }
    let ell = "…";
    let chars: Vec<char> = s.chars().collect();
    let mut end = chars.len();
    while end > 0 {
        end -= 1;
        let mut cand: String = chars[..end].iter().collect();
        cand.push_str(ell);
        if m.width(&cand, font as f32) as f64 <= max_w {
            return Some(cand);
        }
    }
    None
}

/// `worst`: 与えた area 行を length 辺に沿って並べたときの最悪アスペクト比。
/// Bruls et al. の定義。
fn worst(row: &[f64], length: f64) -> f64 {
    if row.is_empty() || length <= 0.0 {
        return f64::INFINITY;
    }
    let s: f64 = row.iter().sum();
    if s <= 0.0 {
        return f64::INFINITY;
    }
    let rmax = row.iter().cloned().fold(f64::MIN, f64::max);
    let rmin = row.iter().cloned().fold(f64::MAX, f64::min);
    let l2 = length * length;
    let s2 = s * s;
    (l2 * rmax / s2).max(s2 / (l2 * rmin.max(f64::EPSILON)))
}

/// squarified treemap: `areas` (各ノードの値) を `rect` 内へ充填し、入力順に対応する
/// 矩形列を返す。面積は値に比例し、矩形は rect を重なりなくタイルする。
pub(crate) fn squarify(areas: &[f64], rect: TreemapRect) -> Vec<TreemapRect> {
    let n = areas.len();
    let zero = TreemapRect {
        x: rect.x,
        y: rect.y,
        w: 0.0,
        h: 0.0,
    };
    let total: f64 = areas.iter().map(|a| a.max(0.0)).sum();
    if n == 0 || total <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return vec![zero; n];
    }
    let scale = (rect.w * rect.h) / total;
    let scaled: Vec<f64> = areas.iter().map(|a| a.max(0.0) * scale).collect();

    let mut result = vec![zero; n];
    let mut free = rect;
    let mut i = 0;
    while i < n {
        let shorter = free.w.min(free.h);
        // worst を悪化させない範囲で行を伸ばす。
        let mut row_end = i + 1;
        let mut best = worst(&scaled[i..row_end], shorter);
        while row_end < n {
            let cand = worst(&scaled[i..row_end + 1], shorter);
            if cand <= best {
                best = cand;
                row_end += 1;
            } else {
                break;
            }
        }
        let row = &scaled[i..row_end];
        let row_sum: f64 = row.iter().sum();
        if free.w >= free.h {
            // 左側に縦ストリップを敷く。幅 = row_sum / free.h。
            let strip_w = if free.h > 0.0 { row_sum / free.h } else { 0.0 };
            let mut y = free.y;
            for (j, &a) in row.iter().enumerate() {
                let cell_h = if strip_w > 0.0 { a / strip_w } else { 0.0 };
                result[i + j] = TreemapRect {
                    x: free.x,
                    y,
                    w: strip_w,
                    h: cell_h,
                };
                y += cell_h;
            }
            free.x += strip_w;
            free.w -= strip_w;
        } else {
            // 上側に横ストリップを敷く。高さ = row_sum / free.w。
            let strip_h = if free.w > 0.0 { row_sum / free.w } else { 0.0 };
            let mut x = free.x;
            for (j, &a) in row.iter().enumerate() {
                let cell_w = if strip_h > 0.0 { a / strip_h } else { 0.0 };
                result[i + j] = TreemapRect {
                    x,
                    y: free.y,
                    w: cell_w,
                    h: strip_h,
                };
                x += cell_w;
            }
            free.y += strip_h;
            free.h -= strip_h;
        }
        i = row_end;
    }
    result
}

fn inset(r: TreemapRect, by: f64) -> TreemapRect {
    TreemapRect {
        x: r.x + by / 2.0,
        y: r.y + by / 2.0,
        w: (r.w - by).max(0.0),
        h: (r.h - by).max(0.0),
    }
}

/// リーフ矩形の中央にラベル(+値)を描く。収まらなければ truncate、極小は省略。
fn draw_leaf_label(
    node: &TreeNode,
    r: TreemapRect,
    fill: Color,
    font: f64,
    m: &TextMeasurer,
    items: &mut Vec<Prim>,
) {
    let avail_w = r.w - 2.0 * PAD;
    let avail_h = r.h - 2.0 * PAD;
    if avail_w <= 0.0 || avail_h < font {
        return;
    }
    let color = text_on(fill);
    let cx = r.x + r.w / 2.0;
    let cy = r.y + r.h / 2.0;
    let value_str = fmt_num(node.value);
    let two_lines = !node.label.is_empty() && avail_h >= 2.0 * font + 2.0;
    if two_lines {
        if let Some(lbl) = truncate_to_width(&node.label, avail_w, font, m) {
            items.push(Prim::Text {
                x: cx,
                y: cy - font * 0.1,
                size: font,
                anchor: Anchor::Middle,
                fill: color,
                content: lbl,
            });
        }
        if let Some(v) = truncate_to_width(&value_str, avail_w, font, m) {
            items.push(Prim::Text {
                x: cx,
                y: cy + font * 0.95,
                size: font,
                anchor: Anchor::Middle,
                fill: color,
                content: v,
            });
        }
    } else {
        let single = if node.label.is_empty() {
            value_str
        } else {
            node.label.clone()
        };
        if let Some(t) = truncate_to_width(&single, avail_w, font, m) {
            items.push(Prim::Text {
                x: cx,
                y: cy + font * TEXT_BASELINE_RATIO,
                size: font,
                anchor: Anchor::Middle,
                fill: color,
                content: t,
            });
        }
    }
}

/// グループ矩形の上部にキャプション(グループ名)を描く。
fn draw_caption(
    label: &str,
    r: TreemapRect,
    fill: Color,
    font: f64,
    m: &TextMeasurer,
    items: &mut Vec<Prim>,
) {
    // 縦方向に収まらない極小グループ矩形ではキャプションを描かない (リーフと対称)。
    if r.h < font + PAD {
        return;
    }
    let avail_w = r.w - 2.0 * PAD;
    if let Some(t) = truncate_to_width(label, avail_w, font, m) {
        items.push(Prim::Text {
            x: r.x + PAD,
            y: r.y + font + 1.0,
            size: font,
            anchor: Anchor::Start,
            fill: text_on(fill),
            content: t,
        });
    }
}

/// ノード列を rect 内に squarify して再帰描画する。
/// base=None ならトップレベル (各ノードに palette[i])、Some なら親色を継承。
#[allow(clippy::too_many_arguments)]
fn draw_nodes(
    nodes: &[TreeNode],
    rect: TreemapRect,
    depth: usize,
    base: Option<Color>,
    palette: &[Color],
    font: f64,
    m: &TextMeasurer,
    items: &mut Vec<Prim>,
) {
    if nodes.is_empty() || palette.is_empty() {
        return;
    }
    // value 降順、同値は元 index で安定 tie-break (determinism)。
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        nodes[b]
            .value
            .partial_cmp(&nodes[a].value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let areas: Vec<f64> = order.iter().map(|&i| nodes[i].value.max(0.0)).collect();
    let rects = squarify(&areas, rect);

    for (k, &i) in order.iter().enumerate() {
        let node = &nodes[i];
        let node_base = base.unwrap_or_else(|| palette[i % palette.len()]);
        let fill = lighten(node_base, depth);
        let cell = inset(rects[k], SPACING);
        if cell.w <= 0.0 || cell.h <= 0.0 {
            continue;
        }
        items.push(Prim::Rect {
            x: cell.x,
            y: cell.y,
            w: cell.w,
            h: cell.h,
            fill,
        });
        if node.children.is_empty() {
            draw_leaf_label(node, cell, fill, font, m, items);
        } else {
            draw_caption(&node.label, cell, fill, font, m, items);
            let cap_h = font + 6.0;
            let child_rect = TreemapRect {
                x: cell.x,
                y: cell.y + cap_h,
                w: cell.w,
                h: (cell.h - cap_h).max(0.0),
            };
            if child_rect.h > 0.0 {
                draw_nodes(
                    &node.children,
                    child_rect,
                    depth + 1,
                    Some(node_base),
                    palette,
                    font,
                    m,
                    items,
                );
            }
        }
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let font = spec.theme.font_size;
    let ink = spec.theme.text_color;
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

    let plot = TreemapRect {
        x: OUTER_PAD,
        y: OUTER_PAD + title_band,
        w: (spec.width - 2.0 * OUTER_PAD).max(0.0),
        h: (spec.height - 2.0 * OUTER_PAD - title_band).max(0.0),
    };

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

    let forest: &[TreeNode] = spec
        .series
        .first()
        .map(|s| s.tree.as_slice())
        .unwrap_or(&[]);
    draw_nodes(
        forest,
        plot,
        0,
        None,
        &spec.theme.palette,
        font,
        m,
        &mut items,
    );

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;

    fn rects_overlap(a: &TreemapRect, b: &TreemapRect) -> bool {
        let eps = 1e-6;
        a.x + eps < b.x + b.w
            && b.x + eps < a.x + a.w
            && a.y + eps < b.y + b.h
            && b.y + eps < a.y + a.h
    }

    #[test]
    fn squarify_areas_proportional_to_values() {
        let rect = TreemapRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let values = [6.0, 4.0, 3.0, 2.0, 1.0];
        let total: f64 = values.iter().sum();
        let rects = squarify(&values, rect);
        let container = rect.w * rect.h;
        for (k, &v) in values.iter().enumerate() {
            let area = rects[k].w * rects[k].h;
            let expected = v / total * container;
            assert!(
                (area - expected).abs() < 1e-3,
                "leaf {k}: area {area} != expected {expected}"
            );
        }
    }

    #[test]
    fn squarify_tiles_without_overlap_and_fills_container() {
        let rect = TreemapRect {
            x: 5.0,
            y: 7.0,
            w: 200.0,
            h: 120.0,
        };
        let values = [10.0, 7.0, 5.0, 3.0, 2.0, 1.0, 1.0];
        let rects = squarify(&values, rect);
        let sum: f64 = rects.iter().map(|r| r.w * r.h).sum();
        assert!(
            (sum - rect.w * rect.h).abs() < 1e-3,
            "areas must fill container"
        );
        for a in 0..rects.len() {
            for b in (a + 1)..rects.len() {
                assert!(
                    !rects_overlap(&rects[a], &rects[b]),
                    "rects {a} and {b} overlap"
                );
            }
        }
        for r in &rects {
            assert!(r.x >= rect.x - 1e-6 && r.y >= rect.y - 1e-6);
            assert!(r.x + r.w <= rect.x + rect.w + 1e-6);
            assert!(r.y + r.h <= rect.y + rect.h + 1e-6);
        }
    }

    fn treemap_spec(json: &str) -> ChartSpec {
        chartjs::parse(json, false).expect("parse error")
    }

    #[test]
    fn nested_treemap_has_rects_and_text() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{
                "key": "v", "groups": ["a", "b"],
                "tree": [
                    {"a":"X","b":"p","v":8},
                    {"a":"X","b":"q","v":4},
                    {"a":"Y","b":"r","v":6}
                ]
            }] }
        }"#;
        let spec = treemap_spec(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rects = scene
            .items
            .iter()
            .filter(|p| matches!(p, Prim::Rect { .. }))
            .count();
        let texts = scene
            .items
            .iter()
            .filter(|p| matches!(p, Prim::Text { .. }))
            .count();
        assert!(rects >= 5, "expected nested rects, got {rects}");
        assert!(texts > 0, "expected labels/captions");
        assert!(!format!("{:?}", scene.items).contains("NaN"));
    }

    #[test]
    fn build_is_deterministic() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{ "tree": [5, 5, 3, 3, 2] }] }
        }"#;
        let spec = treemap_spec(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let a = build(&spec, &m);
        let b = build(&spec, &m);
        assert_eq!(a, b, "same spec must produce identical scene");
    }

    #[test]
    fn scene_dims_match_spec() {
        let json = r#"{"type":"treemap","data":{"datasets":[{"tree":[1,2,3]}]}}"#;
        let spec = treemap_spec(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(scene.width, spec.width);
        assert_eq!(scene.height, spec.height);
    }
}
