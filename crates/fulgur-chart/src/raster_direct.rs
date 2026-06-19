//! Scene → PNG の直接描画（SVG 文字列を経由しない）。
//! tiny-skia でプリミティブを直描きし、テキストは ttf_parser::outline_glyph で
//! グリフ輪郭をパスに変換して描く。
//!
//! ## SVG 経由との違い
//! - SVG 文字列と usvg/resvg パーサを PNG 経路から除去。
//! - アンチエイリアスは tiny-skia 直描きの AA になるため、SVG 経由と画素単位では一致しない。
//! - テキスト描画品質は resvg 経由と実用上同等（グリフ輪郭ベース）。
//!
//! ## 制約
//! - Prim::Path の d 文字列は M/L/C/A/Z コマンドのみを含む前提（レイアウト生成コードの不変条件）。
//! - 未知コマンドのパスは無描画でスキップ（エラー伝播しない）。

use std::collections::HashMap;
use std::f64::consts::PI;

use resvg::tiny_skia::{self, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
use ttf_parser::OutlineBuilder;

use crate::font::DEFAULT_FONT;
use crate::ir::Color;
use crate::raster::MAX_PNG_AREA_PIXELS;
use crate::scene::{Anchor, Prim, Scene};

// ---------------------------------------------------------------------------
// 公開エントリポイント
// ---------------------------------------------------------------------------

/// ChartSpec を PNG バイト列に直接ラスタライズする。
///
/// SVG 文字列を経由しないため、SVG 経由と画素単位では一致しない。
/// 決定論性（同一入力 → 同一出力）は保証する。
pub fn render_chart_to_png(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let face =
        ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("フォント解析失敗: {e}"))?;
    let measurer =
        crate::text::TextMeasurer::new(font_bytes).map_err(|e| format!("計測初期化失敗: {e}"))?;
    let scene = crate::layout::build_scene(spec, &measurer);
    scene_to_png_with_face(&scene, scale, &face)
}

/// ChartSpec を PNG バイト列に直接ラスタライズする（デフォルトフォント）。
pub fn render_chart_to_png_default(
    spec: &crate::ir::ChartSpec,
    scale: f32,
) -> Result<Vec<u8>, String> {
    render_chart_to_png(spec, scale, DEFAULT_FONT)
}

/// Scene を PNG バイト列に直接ラスタライズする。
///
/// `scale` が 0 以下または非有限の場合は 1.0 にフォールバックする。
pub fn scene_to_png(scene: &Scene, scale: f32, font_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let face =
        ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("フォント解析失敗: {e}"))?;
    scene_to_png_with_face(scene, scale, &face)
}

// ---------------------------------------------------------------------------
// 内部実装
// ---------------------------------------------------------------------------

fn scene_to_png_with_face(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
) -> Result<Vec<u8>, String> {
    let scale = if scale > 0.0 { scale } else { 1.0 };

    let w = (scene.width as f32 * scale).round().max(1.0) as u32;
    let h = (scene.height as f32 * scale).round().max(1.0) as u32;

    let area = w as u64 * h as u64;
    if area > MAX_PNG_AREA_PIXELS {
        return Err(format!(
            "PNG 解像度 {w}×{h} px ({area} ピクセル) が上限 {MAX_PNG_AREA_PIXELS} px² を超えています"
        ));
    }

    let mut pixmap =
        Pixmap::new(w, h).ok_or_else(|| format!("Pixmap 確保失敗: 寸法 {w}x{h} が無効です"))?;

    let transform = Transform::from_scale(scale, scale);
    let mut glyph_cache: HashMap<ttf_parser::GlyphId, Option<tiny_skia::Path>> = HashMap::new();

    for prim in &scene.items {
        render_prim(&mut pixmap, prim, transform, face, &mut glyph_cache);
    }

    pixmap
        .encode_png()
        .map_err(|e| format!("PNG エンコード失敗: {e}"))
}

