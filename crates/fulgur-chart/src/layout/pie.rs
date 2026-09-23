//! pie / doughnut チャート。軸・グリッドを持たず、タイトルと凡例(カテゴリ別)を自前で描く。

use super::common;
use crate::ir::{ArcBorderRadius, ChartKind, ChartSpec, Color, LegendPos, PieCutout};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;
use std::fmt::Write;

/// スライス境界の白線（chart.js 風）。
pub(crate) const SLICE_STROKE: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 1.0,
};

/// データラベルの文字色(スライス上で読めるよう白)。
pub(crate) const LABEL_COLOR: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 1.0,
};

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut items: Vec<Prim> = Vec::new();

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // doughnut の内径比。
    let no_dataset_options: &[crate::ir::PieGeometryOptions] = &[];
    let (cutout, dataset_options) = match &spec.kind {
        ChartKind::Pie {
            cutout,
            dataset_options,
        } => (*cutout, dataset_options.as_slice()),
        _ => (PieCutout::Percent(0.0), no_dataset_options),
    };

    let series = spec.series.first();

    // 1. タイトル。
    let title_band = if spec.title.is_some() {
        common::TITLE_BAND
    } else {
        0.0
    };
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: common::OUTER_PAD + common::TITLE_FONT,
            size: common::TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // 2. 凡例(カテゴリ別)。
    let legend_title = common::legend_title(spec);
    let has_legend = matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && (spec.categories.iter().any(|c| !c.is_empty()) || legend_title.is_some());
    let legend_font = common::legend_label_font_size(&spec.legend_options, label_font);
    let legend_height = common::legend_horizontal_band_height(
        &spec.legend_options,
        label_font,
        legend_title.is_some(),
    );
    let legend_top = if has_legend && spec.legend == LegendPos::Top {
        legend_height
    } else {
        0.0
    };
    let legend_bottom = if has_legend && spec.legend == LegendPos::Bottom {
        legend_height
    } else {
        0.0
    };
    // Left/Right の凡例帯幅(カテゴリ名から算出)。
    let legend_left = if has_legend && spec.legend == LegendPos::Left {
        let mut names = spec.categories.clone();
        names.extend(legend_title.map(str::to_owned));
        common::legend_band_width_vertical_styled(m, &names, legend_font, &spec.legend_options)
    } else {
        0.0
    };
    let legend_right = if has_legend && spec.legend == LegendPos::Right {
        let mut names = spec.categories.clone();
        names.extend(legend_title.map(str::to_owned));
        common::legend_band_width_vertical_styled(m, &names, legend_font, &spec.legend_options)
    } else {
        0.0
    };
    if has_legend && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let entries: Vec<(String, Color)> = spec
            .categories
            .iter()
            .enumerate()
            .map(|(index, category)| {
                let color = series.map(|series| series.fill_at(index)).unwrap_or(ink);
                (category.clone(), color)
            })
            .collect();
        let legend_cy = if spec.legend == LegendPos::Top {
            common::OUTER_PAD + title_band + legend_height / 2.0
        } else {
            spec.height - common::OUTER_PAD - legend_height / 2.0
        };
        common::draw_horizontal_legend(
            &mut items,
            &entries,
            legend_title,
            spec.width,
            legend_cy,
            label_font,
            ink,
            m,
            &spec.legend_options,
        );
    }

    // 2b. 凡例(Left/Right: 縦並び、カテゴリ別)。
    if has_legend && matches!(spec.legend, LegendPos::Left | LegendPos::Right) {
        let entries: Vec<(String, Color)> = spec
            .categories
            .iter()
            .enumerate()
            .map(|(i, cat)| {
                let swatch = series.map(|s| s.fill_at(i)).unwrap_or(ink);
                (cat.clone(), swatch)
            })
            .collect();
        let band_w = if spec.legend == LegendPos::Left {
            legend_left
        } else {
            legend_right
        };
        let band_x = if spec.legend == LegendPos::Left {
            common::OUTER_PAD
        } else {
            spec.width - common::OUTER_PAD - band_w
        };
        // 円の縦スパン(area_top..area_bottom)中央に揃える。
        let area_top = common::OUTER_PAD + title_band + legend_top;
        let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
        common::draw_vertical_legend_styled(
            &mut items,
            &entries,
            legend_title,
            band_x,
            area_top,
            area_bottom,
            ink,
            label_font,
            &spec.legend_options,
        );
    }

    // 3. 円の領域。
    let area_top = common::OUTER_PAD + title_band + legend_top;
    let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
    let area_left = common::OUTER_PAD + legend_left;
    let area_right = spec.width - common::OUTER_PAD - legend_right;
    let cx = (area_left + area_right) / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let spacing_reserve = dataset_options
        .iter()
        .map(|options| {
            if options.spacing.is_finite() {
                options.spacing.max(0.0)
            } else {
                0.0
            }
        })
        .fold(0.0_f64, f64::max)
        / 2.0;
    let max_offset = spec
        .series
        .iter()
        .enumerate()
        .flat_map(|(dataset_index, dataset)| {
            let options = dataset_options.get(dataset_index);
            (0..dataset.values.len())
                .filter_map(move |index| options.map(|options| options.offset_at(index)))
        })
        .filter(|offset| offset.is_finite())
        .map(|offset| offset.abs())
        .fold(0.0_f64, f64::max);
    let offset_reserve = max_offset / 2.0;
    let radius = ((area_right - area_left).min(area_bottom - area_top) / 2.0 * 0.9
        - spacing_reserve
        - offset_reserve)
        .max(0.0);
    let inner = cutout.inner_radius(radius);

    // 4. データセットごとの同心円リング。dataset[0] が最外周。
    let mut labels: Vec<Prim> = Vec::new();
    if radius > inner && !spec.series.is_empty() {
        let ring_thickness = (radius - inner) / spec.series.len() as f64;
        for (dataset_index, dataset) in spec.series.iter().enumerate() {
            let ring_outer = radius - ring_thickness * dataset_index as f64;
            let ring_inner = if dataset_index + 1 == spec.series.len() {
                inner
            } else {
                radius - ring_thickness * (dataset_index + 1) as f64
            };
            let total: f64 = dataset
                .values
                .iter()
                .filter(|value| value.is_finite() && **value > 0.0)
                .sum();
            if !(total.is_finite() && total > 0.0) {
                continue;
            }

            let geometry_options = dataset_options.get(dataset_index);
            let spacing = geometry_options
                .map(|options| options.spacing)
                .filter(|spacing| spacing.is_finite())
                .unwrap_or(0.0)
                .max(0.0);
            let arc_spacing = spacing / 2.0;
            let mut a0 = -PI / 2.0; // 12 時方向。
            for (i, &value) in dataset.values.iter().enumerate() {
                if !(value.is_finite() && value > 0.0) {
                    continue; // v<=0 は角度を進めずスキップ。
                }
                let a1 = a0 + (value / total) * 2.0 * PI;
                let fill = dataset.fill_at(i);
                let offset = geometry_options
                    .map(|options| options.offset_at(i))
                    .filter(|offset| offset.is_finite())
                    .unwrap_or(0.0);
                let border_radius = geometry_options
                    .map(|options| options.border_radius_at(i))
                    .unwrap_or(ArcBorderRadius::Uniform(0.0));
                let label_angle = (a0 + a1) / 2.0;
                let offset_x = (offset / 4.0) * label_angle.cos();
                let offset_y = (offset / 4.0) * label_angle.sin();
                let radius_offset = (offset / 4.0) * (1.0 - (a1 - a0).min(PI).sin());
                let radial_adjustment = arc_spacing + radius_offset;
                let geom = Geom {
                    cx: cx + offset_x,
                    cy: cy + offset_y,
                    r_outer: (ring_outer + radial_adjustment).max(0.0),
                    r_inner: if ring_inner > 0.0 {
                        (ring_inner + radial_adjustment)
                            .max(0.0)
                            .min((ring_outer + radial_adjustment).max(0.0))
                    } else {
                        0.0
                    },
                };

                // Full circles need two SVG arcs. Keep a single center translation for both halves;
                // spacing and corner radii have no exposed arc boundary on a self-joined circle.
                if a1 - a0 >= 2.0 * PI - 1e-9 {
                    let amid = a0 + (a1 - a0) / 2.0;
                    items.push(make_slice(&geom, a0, amid, fill));
                    items.push(make_slice(&geom, amid, a1, fill));
                } else if let Some(slice) =
                    make_configured_slice(&geom, a0, a1, fill, arc_spacing, border_radius)
                {
                    items.push(slice);
                }

                if spec.data_labels {
                    let label_radius = (ring_inner + ring_outer) / 2.0;
                    labels.push(common::value_label(
                        cx + offset_x + label_radius * label_angle.cos(),
                        cy + offset_y
                            + label_radius * label_angle.sin()
                            + label_font * common::TEXT_BASELINE_RATIO,
                        label_font,
                        Anchor::Middle,
                        LABEL_COLOR,
                        value,
                        false, // pie/doughnut に対数軸の概念はない
                    ));
                }
                a0 = a1;
            }
        }
    }

    // ラベルは全スライスの上に描く（最後に push）。
    items.extend(labels);

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// 円スライスのジオメトリ（中心と内外半径）。
pub(crate) struct Geom {
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    pub(crate) r_outer: f64,
    pub(crate) r_inner: f64,
}

