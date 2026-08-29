//! line / area チャート。共有フレーム(common)の上に折れ線・面・マーカーを重ねる。

use super::{common, monotone::monotone_path};
use crate::ir::{ChartKind, ChartSpec, StepMode};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::fmt::Write;

/// マーカー（点）の半径。
const MARKER_R: f64 = 3.0;

/// 欠損を除いた点列を、元カテゴリの不連続箇所で線分へ分割する。
/// `span_gaps` 時は不連続をまたいで 1 本の線分として扱う。
fn segments_for_valid_points(
    valid: &[(f64, f64, usize)],
    span_gaps: bool,
) -> Vec<Vec<(f64, f64, usize)>> {
    if span_gaps {
        return (!valid.is_empty())
            .then(|| valid.to_vec())
            .into_iter()
            .collect();
    }

    let mut segments = Vec::new();
    let mut current = Vec::new();
    let mut previous_category = None;
    for &(x, y, category) in valid {
        if previous_category.is_some_and(|previous| category != previous + 1) && !current.is_empty()
        {
            segments.push(std::mem::take(&mut current));
        }
        current.push((x, y, category));
        previous_category = Some(category);
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// 隣接する点の間に階段状の折れ点を追加する。
fn step_capacity(point_count: usize, mode: StepMode) -> usize {
    let intermediate_per_segment = match mode {
        StepMode::Before | StepMode::After => 1,
        StepMode::Middle => 2,
    };
    point_count.saturating_add(
        point_count
            .saturating_sub(1)
            .saturating_mul(intermediate_per_segment),
    )
}

fn step_points(
    points: impl ExactSizeIterator<Item = (f64, f64)> + Clone,
    mode: StepMode,
) -> Vec<(f64, f64)> {
    let point_count = points.len();
    let Some(first) = points.clone().next() else {
        return Vec::new();
    };

    let mut stepped = Vec::with_capacity(step_capacity(point_count, mode));
    stepped.push(first);
    let mut push_if_new = |point| {
        if stepped.last() != Some(&point) {
            stepped.push(point);
        }
    };
    for (previous, target) in points.clone().zip(points.skip(1)) {
        let (x0, y0) = previous;
        let (x1, y1) = target;
        match mode {
            StepMode::Before => push_if_new((x1, y0)),
            StepMode::After => push_if_new((x0, y1)),
            StepMode::Middle => {
                let middle_x = (x0 + x1) / 2.0;
                push_if_new((middle_x, y0));
                push_if_new((middle_x, y1));
            }
        }
        push_if_new((x1, y1));
    }
    stepped
}

enum AreaPoints<'a> {
    Borrowed(&'a [(f64, f64, usize)]),
    Stepped(Vec<(f64, f64)>),
}

fn area_points(segment: &[(f64, f64, usize)], step_mode: Option<StepMode>) -> AreaPoints<'_> {
    step_mode
        .map(|step_mode| {
            AreaPoints::Stepped(step_points(
                segment.iter().map(|&(x, y, _)| (x, y)),
                step_mode,
            ))
        })
        .unwrap_or(AreaPoints::Borrowed(segment))
}

fn append_area_points(d: &mut String, points: impl IntoIterator<Item = (f64, f64)>) -> (f64, f64) {
    let mut points = points.into_iter();
    let (first_x, first_y) = points.next().expect("area segment is non-empty");
    write!(d, "M {} {} ", fmt_num(first_x), fmt_num(first_y)).unwrap();

    let mut last_x = first_x;
    for (x, y) in points {
        write!(d, "L {} {} ", fmt_num(x), fmt_num(y)).unwrap();
        last_x = x;
    }
    (first_x, last_x)
}

/// line チャートのモデル幾何用の全マーカー点（`model::build_model` が参照）。
/// レンダリング経路の `build()` は点を独立に計算しデシメーションするため、巨大データでは
/// この全点列と実際の描画点は乖離する（モデルは chart.js 数値照合用＝間引きなしが正しい）。
/// 非stacked: 欠損値 (get() None) と非有限値 (NaN / ±∞) は skip し point は emit しない
/// (bar の `vertical_bar_boxes` と同じ null 挙動)。対数y軸では値0も `build()` と同じく
/// skip する(chart.js は log 軸上の値0を欠損として扱うため。この乖離は自動レビュー指摘で発見・修正した)。
/// stacked: `build()` の `valid` 構築と同じくガードを一切適用せず、全カテゴリで point を
/// emit する(欠損/非有限は `stack_offsets` が 0 として補完済み; 対数軸との組み合わせは
/// `value_domain` 側で未対応・到達不能。1系列だけ欠損があっても隣接系列の帯は一貫している
/// 必要があるため、非stacked と違い skip しない — これも自動レビュー指摘で発見・修正した)。
pub fn line_points(
    spec: &crate::ir::ChartSpec,
    frame: &common::Frame,
) -> Vec<crate::layout::scatter::PointBox> {
    let is_log = spec.y_axis.scale_kind == crate::ir::ScaleKind::Logarithmic;
    let stacked = matches!(spec.kind, ChartKind::Line { stacked: true });
    let offsets = stacked.then(|| stack_offsets(spec));
    let mut pts = Vec::new();
    for (sidx, ser) in spec.series.iter().enumerate() {
        if ser.point_radius.is_some_and(|radius| radius <= 0.0) {
            continue;
        }
        for i in 0..spec.categories.len() {
            let x = common::line_x(spec, frame, i);
            let plot_y = if let Some(offsets) = &offsets {
                offsets[sidx][i].1 // far
            } else {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                if is_log && v == 0.0 {
                    continue;
                }
                v
            };
            pts.push(crate::layout::scatter::PointBox {
                series: sidx,
                index: i,
                kind: "line",
                cx: x,
                cy: frame.ys.map(plot_y),
                r: MARKER_R,
            });
        }
    }
    pts
}

/// 積み上げ area のカテゴリ・系列ごとの (near, far) オフセットを計算する。
/// 正値は正側の running total、負値は負側の running total に独立で積む
/// (Vega-Lite の stack:"zero" と同じ; `value_domain` の正負サム分離と対応する)。
/// near = この系列を足す前の running total(baseline 側/隣接帯との共有辺)、
/// far = 足した後の running total(stroke/marker を置く辺)。非有限値は 0 として扱う
/// (積み上げ上の欠損補完; Vega-Lite の stack transform と同じ)。
fn stack_offsets(spec: &ChartSpec) -> Vec<Vec<(f64, f64)>> {
    let n = spec.categories.len();
    let mut pos_running = vec![0.0_f64; n];
    let mut neg_running = vec![0.0_f64; n];
    spec.series
        .iter()
        .map(|ser| {
            (0..n)
                .map(|i| {
                    let v = ser
                        .values
                        .get(i)
                        .copied()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0);
                    let running = if v >= 0.0 {
                        &mut pos_running[i]
                    } else {
                        &mut neg_running[i]
                    };
                    let near = *running;
                    *running += v;
                    (near, *running)
                })
                .collect()
        })
        .collect()
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let frame = common::compute(spec, m);
    let is_log = spec.y_axis.scale_kind == crate::ir::ScaleKind::Logarithmic;
    let stacked = matches!(spec.kind, ChartKind::Line { stacked: true });
    // 積み上げは常に密なデータ前提(色分け系列は必ず全カテゴリで値を持つ; VL フロントエンドが
    // build_categorical/build_temporal_line で保証する)なので gap 分割・間引きを行わない。
    // 複数系列を独立に間引くと x 位置がずれてスタックが破綻するため意図的にスキップする。
    let offsets = stacked.then(|| stack_offsets(spec));

    let mut items: Vec<Prim> = Vec::new();
    common::draw_frame(&mut items, spec, &frame, m);

    for (si, ser) in spec.series.iter().enumerate() {
        // 有効点列: (x, y, 元カテゴリインデックス)。欠損・非有限値を除外。
        // 対数y軸では 0 も欠損(gap)として扱う: chart.js は log 軸上の値0を
        // "skip" 点として扱い(ドメイン計算にだけ使い、マーカー・線分は描かない)、
        // その実測(tools/ で node chart.js 実行して確認)に合わせている。
        // 元インデックスはラベル lookup と gap 検出に使う。
        let valid: Vec<(f64, f64, usize)> = (0..spec.categories.len())
            .filter_map(|i| {
                let x = common::line_x(spec, &frame, i);
                if let Some(offsets) = &offsets {
                    Some((x, frame.ys.map(offsets[si][i].1), i))
                } else {
                    let v = ser.values.get(i).copied()?;
                    if !v.is_finite() {
                        return None;
                    }
                    if is_log && v == 0.0 {
                        return None;
                    }
                    Some((x, frame.ys.map(v), i))
                }
            })
            .collect();

        // 元インデックスが連続しない箇所でセグメントを分割する。
        // chart.js の spanGaps=false デフォルトと同じ「欠損で線が途切れる」挙動。
        // 間引きは cat を保持したまま各セグメントへ適用するため、cat を含めて分割する
        // （間引き後に cat で再分割すると全点が gap 扱いになり線が消えるため、再分割しない）。
        let segments = segments_for_valid_points(&valid, ser.span_gaps);

        // デシメーション判定は系列全体の点数で（gap 分割の前後で一貫）。
        // 各セグメントを個別に間引き、line はその結果から直接描く（再分割しない）。
        // 積み上げ area は必ずスキップする: 系列ごとに独立に間引くと生存する x 位置が
        // 系列間でずれ、帯(near/far)が食い違って壊れるため(このコメント直上の comment 参照)。
        let plot_width = frame.plot_right - frame.plot_left;
        let dec = if stacked {
            None
        } else {
            crate::layout::decimate::resolve(&spec.decimation, plot_width, valid.len())
        };
        let decimated = dec.is_some();
        let segments: Vec<Vec<(f64, f64, usize)>> = if let Some((algo, samples)) = dec {
            // samples はセグメント長で按分される（decimate_segments）。これにより gap で
            // 多数セグメントに割れた LTTB 系列でも合計が samples+3×セグメント数 以下に収まる
            // （min-max は samples を無視し占有ピクセル列数で自己制限）。
            crate::layout::decimate::decimate_segments(&segments, algo, samples)
        } else {
            segments
        };
        // area/marker/label 用に間引き後の点列へ差し替え（Chart.js dataset.data 差し替えモデル）。
        // cat は維持するため、ラベルの ser.values[cat] 参照は引き続き正しい。
        let valid: Vec<(f64, f64, usize)> = segments.iter().flatten().copied().collect();

        // area(背面): 線と同じくセグメント単位で 1 つずつ閉多角形を描く。
        // gap を跨いだ塗り(線は途切れているのに塗りは繋がる)を防ぐ。
        // 非 null / 非 gap 系列では segments が 1 本のため、旧「valid 全体で 1 多角形」
        // 経路と同一のパスデータを出力する(バイト不変)。
        if ser.area {
            let baseline_y = frame
                .ys
                .map(0.0_f64.clamp(frame.ticks.min, frame.ticks.max));
            for seg in &segments {
                if seg.is_empty() {
                    continue;
                }
                let mut d = String::new();
                let (first_x, last_x) = match area_points(seg, ser.step_mode) {
                    AreaPoints::Borrowed(points) => {
                        append_area_points(&mut d, points.iter().map(|&(x, y, _)| (x, y)))
                    }
                    AreaPoints::Stepped(points) => append_area_points(&mut d, points),
                };
                if let Some(offsets) = &offsets {
                    for &(_, _, cat) in seg.iter().rev() {
                        let near_x = common::line_x(spec, &frame, cat);
                        let near_y = frame.ys.map(offsets[si][cat].0);
                        write!(d, "L {} {} ", fmt_num(near_x), fmt_num(near_y)).unwrap();
                    }
                    write!(d, "Z").unwrap();
                } else {
                    write!(
                        d,
                        "L {} {} L {} {} Z",
                        fmt_num(last_x),
                        fmt_num(baseline_y),
                        fmt_num(first_x),
                        fmt_num(baseline_y)
                    )
                    .unwrap();
                }
                items.push(Prim::Path {
                    d,
                    fill: Some(ser.fill_at(0)),
                    stroke: None,
                    stroke_width: 0.0,
                });
            }
        }

        // 線: セグメントごとに描く(gap で線が途切れる)。間引き済みセグメントから直接描画する。
        for seg in &segments {
            if seg.len() < 2 {
                continue;
            }
            if let Some(step_mode) = ser.step_mode {
                items.push(Prim::Polyline {
                    points: step_points(seg.iter().map(|&(x, y, _)| (x, y)), step_mode),
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
                continue;
            }
            let xy: Vec<(f64, f64)> = seg.iter().map(|&(x, y, _)| (x, y)).collect();
            match ser.interpolation {
                crate::ir::LineInterpolation::Linear => {
                    items.push(Prim::Polyline {
                        points: xy,
                        stroke: ser.stroke_at(0),
                        stroke_width: ser.stroke_width,
                    });
                }
                crate::ir::LineInterpolation::CatmullRom { tension } => {
                    let d = catmull_rom_path(&xy, tension);
                    items.push(Prim::Path {
                        d,
                        fill: None,
                        stroke: Some(ser.stroke_at(0)),
                        stroke_width: ser.stroke_width,
                    });
                }
                crate::ir::LineInterpolation::Monotone => {
                    items.push(Prim::Path {
                        d: monotone_path(&xy),
                        fill: None,
                        stroke: Some(ser.stroke_at(0)),
                        stroke_width: ser.stroke_width,
                    });
                }
            }
        }

        // マーカー。threshold 超過で間引いた場合、線として描かれる(≥2点)セグメントの帯マーカーは
        // 既定で抑制する。ただし単点セグメント(gap で孤立し線にならない点)はマーカーが唯一の
        // 表現なので描画し、空チャート化を防ぐ。pointRadius 明示時は全点描画(エスケープハッチ)。
        // 非間引き時は従来どおり全点を MARKER_R で描画(バイト不変。segments を平坦化すると valid と
        // 同順・同内容)。
        for seg in &segments {
            let r = match (decimated, ser.point_radius) {
                (_, Some(r)) if r > 0.0 => Some(r),
                (_, Some(_)) => None,
                (false, None) => Some(MARKER_R),
                // 間引き既定: 線になる(≥2点)なら帯を抑制、単点(孤立点)は描画。
                (true, None) if seg.len() < 2 => Some(MARKER_R),
                (true, None) => None,
            };
            if let Some(r) = r {
                for &(cx, cy, _) in seg {
                    items.push(Prim::Circle {
                        cx,
                        cy,
                        r,
                        fill: ser.stroke_at(0),
                        stroke: ser.stroke_at(0),
                        stroke_width: 0.0,
                    });
                }
            }
        }

        // データラベル。非積み上げ: 点の上、マーカー半径ぶん+余白だけ上。
        // 積み上げ: 自帯のピクセル空間中点(bar.rs の
        // stacked_data_label_midpoint_uses_pixel_space_not_value_space_under_log_scale /
        // stacked_data_label_midpoint_unaffected_by_fix_under_linear_scale と同じ規約)。
        // `y - marker - gap`(非積み上げと同じ式)だと、最上位以外の系列のラベルは
        // 自分の far(=真上の帯の near)のすぐ上に来るため、真上の帯へ食い込む
        // (Task 3 の2回目コードレビューで指摘)。near/far をそれぞれ map してから
        // ピクセル空間で平均する(値空間で先に平均して map すると log 軸で非アフィンに
        // ズレる。bar.rs 同様の理由で、この差は VL 側で log y 軸が未到達なため現状は
        // 観測できないが、layout/line.rs は log 軸対応呼び出し元と将来共有されうる)。
        // 元カテゴリインデックスで ser.values を引くことで filter 後のずれを防ぐ。
        if spec.data_labels {
            // marker 無効(0 以下)や間引きで省略された既定 marker は、従来どおり
            // MARKER_R を基準にする。正の明示 pointRadius だけ実際の marker と揃える。
            let label_marker_r = ser
                .point_radius
                .filter(|radius| *radius > 0.0)
                .unwrap_or(MARKER_R);
            for &(x, y, cat) in &valid {
                let label_y = if let Some(offsets) = &offsets {
                    let (near, far) = offsets[si][cat];
                    (frame.ys.map(near) + frame.ys.map(far)) / 2.0
                } else {
                    y - label_marker_r - common::LABEL_GAP
                };
                items.push(common::value_label(
                    x,
                    label_y,
                    spec.theme.font_size,
                    Anchor::Middle,
                    spec.theme.text_color,
                    ser.values.get(cat).copied().unwrap_or(0.0),
                    is_log,
                ));
            }
        }
    }

    Scene {
        width: frame.scene_width,
        height: frame.scene_height,
        items,
    }
}

/// Catmull-Rom スプラインを 3 次ベジエの SVG path data へ変換する。
/// 端点は自身を複製して扱う。`pts.len() >= 2` を前提とする。
fn catmull_rom_path(pts: &[(f64, f64)], tension: f64) -> String {
    let k = pts.len();
    let mut d = String::new();
    write!(d, "M {} {} ", fmt_num(pts[0].0), fmt_num(pts[0].1)).unwrap();
    for i in 0..k - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(k - 1)];
        let cp1 = (
            p1.0 + (p2.0 - p0.0) / 6.0 * tension,
            p1.1 + (p2.1 - p0.1) / 6.0 * tension,
        );
        let cp2 = (
            p2.0 - (p3.0 - p1.0) / 6.0 * tension,
            p2.1 - (p3.1 - p1.1) / 6.0 * tension,
        );
        write!(
            d,
            "C {} {} {} {} {} {} ",
            fmt_num(cp1.0),
            fmt_num(cp1.1),
            fmt_num(cp2.0),
            fmt_num(cp2.1),
            fmt_num(p2.0),
            fmt_num(p2.1)
        )
        .unwrap();
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::layout::common;
    use crate::text::TextMeasurer;

    fn pts_for(json: &str) -> Vec<crate::layout::scatter::PointBox> {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        line_points(&spec, &frame)
    }

    fn scene_for(json: &str) -> Scene {
        let spec = chartjs::parse(json, false).unwrap();
        build(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap())
    }

    fn value_label_positions(scene: &Scene) -> Vec<(String, f64, f64)> {
        scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Text { x, y, content, .. } if content == "13" || content == "-17" => {
                    Some((content.clone(), *x, *y))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn line_data_labels_clear_custom_marker_radius() {
        let scene = scene_for(
            r#"{"type":"line","data":{"labels":["a","b"],
               "datasets":[{"data":[13,-17],"pointRadius":20}]},
               "options":{"plugins":{"datalabels":{"display":true}}}}"#,
        );
        let markers: Vec<_> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cx, cy, r, .. } => Some((*cx, *cy, *r)),
                _ => None,
            })
            .collect();
        let labels = value_label_positions(&scene);

        assert_eq!(labels.len(), 2);
        assert_eq!(markers.len(), 2);
        for (_, x, y) in labels {
            let (_, marker_y, marker_r) = markers
                .iter()
                .find(|(marker_x, _, _)| (*marker_x - x).abs() < 1e-9)
                .expect("value label must align with a marker");
            assert_eq!(*marker_r, 20.0);
            assert!(
                (y - (marker_y - marker_r - common::LABEL_GAP)).abs() < 1e-9,
                "label must clear the marker: label_y={y}, marker_y={marker_y}, marker_r={marker_r}"
            );
        }
    }

    #[test]
    fn line_data_label_offsets_preserve_default_and_nonpositive_marker_radius() {
        let json = |point_radius: &str| {
            format!(
                r#"{{"type":"line","data":{{"labels":["a","b"],"datasets":[{{"data":[13,-17]{point_radius}}}]}},"options":{{"plugins":{{"datalabels":{{"display":true}}}}}}}}"#
            )
        };
        let expected = value_label_positions(&scene_for(&json("")));
        for point_radius in [
            r#","pointRadius":null"#,
            r#","pointRadius":0"#,
            r#","pointRadius":-5"#,
        ] {
            assert_eq!(
                value_label_positions(&scene_for(&json(point_radius))),
                expected,
                "pointRadius {point_radius} must retain the default label offset"
            );
        }
    }

    #[test]
    fn line_points_count_is_series_times_categories() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b","c","d","e","f","g"],
               "datasets":[{"data":[1,2,3,4,5,6,7]},{"data":[7,6,5,4,3,2,1]}]}}"#,
        );
        assert_eq!(ps.len(), 14);
        for p in &ps {
            assert_eq!(p.kind, "line");
        }
    }

    /// 実機バグ回帰テスト: `build()` は対数y軸で値0を欠損(gap)として扱いマーカー・
    /// 線分を描かないが、model 幾何用の `line_points()` はこの skip をミラーしておらず
    /// 値0の点も emit していた。model の geometry.elements が実際の描画シーンと
    /// 食い違う(自動レビュー指摘)。
    #[test]
    fn line_points_skips_zero_on_logarithmic_y_axis() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[0,10,20]}]},
               "options":{"scales":{"y":{"type":"logarithmic"}}}}"#,
        );
        assert_eq!(ps.len(), 2, "値0の点は欠損として skip されるべき");
        assert!(ps.iter().all(|p| p.index != 0));
    }

    #[test]
    fn line_points_x_is_edge_to_edge() {
        // chart.js offset:false: n=3 の点は plot_left / 中点 / plot_right に並ぶ。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let ps = line_points(&spec, &frame);
        let s0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        assert!((s0[0].cx - frame.plot_left).abs() < 1e-9);
        assert!((s0[2].cx - frame.plot_right).abs() < 1e-9);
        assert!((s0[1].cx - (frame.plot_left + frame.plot_right) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn plot_area_line_scene_uses_outer_frame_size() {
        let mut spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]}}"#,
            false,
        )
        .unwrap();
        spec.x_positions = crate::ir::XPositions::Temporal {
            unix_millis: vec![0, 86_400_000, 3 * 86_400_000],
        };
        spec.size_mode = crate::ir::SizeMode::PlotArea;
        spec.width = 720.0;
        spec.height = 320.0;
        spec.theme.background = Some(crate::ir::Color {
            r: 255,
            g: 255,
            b: 255,
            a: 1.0,
        });
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = crate::layout::build_scene(&spec, &m);
        assert_eq!(
            (scene.width, scene.height),
            (frame.scene_width, frame.scene_height)
        );
        assert!(matches!(
            scene.items.first(),
            Some(Prim::Rect { w, h, .. })
                if (*w, *h) == (frame.scene_width, frame.scene_height)
        ));
    }

    #[test]
    fn line_frame_stays_valid_when_edge_labels_exceed_width() {
        // 狭い幅 + 長い端ラベルでも edge 余白で描画領域が反転しない(plot_right >= plot_left)。
        let mut spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["VeryLongCategoryLabelLeft","VeryLongCategoryLabelRight"],
               "datasets":[{"data":[1,2]}]}}"#,
            false,
        )
        .unwrap();
        spec.width = 60.0; // edge ラベル半幅合計が利用可能幅を超える狭い幅。
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        assert!(
            frame.plot_right >= frame.plot_left,
            "plot area inverted: left={} right={}",
            frame.plot_left,
            frame.plot_right
        );
        // line_x は有限かつ先頭<=末尾(NaN や順序反転を生まない)。
        let n = spec.categories.len();
        let x0 = common::line_x(&spec, &frame, 0);
        let x_last = common::line_x(&spec, &frame, n - 1);
        assert!(x0.is_finite() && x_last.is_finite());
        assert!(x_last >= x0);
    }

    #[test]
    fn line_points_cx_monotone_with_category_order() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]}}"#,
        );
        let ser0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        assert!(ser0[0].cx < ser0[1].cx && ser0[1].cx < ser0[2].cx);
    }

    #[test]
    fn line_points_cy_tracks_value() {
        let ps = pts_for(
            r#"{"type":"line","data":{"labels":["a","b"],
               "datasets":[{"data":[10,100]}]}}"#,
        );
        let ser0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        assert!(
            ser0[1].cy < ser0[0].cy,
            "大きい値は小さい cy(上方向): ser0[0].cy={}, ser0[1].cy={}",
            ser0[0].cy,
            ser0[1].cy
        );
    }

    #[test]
    fn line_points_x_is_band_centered_when_offset() {
        // chart.js offset:true: 点は category_center(band 中心)に並ぶ。
        // n=3 なら plot_left+0.5*band_w / +1.5*band_w / +2.5*band_w。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]},
               "options":{"scales":{"x":{"offset":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let ps = line_points(&spec, &frame);
        let s0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
        let band_w = (frame.plot_right - frame.plot_left) / 3.0;
        for (i, p) in s0.iter().enumerate() {
            let expect = frame.plot_left + (i as f64 + 0.5) * band_w;
            assert!(
                (p.cx - expect).abs() < 1e-9,
                "offset:true の点は band 中心: i={i} cx={} expect={expect}",
                p.cx
            );
        }
        // edge-to-edge と区別: 先頭は plot_left より内側、末尾は plot_right より内側。
        assert!(s0[0].cx > frame.plot_left);
        assert!(s0[2].cx < frame.plot_right);
    }

    #[test]
    fn offset_line_skips_edge_padding() {
        // offset:true は bar 同様に端ラベル半幅の余白を取らない。
        // edge-to-edge(既定)では末尾ラベル半幅ぶん plot_right を内側化するため、
        // offset 版の plot_right はそれより外側(広い)になる。
        let parse = |opts: &str| {
            chartjs::parse(
                &format!(
                    r#"{{"type":"line","data":{{"labels":["Jan","Feb","Mar"],
                       "datasets":[{{"data":[10,20,30]}}]}}{opts}}}"#
                ),
                false,
            )
            .unwrap()
        };
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let edge = common::compute(&parse(""), &m);
        let off = common::compute(&parse(r#","options":{"scales":{"x":{"offset":true}}}"#), &m);
        assert!(
            off.plot_right > edge.plot_right,
            "offset:true は端余白を取らないため plot_right がより外側: off={} edge={}",
            off.plot_right,
            edge.plot_right
        );
    }

    #[test]
    fn line_with_null_and_fill_splits_area_at_gap() {
        // fill:true の line で欠損があるとき、area は gap を跨がず 2 つの閉多角形に分割される。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c","d","e"],
               "datasets":[{"data":[1, 2, null, 4, 5], "fill": true}]}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let area_paths = scene
            .items
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    Prim::Path {
                        fill: Some(_),
                        stroke: None,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            area_paths, 2,
            "area should split into 2 polygons at the gap"
        );
    }

    #[test]
    fn span_gaps_bridges_a_null_between_two_line_points() {
        let without_span_gaps = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[1, null, 3]}]}}"#,
        );
        assert!(
            !without_span_gaps
                .items
                .iter()
                .any(|item| matches!(item, Prim::Polyline { points, .. } if points.len() == 2)),
            "the default must leave the two valid points disconnected"
        );

        let with_span_gaps = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[1, null, 3], "spanGaps": true}]}}"#,
        );
        assert!(
            with_span_gaps
                .items
                .iter()
                .any(|item| matches!(item, Prim::Polyline { points, .. } if points.len() == 2)),
            "spanGaps must join the two valid points"
        );
    }

    /// 実機バグ回帰テスト: chart.js は対数y軸上の値0を "skip" 点として扱い(marker
    /// も接続線も描かない、ドメイン計算にのみ使う)。修正前は 0 を通常の有限値として
    /// 扱い、軸の床(floor)にクランプされた位置へマーカーと接続線を描いてしまい、
    /// 実際のデータにない「V字」の谷が見えてしまっていた(tools/ で node chart.js
    /// 実行して skip:true を確認、PR #144 の自動レビューで指摘)。
    #[test]
    fn logarithmic_line_treats_zero_as_a_gap_not_a_floor_clamped_point() {
        let scene = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[1, 0, 10]}]},
               "options":{"scales":{"y":{"type":"logarithmic"}}}}"#,
        );
        assert!(
            !scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::Polyline { points, .. } if points.len() >= 2)),
            "0 を挟む2区間はどちらも単独点(gap)であり、2点以上を結ぶ折れ線は出ないはず"
        );
        let marker_count = scene
            .items
            .iter()
            .filter(|item| matches!(item, Prim::Circle { .. }))
            .count();
        assert_eq!(
            marker_count, 2,
            "値0の点にはマーカーを描かない(gapとしてskip)"
        );
    }

    #[test]
    fn linear_line_still_draws_zero_as_a_normal_point() {
        // 線形軸では 0 は通常の有限値のまま(対数軸限定の挙動であることの回帰確認)。
        let scene = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[1, 0, 10]}]}}"#,
        );
        let marker_count = scene
            .items
            .iter()
            .filter(|item| matches!(item, Prim::Circle { .. }))
            .count();
        assert_eq!(marker_count, 3, "線形軸では値0も通常の点として描く");
    }

    #[test]
    fn stepped_before_emits_horizontal_corners_before_each_next_point() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, 2, 3], "stepped":"before"}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let points = scene_for(json)
            .items
            .into_iter()
            .find_map(|item| match item {
                Prim::Polyline { points, .. } => Some(points),
                _ => None,
            })
            .expect("stepped line must be a polyline");

        let x0 = common::line_x(&spec, &frame, 0);
        let x1 = common::line_x(&spec, &frame, 1);
        let x2 = common::line_x(&spec, &frame, 2);
        let y0 = frame.ys.map(1.0);
        let y1 = frame.ys.map(2.0);
        let y2 = frame.ys.map(3.0);
        assert_eq!(
            points,
            vec![(x0, y0), (x1, y0), (x1, y1), (x2, y1), (x2, y2)]
        );
    }

    #[test]
    fn stepped_after_emits_vertical_corners_at_each_previous_point() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, 2, 3], "stepped":"after"}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let points = scene_for(json)
            .items
            .into_iter()
            .find_map(|item| match item {
                Prim::Polyline { points, .. } => Some(points),
                _ => None,
            })
            .expect("stepped line must be a polyline");

        let x0 = common::line_x(&spec, &frame, 0);
        let x1 = common::line_x(&spec, &frame, 1);
        let x2 = common::line_x(&spec, &frame, 2);
        let y0 = frame.ys.map(1.0);
        let y1 = frame.ys.map(2.0);
        let y2 = frame.ys.map(3.0);
        assert_eq!(
            points,
            vec![(x0, y0), (x0, y1), (x1, y1), (x1, y2), (x2, y2)]
        );
    }

    #[test]
    fn stepped_middle_emits_two_corners_at_each_midpoint() {
        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, 2, 3], "stepped":"middle"}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let points = scene_for(json)
            .items
            .into_iter()
            .find_map(|item| match item {
                Prim::Polyline { points, .. } => Some(points),
                _ => None,
            })
            .expect("stepped line must be a polyline");

        let x0 = common::line_x(&spec, &frame, 0);
        let x1 = common::line_x(&spec, &frame, 1);
        let x2 = common::line_x(&spec, &frame, 2);
        let y0 = frame.ys.map(1.0);
        let y1 = frame.ys.map(2.0);
        let y2 = frame.ys.map(3.0);
        assert_eq!(
            points,
            vec![
                (x0, y0),
                ((x0 + x1) / 2.0, y0),
                ((x0 + x1) / 2.0, y1),
                (x1, y1),
                ((x1 + x2) / 2.0, y1),
                ((x1 + x2) / 2.0, y2),
                (x2, y2),
            ]
        );
    }

    #[test]
    fn stepped_flat_pair_omits_adjacent_duplicate_vertices() {
        for (mode, uses_middle_corners) in [("before", false), ("after", false), ("middle", true)] {
            let json = format!(
                r#"{{"type":"line","data":{{"labels":["a","b"],
                    "datasets":[{{"data":[2, 2], "stepped":"{mode}"}}]}}}}"#
            );
            let spec = chartjs::parse(&json, false).unwrap();
            let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
            let points = scene_for(&json)
                .items
                .into_iter()
                .find_map(|item| match item {
                    Prim::Polyline { points, .. } => Some(points),
                    _ => None,
                })
                .expect("stepped line must be a polyline");

            let start = (common::line_x(&spec, &frame, 0), frame.ys.map(2.0));
            let end = (common::line_x(&spec, &frame, 1), frame.ys.map(2.0));
            let expected = if uses_middle_corners {
                vec![start, ((start.0 + end.0) / 2.0, start.1), end]
            } else {
                vec![start, end]
            };
            assert_eq!(
                points, expected,
                "{mode} should retain the direct endpoints"
            );
            assert!(
                points.windows(2).all(|pair| pair[0] != pair[1]),
                "{mode} emitted adjacent duplicate vertices: {points:?}"
            );
        }
    }

    #[test]
    fn stepped_area_uses_step_corners_and_span_gaps_bridges_its_fill() {
        let without_span_gaps = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
                "datasets":[{"data":[1, null, 3], "fill": true, "stepped":"before"}]}}"#,
        );
        let default_area_count = without_span_gaps
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Prim::Path {
                        fill: Some(_),
                        stroke: None,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(default_area_count, 2, "default fill must split at the gap");

        let json = r#"{"type":"line","data":{"labels":["a","b","c"],
            "datasets":[{"data":[1, null, 3], "fill": true, "spanGaps": true,
            "stepped":"before"}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let area_paths: Vec<_> = scene_for(json)
            .items
            .into_iter()
            .filter_map(|item| match item {
                Prim::Path {
                    d,
                    fill: Some(_),
                    stroke: None,
                    ..
                } => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(area_paths.len(), 1, "spanGaps fill must be one polygon");

        let x0 = common::line_x(&spec, &frame, 0);
        let x2 = common::line_x(&spec, &frame, 2);
        let y0 = frame.ys.map(1.0);
        let y2 = frame.ys.map(3.0);
        let step_edge = format!(
            "M {} {} L {} {} L {} {} ",
            fmt_num(x0),
            fmt_num(y0),
            fmt_num(x2),
            fmt_num(y0),
            fmt_num(x2),
            fmt_num(y2),
        );
        assert!(
            area_paths[0].starts_with(&step_edge),
            "area must use the same step corner as the stroke"
        );
    }

    #[test]
    fn stepped_line_ignores_nonzero_tension_and_uses_a_polyline() {
        let scene = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
                "datasets":[{"data":[1, 2, 3], "tension": 0.8, "stepped": true}]}}"#,
        );
        assert!(
            scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::Polyline { points, .. } if points.len() == 5)),
            "a stepped line must remain a polyline"
        );
        let is_cubic_stroke = |item: &Prim| {
            matches!(
                item,
                Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(_),
                    ..
                } if d.contains(" C ")
            )
        };
        assert!(
            !scene.items.iter().any(is_cubic_stroke),
            "tension must not turn a stepped line into a cubic path"
        );
        let non_stepped_scene = scene_for(
            r#"{"type":"line","data":{"labels":["a","b","c"],
                "datasets":[{"data":[1, 2, 3], "tension": 0.8}]}}"#,
        );
        assert!(
            non_stepped_scene.items.iter().any(is_cubic_stroke),
            "non-stepped tension must retain its cubic path"
        );
    }

    #[test]
    fn empty_step_input_emits_no_vertices() {
        assert_eq!(
            step_points(std::iter::empty(), StepMode::Before),
            Vec::new()
        );
    }

    #[test]
    fn step_capacity_matches_mode_specific_vertex_upper_bound() {
        assert_eq!(step_capacity(0, StepMode::Before), 0);
        assert_eq!(step_capacity(1, StepMode::After), 1);
        assert_eq!(step_capacity(3, StepMode::Before), 5);
        assert_eq!(step_capacity(3, StepMode::After), 5);
        assert_eq!(step_capacity(3, StepMode::Middle), 7);
    }

    #[test]
    fn step_before_and_after_match_chartjs_without_copying_segment_coordinates() {
        let segment = [(1.0, 2.0, 0), (3.0, 4.0, 1)];

        assert_eq!(
            step_points(segment.iter().map(|&(x, y, _)| (x, y)), StepMode::Before),
            vec![(1.0, 2.0), (3.0, 2.0), (3.0, 4.0)]
        );
        assert_eq!(
            step_points(segment.iter().map(|&(x, y, _)| (x, y)), StepMode::After),
            vec![(1.0, 2.0), (1.0, 4.0), (3.0, 4.0)]
        );
    }

    #[test]
    fn area_points_borrow_unstepped_segments() {
        let segment = [(1.0, 2.0, 0), (3.0, 4.0, 1)];

        match area_points(&segment, None) {
            AreaPoints::Borrowed(points) => assert_eq!(points.as_ptr(), segment.as_ptr()),
            AreaPoints::Stepped(_) => panic!("unstepped area points must not be copied"),
        }
        assert!(matches!(
            area_points(&segment, Some(StepMode::Middle)),
            AreaPoints::Stepped(_)
        ));
    }

    fn stacked_area_spec(categories: Vec<&str>, series: Vec<(&str, Vec<f64>)>) -> ChartSpec {
        use crate::ir::{
            AxisBorder, AxisGrid, AxisSpec, ChartKind, Decimation, LegendPos, Point, ScaleKind,
            SizeMode, Theme, XPositions,
        };
        let palette = crate::palette::PALETTE.to_vec();
        let axis = AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None,
            suggested_max: None,
            begin_at_zero: true,
            offset: false,
            grid: AxisGrid::default(),
            border: AxisBorder::default(),
            scale_kind: ScaleKind::Linear,
        };
        ChartSpec {
            kind: ChartKind::Line { stacked: true },
            categories: categories.into_iter().map(str::to_string).collect(),
            x_positions: XPositions::Category,
            series: series
                .into_iter()
                .enumerate()
                .map(|(i, (name, values))| {
                    let color = palette[i % palette.len()];
                    crate::ir::Series {
                        name: name.to_string(),
                        values,
                        points: Vec::<Point>::new(),
                        fill: vec![color],
                        stroke: vec![color],
                        stroke_width: 2.0,
                        area: true,
                        interpolation: crate::ir::LineInterpolation::Linear,
                        span_gaps: false,
                        step_mode: None,
                        series_type: crate::ir::SeriesType::Line,
                        point_radius: None,
                        box_points: vec![],
                        tree: vec![],
                        links: vec![],
                    }
                })
                .collect(),
            x_axis: axis.clone(),
            y_axis: axis,
            legend: LegendPos::None,
            legend_title: None,
            title: None,
            width: 720.0,
            height: 400.0,
            size_mode: SizeMode::Canvas,
            data_labels: false,
            theme: Theme::default(),
            decimation: Decimation::default(),
            radial_axis: None,
        }
    }

    #[test]
    fn stacked_area_bands_are_contiguous() {
        // s1's cat-"b" value is deliberately 12 (not 15): with 15 there, the "far == 15"
        // assertion below would also match series 1's own *raw, unstacked* value at cat "b"
        // by coincidence, so the test would pass even without correct stacking. With 12,
        // no raw value in the fixture equals 15 -- 15 can only appear via correct stacking
        // (s0's 10 + s1's 5 at cat "a") (reviewer-flagged coverage gap, tightened here).
        let spec = stacked_area_spec(
            vec!["a", "b"],
            vec![("s0", vec![10.0, 20.0]), ("s1", vec![5.0, 12.0])],
        );
        let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let scene = build(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
        let markers: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
                _ => None,
            })
            .collect();
        // s0 at cat "a": far = 10 (bottom band). s1 at cat "a": near = 10, far = 15 (top band).
        let s0_a_y = frame.ys.map(10.0);
        let s1_a_y = frame.ys.map(15.0);
        assert!(
            markers.iter().any(|&(_, y)| (y - s0_a_y).abs() < 1e-6),
            "series 0 marker must sit at its cumulative top (10)"
        );
        assert!(
            markers.iter().any(|&(_, y)| (y - s1_a_y).abs() < 1e-6),
            "series 1 marker must sit at its cumulative top (10+5=15)"
        );
    }

    #[test]
    fn stacked_area_top_band_stays_within_plot_bounds() {
        let spec = stacked_area_spec(
            vec!["a", "b", "c"],
            vec![
                ("s0", vec![10.0, 20.0, 30.0]),
                ("s1", vec![5.0, 15.0, 25.0]),
            ],
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);
        let top_ys: Vec<f64> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cy, .. } => Some(*cy),
                _ => None,
            })
            .collect();
        for y in top_ys {
            assert!(
                y >= frame.plot_top - 1e-6 && y <= frame.plot_bottom + 1e-6,
                "marker y={y} escaped plot bounds [{}, {}]",
                frame.plot_top,
                frame.plot_bottom
            );
        }
    }

    #[test]
    fn stacked_area_skips_decimation_above_threshold() {
        // plot_width for an 800px-wide chart is well under 800, so the default decimation
        // threshold (plot_width_px * 4.0) is comfortably under 3200. Use 4000 categories,
        // two series, to force src/layout/decimate.rs::resolve to trigger for a *naive*
        // per-series-independent decimation path, then assert the stack stays aligned
        // everywhere (not just at a hand-picked few indices).
        let n = 4000;
        let categories: Vec<String> = (0..n).map(|i| format!("c{i}")).collect();
        let s0_values: Vec<f64> = (0..n).map(|i| (i % 7) as f64 + 1.0).collect();
        let s1_values: Vec<f64> = (0..n).map(|i| (i % 5) as f64 + 1.0).collect();
        let categories_ref: Vec<&str> = categories.iter().map(String::as_str).collect();
        let spec = stacked_area_spec(
            categories_ref,
            vec![("s0", s0_values.clone()), ("s1", s1_values.clone())],
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);
        let mut markers: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
                _ => None,
            })
            .collect();
        assert_eq!(
            markers.len(),
            n * 2,
            "stacked area must not decimate away any marker (would desync the bands)"
        );
        markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for i in 0..n {
            let expected_s0_far = frame.ys.map(s0_values[i]);
            let expected_s1_far = frame.ys.map(s0_values[i] + s1_values[i]);
            let got: Vec<f64> = markers[i * 2..i * 2 + 2].iter().map(|&(_, y)| y).collect();
            assert!(
                got.iter().any(|&y| (y - expected_s0_far).abs() < 1e-6),
                "category {i}: s0 far offset missing (decimation likely desynced the stack)"
            );
            assert!(
                got.iter().any(|&y| (y - expected_s1_far).abs() < 1e-6),
                "category {i}: s1 far offset missing"
            );
        }
    }

    /// 帯マーカー(far)だけを見るテストは近接辺(near)の閉じ方を検証しない。
    /// (near, far) の設計で load-bearing なのはこの近接辺 close の方(area polygon が
    /// 固定 baseline ではなく下の帯の far offset に閉じること)なので、path data 自体を
    /// 直接検査する。これを外して baseline close に戻しても上のマーカー系テストは
    /// 全て通ってしまう(レビューで指摘・追加)。
    #[test]
    fn stacked_area_polygon_closes_against_near_offset_not_baseline() {
        let spec = stacked_area_spec(
            vec!["a", "b"],
            vec![("s0", vec![10.0, 20.0]), ("s1", vec![5.0, 15.0])],
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);
        let area_paths: Vec<&String> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Path {
                    d,
                    fill: Some(_),
                    stroke: None,
                    ..
                } => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(area_paths.len(), 2, "one area polygon per series");

        let x0 = common::line_x(&spec, &frame, 0);
        let x1 = common::line_x(&spec, &frame, 1);

        // series 0 (bottom band): near is always 0 (nothing stacked beneath it).
        let s0_near_edge = format!(
            "L {} {} L {} {} Z",
            fmt_num(x1),
            fmt_num(frame.ys.map(0.0)),
            fmt_num(x0),
            fmt_num(frame.ys.map(0.0)),
        );
        assert!(
            area_paths[0].ends_with(&s0_near_edge),
            "series 0 near edge must sit at 0: got {}",
            area_paths[0]
        );

        // series 1 (top band): near must equal series 0's far offset per category
        // (10 at "a", 20 at "b") -- NOT the fixed y=0 baseline. Reverting the near-edge
        // close to `baseline_y` would still satisfy every marker-only assertion above,
        // so this is the one check that actually pins the polygon-close behavior.
        let s1_near_edge = format!(
            "L {} {} L {} {} Z",
            fmt_num(x1),
            fmt_num(frame.ys.map(20.0)), // series 0's far at cat "b"
            fmt_num(x0),
            fmt_num(frame.ys.map(10.0)), // series 0's far at cat "a"
        );
        assert!(
            area_paths[1].ends_with(&s1_near_edge),
            "series 1 near edge must equal series 0's far offset per category: got {}",
            area_paths[1]
        );
    }

    /// 実機バグ回帰テスト: `build()` の stacked 経路はカテゴリ全域を走査し、near/far
    /// オフセットは `stack_offsets` の 0 補完で構築される。だがデータラベル描画は
    /// `ser.values[cat]` を直接 index していたため、系列の `values` が `categories` より
    /// 短いと(stacked 系列で 1 系列だけ欠損があるケース)panic していた
    /// (レビューで `categories.len() == 3`・1系列だけ values.len() == 2・data_labels:true
    /// の組み合わせで実測・指摘、修正済み)。
    #[test]
    fn stacked_area_data_labels_do_not_panic_when_series_is_shorter_than_categories() {
        let mut spec = stacked_area_spec(
            vec!["a", "b", "c"],
            vec![("s0", vec![10.0, 20.0]), ("s1", vec![5.0, 15.0, 25.0])],
        );
        spec.data_labels = true;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        // Must not panic: s0 has only 2 values for 3 categories.
        let scene = build(&spec, &m);
        let value_label_count = scene
            .items
            .iter()
            .filter(|item| {
                matches!(item, Prim::Text { anchor: Anchor::Middle, content, .. } if content.parse::<f64>().is_ok())
            })
            .count();
        assert_eq!(
            value_label_count, 6,
            "one data label per series per category (2 series x 3 categories), \
             including the imputed-zero one for s0's missing \"c\" value"
        );
    }

    /// `stack_offsets` は正側 (`pos_running`) / 負側 (`neg_running`) を別配列で追跡する。
    /// これらが独立でなければ(例: 符号に関係なく単一の running total を共有する実装
    /// バグがあれば)、負専用系列の near/far に正系列の累計が漏れ込む。ここでは正専用
    /// 系列 1 本・負専用系列 1 本という最小構成でその漏れがないことを直接検証する
    /// (レビューで指摘されたカバレッジ欠落: Task 2 の domain テストは負値を扱うが、
    /// 軸ドメイン計算のみで、この幾何 (near/far) は未検証だった)。
    #[test]
    fn stacked_area_negative_series_stacks_independently_of_positive_series() {
        let spec = stacked_area_spec(
            vec!["a", "b"],
            vec![("pos", vec![10.0, 20.0]), ("neg", vec![-5.0, -8.0])],
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);

        // far (マーカー位置): 負系列は唯一の負系列なので、自身の累計 = 生値そのもの
        // (-5, -8) になるはず。正系列の累計 (10, 20) が漏れ込めば値がずれる。
        let markers: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
                _ => None,
            })
            .collect();
        let neg_far_a = frame.ys.map(-5.0);
        let neg_far_b = frame.ys.map(-8.0);
        assert!(
            markers.iter().any(|&(_, y)| (y - neg_far_a).abs() < 1e-6),
            "negative series far offset at cat a must be its own cumulative (-5), \
             not contaminated by the positive series' running total"
        );
        assert!(
            markers.iter().any(|&(_, y)| (y - neg_far_b).abs() < 1e-6),
            "negative series far offset at cat b must be its own cumulative (-8)"
        );

        // near (area polygon の閉じ辺): 負側で最初(かつ唯一)の系列なので、下に何も
        // 積まれておらず常に 0 のはず。pos_running が漏れ込めば 10 / 20 になってしまう。
        let area_paths: Vec<&String> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Path {
                    d,
                    fill: Some(_),
                    stroke: None,
                    ..
                } => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(area_paths.len(), 2, "one area polygon per series");

        let x0 = common::line_x(&spec, &frame, 0);
        let x1 = common::line_x(&spec, &frame, 1);
        let neg_near_edge = format!(
            "L {} {} L {} {} Z",
            fmt_num(x1),
            fmt_num(frame.ys.map(0.0)),
            fmt_num(x0),
            fmt_num(frame.ys.map(0.0)),
        );
        assert!(
            area_paths[1].ends_with(&neg_near_edge),
            "negative series near edge must be 0, not leaked from the positive \
             series' running total: got {}",
            area_paths[1]
        );
    }

    /// `line_points()` の stacked 分岐に `stacked: true` を実際に通すテストが存在しな
    /// かったため、`build()` とのガード不一致(Issue 2)が見過ごされていた(レビュー指摘)。
    /// far offset を描画していること、かつ非有限値があっても `build()` と同じ点数を
    /// 保つこと(stack_offsets の 0 補完に合わせ skip しない)の両方を確認する。
    #[test]
    fn line_points_matches_build_when_stacked_including_nan_points() {
        let spec = stacked_area_spec(
            vec!["a", "b", "c"],
            vec![
                ("s0", vec![10.0, f64::NAN, 30.0]),
                ("s1", vec![5.0, 15.0, 25.0]),
            ],
        );
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let pts = line_points(&spec, &frame);
        let scene = build(&spec, &m);
        let marker_count = scene
            .items
            .iter()
            .filter(|item| matches!(item, Prim::Circle { .. }))
            .count();

        assert_eq!(
            pts.len(),
            6,
            "2 series x 3 categories, no points dropped even with a NaN in series 0"
        );
        assert_eq!(
            pts.len(),
            marker_count,
            "line_points() must match build()'s marker count for the same stacked spec"
        );

        // series 1(欠損なし)の far: cat "b" は s0 の NaN が 0 補完されるので 0 + 15 = 15。
        let s1_b = pts
            .iter()
            .find(|p| p.series == 1 && p.index == 1)
            .expect("series 1 cat b point must exist");
        let expected_far = frame.ys.map(15.0);
        assert!(
            (s1_b.cy - expected_far).abs() < 1e-9,
            "line_points must plot the far (cumulative) offset when stacked: got {}, expected {}",
            s1_b.cy,
            expected_far
        );
    }

    /// 実機バグ回帰テスト: 積み上げ area のデータラベルは(修正前)各系列自身の far
    /// オフセットのすぐ上(`y - marker_r - LABEL_GAP`)に置かれており、最上位以外の
    /// 系列では真上に積まれた帯の内部(または境界)に描かれてしまっていた
    /// (bar.rs の stacked_data_label_midpoint_* と同じ規約を line.rs にも適用する、
    /// Task 3 の2回目コードレビュー指摘の carry-forward)。修正後は各系列が自帯の
    /// ピクセル空間中点にラベルを置くため、下位系列のラベルは上位系列の帯
    /// (near..far のピクセル範囲)に収まらないはず。
    #[test]
    fn stacked_data_label_sits_at_band_midpoint_not_inside_the_series_above() {
        let mut spec = stacked_area_spec(vec!["a"], vec![("s0", vec![10.0]), ("s1", vec![20.0])]);
        spec.data_labels = true;
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);

        let labels: Vec<(f64, f64)> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Prim::Text {
                    anchor: Anchor::Middle,
                    x,
                    y,
                    content,
                    ..
                } if content.parse::<f64>().is_ok() => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), 2, "1 label per series");

        // s0 (bottom band): near=0, far=10 -> pixel-space midpoint of map(0)/map(10).
        let s0_expected_y = (frame.ys.map(0.0) + frame.ys.map(10.0)) / 2.0;
        // s1 (top band, stacked on s0): near=10, far=30 -> pixel-space midpoint of
        // map(10)/map(30).
        let s1_expected_y = (frame.ys.map(10.0) + frame.ys.map(30.0)) / 2.0;

        let s0_label_y = labels
            .iter()
            .map(|&(_, y)| y)
            .find(|&y| (y - s0_expected_y).abs() < 1e-6)
            .unwrap_or_else(|| {
                panic!("no label at s0's expected band midpoint {s0_expected_y}: got {labels:?}")
            });
        let s1_label_y = labels
            .iter()
            .map(|&(_, y)| y)
            .find(|&y| (y - s1_expected_y).abs() < 1e-6)
            .unwrap_or_else(|| {
                panic!("no label at s1's expected band midpoint {s1_expected_y}: got {labels:?}")
            });

        // s1's band spans the pixel range [map(30), map(10)] (larger value -> smaller y).
        let s1_band_top = frame.ys.map(30.0).min(frame.ys.map(10.0));
        let s1_band_bottom = frame.ys.map(30.0).max(frame.ys.map(10.0));
        assert!(
            s0_label_y < s1_band_top || s0_label_y > s1_band_bottom,
            "s0's label (y={s0_label_y}) must not collide with s1's band \
             [{s1_band_top}, {s1_band_bottom}] stacked directly above it"
        );
        // Sanity: s1's own label must sit within its own band (it should, by construction,
        // since it's the pixel-space midpoint of that band's near/far).
        assert!(
            s1_label_y >= s1_band_top - 1e-6 && s1_label_y <= s1_band_bottom + 1e-6,
            "s1's label (y={s1_label_y}) should sit within its own band \
             [{s1_band_top}, {s1_band_bottom}]"
        );

        // Old buggy behavior for s0: y=map(10) (s0's far) minus marker radius/gap, which
        // sits inside s1's band (s1's near == s0's far == map(10)). Assert we did not
        // regress to that.
        let s0_buggy_y = frame.ys.map(10.0) - MARKER_R - common::LABEL_GAP;
        assert!(
            (s0_label_y - s0_buggy_y).abs() > 1.0,
            "s0's label must not sit at the old far-edge-minus-gap position: \
             s0_label_y={s0_label_y}, s0_buggy_y={s0_buggy_y}"
        );
    }

    #[test]
    fn offset_line_labels_align_to_band_centers() {
        // draw_frame の x ラベルも offset:true では band 中心(line_x ではなく category_center)。
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["a","b","c"],
               "datasets":[{"data":[10,20,30]}]},
               "options":{"scales":{"x":{"offset":true}}}}"#,
            false,
        )
        .unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = common::compute(&spec, &m);
        let scene = build(&spec, &m);
        // title/legend なし → anchor=Middle の Text は x カテゴリラベルのみ。
        let label_xs: Vec<f64> = scene
            .items
            .iter()
            .filter_map(|p| match p {
                Prim::Text {
                    x,
                    anchor: Anchor::Middle,
                    ..
                } => Some(*x),
                _ => None,
            })
            .collect();
        assert_eq!(label_xs.len(), 3, "x ラベルは 3 個");
        let band_w = (frame.plot_right - frame.plot_left) / 3.0;
        for (i, &x) in label_xs.iter().enumerate() {
            let expect = frame.plot_left + (i as f64 + 0.5) * band_w;
            assert!(
                (x - expect).abs() < 1e-9,
                "offset ラベルは band 中心: i={i} x={x} expect={expect}"
            );
        }
    }
}