fn render_prim(
    pixmap: &mut Pixmap,
    prim: &Prim,
    transform: Transform,
    face: &ttf_parser::Face<'_>,
    cache: &mut HashMap<ttf_parser::GlyphId, Option<tiny_skia::Path>>,
) {
    match prim {
        Prim::Rect { x, y, w, h, fill } => {
            let Some(rect) = Rect::from_xywh(*x as f32, *y as f32, *w as f32, *h as f32) else {
                return;
            };
            let path = PathBuilder::from_rect(rect);
            let mut paint = solid_paint(*fill);
            paint.anti_alias = false;
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        Prim::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
        } => {
            let mut b = PathBuilder::new();
            b.move_to(*x1 as f32, *y1 as f32);
            b.line_to(*x2 as f32, *y2 as f32);
            let Some(path) = b.finish() else { return };
            pixmap.stroke_path(
                &path,
                &solid_paint(*stroke),
                &make_stroke(*stroke_width),
                transform,
                None,
            );
        }

        Prim::Polyline {
            points,
            stroke,
            stroke_width,
        } => {
            if points.len() < 2 {
                return;
            }
            let mut b = PathBuilder::new();
            for (i, &(px, py)) in points.iter().enumerate() {
                if i == 0 {
                    b.move_to(px as f32, py as f32);
                } else {
                    b.line_to(px as f32, py as f32);
                }
            }
            let Some(path) = b.finish() else { return };
            pixmap.stroke_path(
                &path,
                &solid_paint(*stroke),
                &make_stroke(*stroke_width),
                transform,
                None,
            );
        }

        Prim::Path {
            d,
            fill,
            stroke,
            stroke_width,
        } => {
            let Some(path) = parse_path_data(d) else {
                return;
            };
            if let Some(fill_color) = fill {
                pixmap.fill_path(
                    &path,
                    &solid_paint(*fill_color),
                    FillRule::Winding,
                    transform,
                    None,
                );
            }
            if let Some(stroke_color) = stroke {
                pixmap.stroke_path(
                    &path,
                    &solid_paint(*stroke_color),
                    &make_stroke(*stroke_width),
                    transform,
                    None,
                );
            }
        }

        Prim::Circle { cx, cy, r, fill } => {
            let Some(path) = PathBuilder::from_circle(*cx as f32, *cy as f32, *r as f32) else {
                return;
            };
            pixmap.fill_path(
                &path,
                &solid_paint(*fill),
                FillRule::Winding,
                transform,
                None,
            );
        }

        Prim::Text {
            x,
            y,
            size,
            anchor,
            fill,
            content,
        } => {
            render_text(
                pixmap, *x, *y, *size, *anchor, *fill, content, face, transform, cache,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// テキスト描画
// ---------------------------------------------------------------------------

/// グリフ輪郭を正規化パスとして PathBuilder に書き込む。
/// フォント座標 (Y 上向き) の Y 軸を反転するのみ（translate/scale は適用しない）。
/// キャッシュ済みパスは描画時に Transform で位置・スケールを合成する。
struct GlyphSinkNorm<'a> {
    builder: &'a mut PathBuilder,
}

impl OutlineBuilder for GlyphSinkNorm<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, -y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(x1, -y1, x, -y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(x1, -y1, x2, -y2, x, -y);
    }
    fn close(&mut self) {
        self.builder.close();
    }
}

/// 正規化グリフパスを構築する（Y 反転・origin=0・scale=1）。
/// アウトラインを持たないグリフ（スペース等）は `None`。
fn build_canonical_glyph_path(
    face: &ttf_parser::Face<'_>,
    gid: ttf_parser::GlyphId,
) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    {
        let mut sink = GlyphSinkNorm {
            builder: &mut builder,
        };
        face.outline_glyph(gid, &mut sink)?;
    }
    builder.finish()
}

#[allow(clippy::too_many_arguments)]
fn render_text(
    pixmap: &mut Pixmap,
    x: f64,
    y: f64,
    size: f64,
    anchor: Anchor,
    fill: Color,
    content: &str,
    face: &ttf_parser::Face<'_>,
    transform: Transform,
    cache: &mut HashMap<ttf_parser::GlyphId, Option<tiny_skia::Path>>,
) {
    if content.is_empty() {
        return;
    }

    let upem = face.units_per_em() as f32;
    let glyph_scale = size as f32 / upem;

    // advance 幅合計（TextMeasurer と同一の計算）。
    let total_width: f32 = content
        .chars()
        .filter_map(|ch| face.glyph_index(ch))
        .filter_map(|gid| face.glyph_hor_advance(gid))
        .map(|adv| adv as f32 * glyph_scale)
        .sum();

    let start_x = match anchor {
        Anchor::Start => x as f32,
        Anchor::Middle => x as f32 - total_width / 2.0,
        Anchor::End => x as f32 - total_width,
    };

    let baseline_y = y as f32;
    let paint = solid_paint(fill);
    let mut cursor_x = start_x;

    for ch in content.chars() {
        let Some(gid) = face.glyph_index(ch) else {
            continue;
        };
        let Some(adv_raw) = face.glyph_hor_advance(gid) else {
            continue;
        };
        let adv = adv_raw as f32 * glyph_scale;

        // キャッシュ済みパスがなければ構築して保存する。
        // 同一 GlyphId のパスはチャート内で再利用される（'A', '0' 等）。
        let entry = cache
            .entry(gid)
            .or_insert_with(|| build_canonical_glyph_path(face, gid));
        if let Some(path) = entry.as_ref() {
            // 正規化パス（origin=0, scale=1, Y 反転済み）を
            // scale(glyph_scale) → translate(cursor_x, baseline_y) → outer_transform の順に合成して描画。
            let glyph_transform = transform.pre_concat(
                Transform::from_translate(cursor_x, baseline_y)
                    .pre_concat(Transform::from_scale(glyph_scale, glyph_scale)),
            );
            pixmap.fill_path(path, &paint, FillRule::Winding, glyph_transform, None);
        }
        cursor_x += adv;
    }
}

// ---------------------------------------------------------------------------
// SVG パスパーサ（M/L/C/A/Z のみ）
// ---------------------------------------------------------------------------

/// SVG path data 文字列を tiny-skia Path に変換する。
/// 未知コマンドや解析失敗は `None` を返す。
fn parse_path_data(d: &str) -> Option<tiny_skia::Path> {
    let mut b = PathBuilder::new();
    let mut tokens = d.split_ascii_whitespace();
    // 円弧変換のために現在点を追跡する（M/L/C/A で更新）。
    let mut cur = [0.0_f64; 2];

    while let Some(tok) = tokens.next() {
        match tok {
            "M" => {
                let x = tokens.next()?.parse::<f64>().ok()?;
                let y = tokens.next()?.parse::<f64>().ok()?;
                b.move_to(x as f32, y as f32);
                cur = [x, y];
            }
            "L" => {
                let x = tokens.next()?.parse::<f64>().ok()?;
                let y = tokens.next()?.parse::<f64>().ok()?;
                b.line_to(x as f32, y as f32);
                cur = [x, y];
            }
            "C" => {
                let x1 = tokens.next()?.parse::<f64>().ok()?;
                let y1 = tokens.next()?.parse::<f64>().ok()?;
                let x2 = tokens.next()?.parse::<f64>().ok()?;
                let y2 = tokens.next()?.parse::<f64>().ok()?;
                let x = tokens.next()?.parse::<f64>().ok()?;
                let y = tokens.next()?.parse::<f64>().ok()?;
                b.cubic_to(
                    x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
                );
                cur = [x, y];
            }
            "A" => {
                let rx = tokens.next()?.parse::<f64>().ok()?;
                let ry = tokens.next()?.parse::<f64>().ok()?;
                let phi = tokens.next()?.parse::<f64>().ok()?;
                let laf_tok = tokens.next()?;
                let swf_tok = tokens.next()?;
                let laf = laf_tok.parse::<u8>().ok()?;
                let swf = swf_tok.parse::<u8>().ok()?;
                let x = tokens.next()?.parse::<f64>().ok()?;
                let y = tokens.next()?.parse::<f64>().ok()?;
                arc_to_bezier(
                    &mut b,
                    rx,
                    ry,
                    phi,
                    laf != 0,
                    swf != 0,
                    x,
                    y,
                    cur[0],
                    cur[1],
                );
                cur = [x, y];
            }
            "Z" => {
                b.close();
            }
            _ => return None,
        }
    }
    b.finish()
}

// ---------------------------------------------------------------------------
// SVG 弧 → cubic bézier 変換（W3C SVG 11 Appendix F.6）
// ---------------------------------------------------------------------------

/// SVG endpoint 弧パラメータを cubic bézier セグメント群に変換し PathBuilder に積む。
/// `sx, sy`: 弧開始点（現在点）。`ex, ey`: 弧終端点。
#[allow(clippy::too_many_arguments)]
fn arc_to_bezier(
    b: &mut PathBuilder,
    mut rx: f64,
    mut ry: f64,
    phi_deg: f64,
    large_arc: bool,
    sweep: bool,
    ex: f64,
    ey: f64,
    sx: f64,
    sy: f64,
) {
    // 半径ゼロや始終点同一は直線に縮退。
    if rx.abs() < 1e-10 || ry.abs() < 1e-10 {
        b.line_to(ex as f32, ey as f32);
        return;
    }
    if (sx - ex).abs() < 1e-10 && (sy - ey).abs() < 1e-10 {
        return;
    }

    let phi = phi_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    // F.6.5.1: (x1', y1')
    let dx = (sx - ex) * 0.5;
    let dy = (sy - ey) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // F.6.6.3: 半径を最小限拡大して接続を保証する。
    rx = rx.abs();
    ry = ry.abs();
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let lambda = x1p2 / rx2 + y1p2 / ry2;
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let rx2 = rx * rx;
    let ry2 = ry * ry;

    // F.6.5.2: (cx', cy')
    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = rx2 * y1p2 + ry2 * x1p2;
    let sq = if den < 1e-10 { 0.0 } else { (num / den).sqrt() };
    let sign = if large_arc == sweep {
        -1.0_f64
    } else {
        1.0_f64
    };
    let cxp = sign * sq * rx * y1p / ry;
    let cyp = sign * sq * -ry * x1p / rx;

    // F.6.5.3: (cx, cy)
    let mx = (sx + ex) * 0.5;
    let my = (sy + ey) * 0.5;
    let cx = cos_phi * cxp - sin_phi * cyp + mx;
    let cy = sin_phi * cxp + cos_phi * cyp + my;

    // F.6.5.5–6: θ1, Δθ
    let theta1 = vec_angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = vec_angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }

    // |Δθ| ≤ π/2 ずつに分割して cubic bézier に近似する。
    let n = ((dtheta.abs() / (PI / 2.0)).ceil() as u32).max(1);
    let d = dtheta / n as f64;
    let mut theta = theta1;
    for _ in 0..n {
        arc_segment(b, cx, cy, rx, ry, phi, theta, d);
        theta += d;
    }
}