/// 1 スライス分の Path プリミティブを生成する。
pub(crate) fn make_slice(g: &Geom, a0: f64, a1: f64, fill: Color) -> Prim {
    Prim::Path {
        d: slice_path(g, a0, a1),
        fill: Some(fill),
        stroke: Some(SLICE_STROKE),
        stroke_width: 1.0,
    }
}

fn make_configured_slice(
    g: &Geom,
    a0: f64,
    a1: f64,
    fill: Color,
    spacing: f64,
    border_radius: ArcBorderRadius,
) -> Option<Prim> {
    let sweep = a1 - a0;
    if !(sweep.is_finite() && sweep > 0.0) {
        return None;
    }
    if spacing == 0.0 && border_radius_is_zero(border_radius) {
        return Some(make_slice(g, a0, a1, fill));
    }

    let average_radius = (g.r_outer + g.r_inner) / 2.0;
    let trim_limit = (sweep / 2.0 - 1e-6).max(0.0);
    let spacing_trim = if spacing > 0.0 && average_radius > 0.0 {
        (spacing / average_radius)
            .clamp(0.0, 1.0)
            .asin()
            .min(trim_limit)
    } else {
        0.0
    };
    let start = a0 + spacing_trim;
    let end = a1 - spacing_trim;
    if end <= start {
        return None;
    }

    Some(Prim::Path {
        d: rounded_sector_path(g, start, end, border_radius),
        fill: Some(fill),
        stroke: Some(SLICE_STROKE),
        stroke_width: 1.0,
    })
}

