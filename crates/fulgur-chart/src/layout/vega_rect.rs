//! Vega-Lite `mark: "rect"` (ヒートマップ) のレイアウト。
//! 純粋な grid renderer: cells[row][col] が Some のときのみ Prim::Rect を出す。
//! matrix.rs の構造を踏襲するが、scale 解決は frontend/vegalite.rs 側で完結している。

use super::common::{
    OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT, X_LABEL_BAND, X_LABEL_CENTER_RATIO,
};
use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let (x_labels, y_labels, cells) = match &spec.kind {
        ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } => (x_labels, y_labels, cells),
        _ => unreachable!("vega_rect::build called on non-VegaRect kind"),
    };

    // Frontend invariant: cells は y_labels.len() 行 × x_labels.len() 列。
    // 不整合は描画が黙って歪むだけなので debug 時に早期パニック。
    debug_assert_eq!(
        cells.len(),
        y_labels.len(),
        "vega_rect cells row count must equal y_labels len"
    );
    debug_assert!(
        cells.iter().all(|row| row.len() == x_labels.len()),
        "vega_rect cells column count must equal x_labels len for every row"
    );

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let n_rows = y_labels.len();
    let n_cols = x_labels.len();

    // y-axis label width
    let mut max_y_w = 0.0_f32;
    for l in y_labels {
        let w = m.width(l, label_font as f32);
        if w > max_y_w {
            max_y_w = w;
        }
    }
    let y_axis_w = max_y_w as f64 + 10.0;

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };

    let plot_left = OUTER_PAD + y_axis_w;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND;

    // Guard against negative dimensions when y-label width exceeds available space.
    let plot_w = (plot_right - plot_left).max(0.0);
    let plot_h = (plot_bottom - plot_top).max(0.0);

    let cell_w = if n_cols > 0 {
        plot_w / n_cols as f64
    } else {
        plot_w
    };
    let cell_h = if n_rows > 0 {
        plot_h / n_rows as f64
    } else {
        plot_h
    };

    let mut items: Vec<Prim> = Vec::new();

    // Title
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // Cells (skip None)
    for (row, row_cells) in cells.iter().enumerate() {
        let cell_y = plot_top + row as f64 * cell_h;
        for (col, cell) in row_cells.iter().enumerate() {
            if let Some(fill) = cell {
                let cell_x = plot_left + col as f64 * cell_w;
                items.push(Prim::Rect {
                    x: cell_x,
                    y: cell_y,
                    w: cell_w,
                    h: cell_h,
                    fill: *fill,
                });
            }
        }
    }

    // x-axis labels (below each column, centered)
    for (col, label) in x_labels.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left + col as f64 * cell_w + cell_w / 2.0,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: label.clone(),
            rotate_deg: None,
        });
    }

    // y-axis labels (left of each row, right-anchored)
    for (row, label) in y_labels.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: plot_top + row as f64 * cell_h + cell_h / 2.0 + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: label.clone(),
            rotate_deg: None,
        });
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