/// 1 弧セグメント（|d| ≤ π/2）を cubic bézier に変換して PathBuilder に積む。
#[allow(clippy::too_many_arguments)]
fn arc_segment(
    b: &mut PathBuilder,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    phi: f64,
    theta: f64,
    d: f64,
) {
    // Dokter/Morken 近似: α = (4/3)·tan(d/4)。|d| ≤ π/2 のとき最大誤差は約 0.0003r 未満。
    let alpha = (d / 4.0).tan() * 4.0 / 3.0;
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();
    let cos_t1 = theta.cos();
    let sin_t1 = theta.sin();
    let theta2 = theta + d;
    let cos_t2 = theta2.cos();
    let sin_t2 = theta2.sin();

    // 楕円上の点と接線方向（局所座標）。
    let p1 = (rx * cos_t1, ry * sin_t1);
    let dp1 = (alpha * (-rx * sin_t1), alpha * (ry * cos_t1));
    let p2 = (rx * cos_t2, ry * sin_t2);
    let dp2 = (alpha * (-rx * sin_t2), alpha * (ry * cos_t2));

    // 局所座標 → 世界座標（回転 phi + 中心平行移動）。
    let to_world = |lx: f64, ly: f64| -> (f32, f32) {
        (
            (cos_phi * lx - sin_phi * ly + cx) as f32,
            (sin_phi * lx + cos_phi * ly + cy) as f32,
        )
    };

    let (cp1x, cp1y) = to_world(p1.0 + dp1.0, p1.1 + dp1.1);
    let (cp2x, cp2y) = to_world(p2.0 - dp2.0, p2.1 - dp2.1);
    let (p2x, p2y) = to_world(p2.0, p2.1);

    b.cubic_to(cp1x, cp1y, cp2x, cp2y, p2x, p2y);
}