fn border_radius_is_zero(radius: ArcBorderRadius) -> bool {
    match radius {
        ArcBorderRadius::Uniform(value) => value == 0.0,
        ArcBorderRadius::Corners {
            outer_start,
            outer_end,
            inner_start,
            inner_end,
        } => outer_start == 0.0 && outer_end == 0.0 && inner_start == 0.0 && inner_end == 0.0,
    }
}

fn rounded_sector_path(g: &Geom, start: f64, end: f64, radius: ArcBorderRadius) -> String {
    let Geom {
        cx,
        cy,
        r_outer,
        r_inner,
    } = *g;
    let sweep = end - start;
    let half_thickness = ((r_outer - r_inner) / 2.0).max(0.0);
    let angle_sine = (sweep / 4.0).sin().max(0.0);
    let outer_angle_limit = r_outer * angle_sine / (1.0 + angle_sine);
    let inner_angle_limit = if r_inner > 0.0 && angle_sine < 1.0 {
        r_inner * angle_sine / (1.0 - angle_sine)
    } else {
        0.0
    };
    let requested = match radius {
        ArcBorderRadius::Uniform(value) => (value, value, value, value),
        ArcBorderRadius::Corners {
            outer_start,
            outer_end,
            inner_start,
            inner_end,
        } => (outer_start, outer_end, inner_start, inner_end),
    };
    let clean = |value: f64, limit: f64| {
        if value.is_finite() {
            value.max(0.0).min(half_thickness).min(limit)
        } else {
            0.0
        }
    };
    let outer_start = clean(requested.0, outer_angle_limit);
    let outer_end = clean(requested.1, outer_angle_limit);
    let inner_start = clean(requested.2, inner_angle_limit);
    let inner_end = clean(requested.3, inner_angle_limit);

    let outer_start_delta = corner_angle(outer_start, r_outer - outer_start);
    let outer_end_delta = corner_angle(outer_end, r_outer - outer_end);
    let inner_start_delta = corner_angle(inner_start, r_inner + inner_start);
    let inner_end_delta = corner_angle(inner_end, r_inner + inner_end);

    let outer_start_contact = polar(
        cx,
        cy,
        (r_outer - outer_start) * outer_start_delta.cos(),
        start,
    );
    let outer_start_arc = polar(cx, cy, r_outer, start + outer_start_delta);
    let outer_end_arc = polar(cx, cy, r_outer, end - outer_end_delta);
    let outer_end_contact = polar(cx, cy, (r_outer - outer_end) * outer_end_delta.cos(), end);

    let mut path = String::new();
    write!(
        path,
        "M {} {}",
        fmt_num(outer_start_contact.0),
        fmt_num(outer_start_contact.1)
    )
    .unwrap();
    if outer_start > 0.0 {
        write!(
            path,
            " A {} {} 0 0 1 {} {}",
            fmt_num(outer_start),
            fmt_num(outer_start),
            fmt_num(outer_start_arc.0),
            fmt_num(outer_start_arc.1)
        )
        .unwrap();
    }
    write!(
        path,
        " A {} {} 0 {} 1 {} {}",
        fmt_num(r_outer),
        fmt_num(r_outer),
        i32::from(end - outer_end_delta - start - outer_start_delta > PI),
        fmt_num(outer_end_arc.0),
        fmt_num(outer_end_arc.1)
    )
    .unwrap();
    if outer_end > 0.0 {
        write!(
            path,
            " A {} {} 0 0 1 {} {}",
            fmt_num(outer_end),
            fmt_num(outer_end),
            fmt_num(outer_end_contact.0),
            fmt_num(outer_end_contact.1)
        )
        .unwrap();
    } else {
        write!(
            path,
            " L {} {}",
            fmt_num(outer_end_contact.0),
            fmt_num(outer_end_contact.1)
        )
        .unwrap();
    }

    if r_inner > 0.0 {
        let inner_end_contact = polar(cx, cy, (r_inner + inner_end) * inner_end_delta.cos(), end);
        let inner_end_arc = polar(cx, cy, r_inner, end - inner_end_delta);
        let inner_start_arc = polar(cx, cy, r_inner, start + inner_start_delta);
        let inner_start_contact = polar(
            cx,
            cy,
            (r_inner + inner_start) * inner_start_delta.cos(),
            start,
        );
        write!(
            path,
            " L {} {}",
            fmt_num(inner_end_contact.0),
            fmt_num(inner_end_contact.1)
        )
        .unwrap();
        if inner_end > 0.0 {
            write!(
                path,
                " A {} {} 0 0 1 {} {}",
                fmt_num(inner_end),
                fmt_num(inner_end),
                fmt_num(inner_end_arc.0),
                fmt_num(inner_end_arc.1)
            )
            .unwrap();
        }
        write!(
            path,
            " A {} {} 0 {} 0 {} {}",
            fmt_num(r_inner),
            fmt_num(r_inner),
            i32::from(end - inner_end_delta - start - inner_start_delta > PI),
            fmt_num(inner_start_arc.0),
            fmt_num(inner_start_arc.1)
        )
        .unwrap();
        if inner_start > 0.0 {
            write!(
                path,
                " A {} {} 0 0 1 {} {}",
                fmt_num(inner_start),
                fmt_num(inner_start),
                fmt_num(inner_start_contact.0),
                fmt_num(inner_start_contact.1)
            )
            .unwrap();
        }
        write!(
            path,
            " L {} {} Z",
            fmt_num(outer_start_contact.0),
            fmt_num(outer_start_contact.1)
        )
        .unwrap();
    } else {
        write!(
            path,
            " L {} {} L {} {} Z",
            fmt_num(cx),
            fmt_num(cy),
            fmt_num(outer_start_contact.0),
            fmt_num(outer_start_contact.1)
        )
        .unwrap();
    }
    path
}

