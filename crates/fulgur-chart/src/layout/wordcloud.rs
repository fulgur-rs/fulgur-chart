//! WordCloud チャートのレイアウト。
//! アルキメデス螺旋 + AABB 衝突検出で単語を非重複配置する。

use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// 螺旋パラメータ: r = SPIRAL_A * θ (px/rad)
const SPIRAL_A: f64 = 3.0;
/// θ のステップ量 (rad)
const DELTA_THETA: f64 = 0.08;
/// フォントサイズに対する行高さ比率
const LINE_HEIGHT: f64 = 1.2;

/// 軸揃え境界ボックス (中心座標 + 半幅/半高)
#[derive(Clone, Copy)]
struct Aabb {
    cx: f64,
    cy: f64,
    half_w: f64,
    half_h: f64,
}

impl Aabb {
    fn intersects(&self, other: &Aabb) -> bool {
        (self.cx - other.cx).abs() < self.half_w + other.half_w
            && (self.cy - other.cy).abs() < self.half_h + other.half_h
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ChartKind::WordCloud {
        entries,
        min_rotation,
        max_rotation,
        rotation_steps,
        padding,
    } = &spec.kind
    else {
        unreachable!()
    };

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let plot_top = OUTER_PAD + title_band;
    let plot_h = (spec.height - plot_top - OUTER_PAD).max(0.0);
    let center_x = spec.width / 2.0;
    let center_y = plot_top + plot_h / 2.0;
    let max_r = (spec.width / 2.0).hypot(plot_h / 2.0) * 1.1;

    // size 降順・同値は text 昇順でソート (決定的)
    let mut sorted: Vec<(usize, &crate::ir::WordEntry)> = entries.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| {
        b.size
            .partial_cmp(&a.size)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.text.cmp(&b.text))
    });

    let mut placed: Vec<Aabb> = Vec::with_capacity(sorted.len());
    let mut items: Vec<Prim> = Vec::new();
    let palette = &spec.theme.palette;

    for (orig_idx, entry) in sorted {
        let text_w = m.width(&entry.text, entry.size as f32) as f64;
        let text_h = entry.size * LINE_HEIGHT;

        // 回転角度の決定
        let rotate_deg = if *rotation_steps <= 1 {
            *max_rotation
        } else {
            let step_idx = orig_idx % (*rotation_steps as usize);
            let t = step_idx as f64 / (*rotation_steps as f64 - 1.0);
            min_rotation + t * (max_rotation - min_rotation)
        };
        // 0° か −90° の 2 択のみ AABB が axis-aligned になる
        let is_vertical = (rotate_deg + 90.0).abs() < 1e-9;

        // AABB (padding 込み)
        let (hw, hh) = if is_vertical {
            (text_h / 2.0 + padding, text_w / 2.0 + padding)
        } else {
            (text_w / 2.0 + padding, text_h / 2.0 + padding)
        };

        // 螺旋探索
        let mut theta: f64 = 0.0;
        let mut placed_pos: Option<(f64, f64)> = None;

        loop {
            let r = SPIRAL_A * theta;
            let cx = center_x + r * theta.cos();
            let cy = center_y + r * theta.sin();

            let candidate = Aabb {
                cx,
                cy,
                half_w: hw,
                half_h: hh,
            };
            let in_bounds = cx - hw >= 0.0
                && cx + hw <= spec.width
                && cy - hh >= plot_top
                && cy + hh <= spec.height - OUTER_PAD;

            if in_bounds && !placed.iter().any(|p| p.intersects(&candidate)) {
                placed.push(candidate);
                placed_pos = Some((cx, cy));
                break;
            }

            theta += DELTA_THETA;
            if r > max_r {
                break; // 収まらない単語はスキップ
            }
        }

        let Some((cx, cy)) = placed_pos else { continue };

        // 色の決定
        let color = entry
            .color
            .unwrap_or_else(|| palette[orig_idx % palette.len()]);

        // SVG テキスト
        let rotate = if rotate_deg.abs() < 1e-9 {
            None
        } else {
            Some(rotate_deg)
        };
        let (tx, ty) = if is_vertical {
            (cx, cy) // 縦文字は回転中心を基準
        } else {
            (cx, cy + entry.size * TEXT_BASELINE_RATIO)
        };

        items.push(Prim::Text {
            x: tx,
            y: ty,
            size: entry.size,
            anchor: Anchor::Middle,
            fill: color,
            content: entry.text.clone(),
            rotate_deg: rotate,
        });
    }

    // タイトル（先頭に挿入して単語テキストより手前へ）
    if let Some(title) = &spec.title {
        items.insert(
            0,
            Prim::Text {
                x: spec.width / 2.0,
                y: OUTER_PAD + TITLE_FONT,
                size: TITLE_FONT,
                anchor: Anchor::Middle,
                fill: spec.theme.text_color,
                content: title.clone(),
                rotate_deg: None,
            },
        );
    }

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
    use crate::ir::{ChartKind, ChartSpec, LegendPos, WordEntry};

    fn make_spec(entries: Vec<WordEntry>) -> ChartSpec {
        ChartSpec {
            kind: ChartKind::WordCloud {
                entries,
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            categories: vec![],
            series: vec![],
            x_axis: crate::ir::AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: true,
                offset: false,
                grid: true,
            },
            y_axis: crate::ir::AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: true,
                offset: false,
                grid: true,
            },
            legend: LegendPos::None,
            title: None,
            width: 600.0,
            height: 400.0,
            data_labels: false,
            theme: crate::ir::Theme::default(),
        }
    }

    #[test]
    fn wordcloud_build_produces_scene() {
        let entries = vec![
            WordEntry {
                text: "Hello".to_string(),
                size: 40.0,
                color: None,
            },
            WordEntry {
                text: "World".to_string(),
                size: 30.0,
                color: None,
            },
            WordEntry {
                text: "Rust".to_string(),
                size: 20.0,
                color: None,
            },
        ];
        let spec = make_spec(entries);
        let m = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(scene.width, 600.0);
        assert_eq!(scene.height, 400.0);
        // 3 単語分のテキストが配置されているはず
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn wordcloud_empty_entries_no_panic() {
        let spec = make_spec(vec![]);
        let m = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert!(scene.items.is_empty());
    }

    #[test]
    fn wordcloud_with_title_inserts_title_first() {
        let entries = vec![WordEntry {
            text: "Test".to_string(),
            size: 30.0,
            color: None,
        }];
        let mut spec = make_spec(entries);
        spec.title = Some("My Cloud".to_string());
        let m = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        // 先頭がタイトルテキスト
        match &scene.items[0] {
            Prim::Text { content, size, .. } => {
                assert_eq!(content, "My Cloud");
                assert_eq!(*size, TITLE_FONT);
            }
            _ => panic!("先頭要素はタイトルテキストであるべき"),
        }
    }

    #[test]
    fn wordcloud_explicit_color_is_used() {
        use crate::ir::Color;
        let red = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0,
        };
        let entries = vec![WordEntry {
            text: "Red".to_string(),
            size: 30.0,
            color: Some(red),
        }];
        let spec = make_spec(entries);
        let m = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        // 最初に配置された単語テキストが赤であること
        let word_prim = scene
            .items
            .iter()
            .find(|p| matches!(p, Prim::Text { content, .. } if content == "Red"));
        match word_prim {
            Some(Prim::Text { fill, .. }) => assert_eq!(*fill, red),
            _ => panic!("Red の単語が見つからない"),
        }
    }
}