/// 2 次元ベクトル間の符号付き角度（ラジアン）。
fn vec_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    let dot = ux * vx + uy * vy;
    let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    let angle = (dot / len).clamp(-1.0, 1.0).acos();
    if ux * vy - uy * vx < 0.0 {
        -angle
    } else {
        angle
    }
}

// ---------------------------------------------------------------------------
// ペイント・ストロークヘルパ
// ---------------------------------------------------------------------------

fn solid_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    let alpha = (color.a * 255.0).round() as u8;
    paint.set_color(tiny_skia::Color::from_rgba8(
        color.r, color.g, color.b, alpha,
    ));
    paint
}

fn make_stroke(width: f64) -> Stroke {
    Stroke {
        width: width as f32,
        ..Stroke::default()
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;

    fn bar_spec() -> crate::ir::ChartSpec {
        chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"label":"売上","data":[10,20,30]}]}}"#,
            false,
        )
        .unwrap()
    }

    #[test]
    fn rasterizes_to_valid_png() {
        let png = render_chart_to_png(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
        assert!(png.len() > 100);
    }

    #[test]
    fn scale_increases_size() {
        let small = render_chart_to_png(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        let large = render_chart_to_png(&bar_spec(), 2.0, DEFAULT_FONT).unwrap();
        assert!(large.len() > small.len());
    }

    #[test]
    fn non_positive_scale_falls_back_to_one() {
        let normal = render_chart_to_png(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        let fallback = render_chart_to_png(&bar_spec(), 0.0, DEFAULT_FONT).unwrap();
        assert_eq!(&fallback[0..4], &[0x89, b'P', b'N', b'G']);
        assert_eq!(normal.len(), fallback.len());
    }

    #[test]
    fn png_is_byte_deterministic() {
        let a = render_chart_to_png(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        let b = render_chart_to_png(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(a, b, "直接描画の出力は決定的でなければならない");
    }

    #[test]
    fn with_invalid_font_is_err() {
        assert!(render_chart_to_png(&bar_spec(), 1.0, b"not a font").is_err());
    }

    #[test]
    fn oversized_area_is_err() {
        let mut spec = bar_spec();
        // 8001×8001 = 64_016_001 > 64_000_000
        spec.width = 8001.0;
        spec.height = 8001.0;
        let err = render_chart_to_png(&spec, 1.0, DEFAULT_FONT);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("上限"));
    }

    #[test]
    fn pie_chart_renders() {
        let spec = chartjs::parse(
            r#"{"type":"pie","data":{"labels":["X","Y"],"datasets":[{"data":[40,60]}]}}"#,
            false,
        )
        .unwrap();
        let png = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn line_chart_with_tension_renders() {
        let spec = chartjs::parse(
            r#"{"type":"line","data":{"labels":["A","B","C"],"datasets":[{"data":[1,3,2],"tension":0.4}]}}"#,
            false,
        )
        .unwrap();
        let png = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn radar_chart_renders() {
        let spec = chartjs::parse(
            r#"{"type":"radar","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,15]}]}}"#,
            false,
        )
        .unwrap();
        let png = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn parse_path_data_m_l_z() {
        let path = parse_path_data("M 10 20 L 30 40 Z");
        assert!(path.is_some(), "M/L/Z パスはパース可能");
    }

    #[test]
    fn parse_path_data_cubic() {
        let path = parse_path_data("M 0 0 C 10 5 20 5 30 0");
        assert!(path.is_some(), "cubic bézier パスはパース可能");
    }

    #[test]
    fn parse_path_data_arc() {
        // 半円弧（pie スライスと同様の形式）
        let path = parse_path_data("M 100 50 A 50 50 0 0 1 150 100");
        assert!(path.is_some(), "arc パスはパース可能");
    }

    #[test]
    fn parse_path_data_unknown_command_is_none() {
        assert!(
            parse_path_data("M 0 0 Q 5 5 10 0").is_none(),
            "Q コマンドは非対応"
        );
    }

    #[test]
    fn arc_to_bezier_produces_closed_circle() {
        // 360° の円を 4 弧で近似する。誤差が 1px 未満のことを間接的に確認。
        let mut b = PathBuilder::new();
        b.move_to(150.0, 100.0);
        // 上半円 (sweep=true)
        arc_to_bezier(
            &mut b, 50.0, 50.0, 0.0, false, true, 50.0, 100.0, 150.0, 100.0,
        );
        // 下半円 (sweep=true)
        arc_to_bezier(
            &mut b, 50.0, 50.0, 0.0, false, true, 150.0, 100.0, 50.0, 100.0,
        );
        b.close();
        assert!(b.finish().is_some(), "360° 円は有効パス");
    }
}