fn corner_angle(radius: f64, radial_distance: f64) -> f64 {
    if radius > 0.0 && radial_distance > 0.0 {
        (radius / radial_distance).clamp(0.0, 1.0).asin()
    } else {
        0.0
    }
}

fn polar(cx: f64, cy: f64, radius: f64, angle: f64) -> (f64, f64) {
    (cx + radius * angle.cos(), cy + radius * angle.sin())
}

/// 円弧スライスの SVG path data を生成する。`a1 > a0` かつ `a1-a0 < 2π` を前提とする。
/// 角度増加方向は SVG 座標(y 下向き)で時計回り＝sweep 1。
fn slice_path(g: &Geom, a0: f64, a1: f64) -> String {
    let Geom {
        cx,
        cy,
        r_outer,
        r_inner,
    } = *g;
    let laf = if (a1 - a0) > PI { 1 } else { 0 };
    let o0 = (cx + r_outer * a0.cos(), cy + r_outer * a0.sin());
    let o1 = (cx + r_outer * a1.cos(), cy + r_outer * a1.sin());
    let mut d = String::new();
    if r_inner > 0.0 {
        // doughnut: 外弧 a0→a1 (sweep 1)、内弧 a1→a0 (sweep 0) で戻る。
        let i0 = (cx + r_inner * a0.cos(), cy + r_inner * a0.sin());
        let i1 = (cx + r_inner * a1.cos(), cy + r_inner * a1.sin());
        write!(
            d,
            "M {} {} A {} {} 0 {} 1 {} {} L {} {} A {} {} 0 {} 0 {} {} Z",
            fmt_num(o0.0),
            fmt_num(o0.1),
            fmt_num(r_outer),
            fmt_num(r_outer),
            laf,
            fmt_num(o1.0),
            fmt_num(o1.1),
            fmt_num(i1.0),
            fmt_num(i1.1),
            fmt_num(r_inner),
            fmt_num(r_inner),
            laf,
            fmt_num(i0.0),
            fmt_num(i0.1),
        )
        .unwrap();
    } else {
        // pie: 中心→外周→外弧→閉じる。
        write!(
            d,
            "M {} {} L {} {} A {} {} 0 {} 1 {} {} Z",
            fmt_num(cx),
            fmt_num(cy),
            fmt_num(o0.0),
            fmt_num(o0.1),
            fmt_num(r_outer),
            fmt_num(r_outer),
            laf,
            fmt_num(o1.0),
            fmt_num(o1.1),
        )
        .unwrap();
    }
    d
}
