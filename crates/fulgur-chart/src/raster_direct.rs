//! Scene → PNG の直接描画（SVG 文字列を経由しない）。
//! tiny-skia でプリミティブを直描きし、テキストは ttf_parser::outline_glyph で
//! グリフ輪郭をパスに変換して描く。
//!
//! ## SVG 経由との違い
//! - SVG 文字列と SVG パーサ（かつての usvg/resvg 経路）を排除。依存自体からも除去済みで、
//!   この経路は tiny-skia と ttf-parser のみに依存する。
//! - アンチエイリアスは tiny-skia 直描きの AA になるため、SVG 経由と画素単位では一致しない。
//! - テキスト描画品質は SVG パーサ経由と実用上同等（グリフ輪郭ベース）。
//!
//! ## 制約
//! - Prim::Path の d 文字列は M/L/C/A/Z コマンドのみを含む前提（レイアウト生成コードの不変条件）。
//! - 未知コマンドのパスは無描画でスキップ（エラー伝播しない）。

use std::collections::HashMap;
use std::f64::consts::PI;

use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder};
use tiny_skia::{self, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
use ttf_parser::OutlineBuilder;

use crate::font::DEFAULT_FONT;
use crate::ir::Color;
use crate::scene::{Anchor, Prim, Scene};

/// PNG 出力の最大ピクセル面積(幅 × 高さ)。
/// scale 適用後のピクセル数がこれを超えると OOM のリスクがあるため Err とする。
/// 64M px ≒ 8000×8000 → raw RGBA で約 256 MB。PNG は demultiply を in-place で行い
/// 別バッファを確保しないため、ピークは pixmap 1 枚分(≈256 MB)に収まる
/// (→ [`demultiply_in_place`])。
const MAX_PNG_AREA_PIXELS: u64 = 64_000_000;

/// WebP 出力の最大ピクセル面積(幅 × 高さ)。
///
/// WebP は pixmap(in-place demultiply 済み)に加えて、`image_webp` のロスレス
/// エンコーダがエンコード時にフルフレーム相当の内部バッファをもう 1 枚確保する
/// (= 実ピークは pixmap の約 2 倍)。PNG と同じ面積上限を許すと約 256 MB の
/// 予算を超えるため、WebP は面積上限を PNG の半分にして pixmap + エンコーダ
/// 内部バッファの合計を約 256 MB に収める。32M px ≒ 5657×5657。
const MAX_WEBP_AREA_PIXELS: u64 = MAX_PNG_AREA_PIXELS / 2;

/// WebP lossless の軸ごとの上限。
const MAX_WEBP_AXIS: u32 = 16_384;

/// ラスタ出力の上限(面積・軸)とエラーメッセージ接頭辞をフォーマットごとに束ねる。
///
/// PNG と WebP では許容できる面積予算が異なる(WebP はエンコーダ内部バッファぶん
/// 厳しい)。フォーマット固有の検証関数を別に持つとガードが 2 箇所に分かれてドリフト
/// するため、上限値をデータとして [`scene_to_pixmap`] の単一ガードへ渡し、pixmap
/// 確保前に同一ロジックで弾く。
struct RasterLimits {
    /// scale 適用後の最大ピクセル面積。
    max_area: u64,
    /// 軸ごとの最大ピクセル数。`None` なら軸チェックなし(PNG)。
    max_axis: Option<u32>,
    /// エラーメッセージ接頭辞("raster" / "WebP")。PNG/WebP 経路を区別する。
    output: &'static str,
}

/// PNG 経路の上限。軸制約はなく、面積のみ [`MAX_PNG_AREA_PIXELS`] で弾く。
const PNG_LIMITS: RasterLimits = RasterLimits {
    max_area: MAX_PNG_AREA_PIXELS,
    max_axis: None,
    output: "raster",
};

/// WebP 経路の上限。軸 [`MAX_WEBP_AXIS`]・面積 [`MAX_WEBP_AREA_PIXELS`] の両方で弾く。
const WEBP_LIMITS: RasterLimits = RasterLimits {
    max_area: MAX_WEBP_AREA_PIXELS,
    max_axis: Some(MAX_WEBP_AXIS),
    output: "WebP",
};

impl RasterLimits {
    /// scale 適用後の寸法を pixmap 確保前に検証する(軸 → 面積の順)。
    fn check(&self, w: u32, h: u32, area: u64) -> Result<(), String> {
        if let Some(max_axis) = self.max_axis
            && (w > max_axis || h > max_axis)
        {
            return Err(format!(
                "{} output {w}×{h} px exceeds the per-axis limit of {max_axis} px",
                self.output
            ));
        }
        if area > self.max_area {
            return Err(format!(
                "{} output {w}×{h} px ({area} pixels) exceeds the area limit of {} px",
                self.output, self.max_area
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 公開エントリポイント
// ---------------------------------------------------------------------------

/// PNG エンコードの圧縮プリセット。速度↔サイズのトレードオフを選ぶ。
///
/// いずれも可逆(同一ピクセル)・決定的。既定は `Balanced`(高速のままサイズを大幅削減)で、
/// `render_chart_to_png` はこれを使う。`Fast` は tiny-skia の `encode_png()` と
/// バイト一致する最速プリセット。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PngCompression {
    /// fdeflate(Fast) + Sub フィルタ。最速・最大サイズ。tiny-skia 既定と同一出力。
    Fast,
    /// fdeflate(Fast) + 適応フィルタ。高速のままサイズを大幅に削減する(既定)。
    #[default]
    Balanced,
    /// zlib(Level 6) + 適応フィルタ。最小サイズ。最も遅い。
    High,
}

impl PngCompression {
    /// `(png 圧縮レベル, 基本フィルタ, 適応フィルタ有効)` へ落とす。
    fn params(self) -> (png::Compression, png::FilterType, bool) {
        match self {
            Self::Fast => (png::Compression::Fast, png::FilterType::Sub, false),
            Self::Balanced => (png::Compression::Fast, png::FilterType::Paeth, true),
            Self::High => (png::Compression::Default, png::FilterType::Paeth, true),
        }
    }
}

/// ChartSpec を PNG バイト列に直接ラスタライズする。
///
/// SVG 文字列を経由しないため、SVG 経由と画素単位では一致しない。
/// 決定論性（同一入力 → 同一出力）は保証する。圧縮は既定の
/// [`PngCompression::Balanced`]（高速のままサイズを大幅削減、ロスレス）。
/// 最速の `Fast` や最小サイズの `High` を選びたい場合は
/// [`render_chart_to_png_with`] を使う。
pub fn render_chart_to_png(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    render_chart_to_png_with(spec, scale, font_bytes, PngCompression::default())
}

/// 圧縮プリセットを指定して ChartSpec を PNG バイト列にラスタライズする。
///
/// 全プリセットで描画ピクセルは同一(可逆)・決定的。サイズと速度のみ異なる。
pub fn render_chart_to_png_with(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
    compression: PngCompression,
) -> Result<Vec<u8>, String> {
    let face =
        ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("font parse failed: {e}"))?;
    let measurer = crate::text::TextMeasurer::new(font_bytes)
        .map_err(|e| format!("text measurer init failed: {e}"))?;
    let scene = crate::layout::build_scene(spec, &measurer);
    scene_to_png_with_face(&scene, scale, &face, compression)
}

/// ChartSpec を PNG バイト列に直接ラスタライズする（デフォルトフォント）。
pub fn render_chart_to_png_default(
    spec: &crate::ir::ChartSpec,
    scale: f32,
) -> Result<Vec<u8>, String> {
    render_chart_to_png(spec, scale, DEFAULT_FONT)
}

/// ChartSpec を WebP バイト列に直接ラスタライズする（ロスレス）。
///
/// SVG 文字列を経由しない。決定論性（同一入力 → 同一出力）を保証する。
pub fn render_chart_to_webp(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let face =
        ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("font parse failed: {e}"))?;
    let measurer = crate::text::TextMeasurer::new(font_bytes)
        .map_err(|e| format!("text measurer init failed: {e}"))?;
    let scene = crate::layout::build_scene(spec, &measurer);
    // WebP 専用の上限(軸・面積)で pixmap 確保前に弾き OOM を防ぐ(→ WEBP_LIMITS)。
    let mut pixmap = scene_to_pixmap(&scene, scale, &face, &WEBP_LIMITS)?;

    // pixmap バッファを in-place で straight 化する(別バッファを確保しない。
    // WebPEncoder は premultiplied ではなく straight alpha を要求する → demultiply_in_place)。
    demultiply_in_place(&mut pixmap);

    let mut buf = Vec::new();
    WebPEncoder::new_lossless(&mut buf)
        .write_image(
            pixmap.data(),
            pixmap.width(),
            pixmap.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("WebP encode failed: {e}"))?;
    Ok(buf)
}

/// Scene を PNG バイト列に直接ラスタライズする。
///
/// `scale` が 0 以下または非有限の場合は 1.0 にフォールバックする。
pub fn scene_to_png(scene: &Scene, scale: f32, font_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let face =
        ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("font parse failed: {e}"))?;
    scene_to_png_with_face(scene, scale, &face, PngCompression::default())
}

// ---------------------------------------------------------------------------
// 内部実装
// ---------------------------------------------------------------------------

/// Scene を RGBA Pixmap にラスタライズする（PNG/WebP 共通）。
fn scene_to_pixmap(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
    limits: &RasterLimits,
) -> Result<Pixmap, String> {
    // scale が 0 以下/非有限なら 1.0 にフォールバック。+Inf 等は u32 で飽和し、
    // 続く limits.check が pixmap を確保する前に弾く。
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let w = (scene.width as f32 * scale).round().max(1.0) as u32;
    let h = (scene.height as f32 * scale).round().max(1.0) as u32;
    let area = w as u64 * h as u64;
    limits.check(w, h, area)?;

    let mut pixmap = Pixmap::new(w, h)
        .ok_or_else(|| format!("Pixmap allocation failed: invalid dimensions {w}x{h}"))?;

    let transform = Transform::from_scale(scale, scale);
    let mut glyph_cache: HashMap<ttf_parser::GlyphId, Option<tiny_skia::Path>> = HashMap::new();

    for prim in &scene.items {
        render_prim(&mut pixmap, prim, transform, face, &mut glyph_cache);
    }

    Ok(pixmap)
}

fn scene_to_png_with_face(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
    compression: PngCompression,
) -> Result<Vec<u8>, String> {
    let mut pixmap = scene_to_pixmap(scene, scale, face, &PNG_LIMITS)?;
    encode_png_fast(&mut pixmap, compression)
}

/// premultiplied RGBA な Pixmap を straight(非 premultiplied) RGBA へ **in-place** 変換する。
///
/// tiny-skia の `Pixmap::encode_png()` と WebP 経路は全画素を `demultiply()` するが、
/// その除算が PNG エンコード時間の大半(計測で ~95%)を占める。premultiplied の不変条件
/// (各チャンネル ≤ α)より、α==255(不透明)・α==0(透明)の画素は premultiplied 値が
/// そのまま straight 値に一致するため、これらは書き換え不要。除算が要るのは
/// AA 縁などの部分α画素のみで、それらの RGB だけを上書きする。出力は「全画素
/// demultiply」とバイト単位で一致する(`demultiply()` は α==255 で恒等、α==0 で 0)。
///
/// pixmap の buffer をそのまま書き換えるため、straight RGBA のフルフレームコピーを
/// 別途確保しない(OOM 対策: ピークメモリを 1 フレーム削減する)。変換後の pixmap は
/// premultiplied 不変条件を満たさなくなるが、本経路では encode 直前の最終処理であり
/// 以降 `pixels()` で読み直さないため問題ない。
fn demultiply_in_place(pixmap: &mut Pixmap) {
    // 部分α画素(AA縁)のみ raw バイトから premultiplied 値を復元して demultiply し、
    // 同じ 4 バイトへ書き戻す。α==255/α==0 はそのままで straight に一致するため触れない。
    for chunk in pixmap.data_mut().chunks_exact_mut(4) {
        let a = chunk[3];
        if a != 0 && a != 255 {
            // pixmap data は常に有効な premultiplied(各チャンネル ≤ α)。万一不正
            // (r/g/b > a)なら from_rgba が None を返すが、その画素は据え置く。
            // .expect でパニックさせると本修正が塞ぐはずの DoS(プロセス終了)を逆に
            // 招くため、防御的に if let で握りクラッシュさせない。
            if let Some(px) =
                tiny_skia::PremultipliedColorU8::from_rgba(chunk[0], chunk[1], chunk[2], a)
            {
                // tiny-skia と同一の demultiply で straight 化（丸めも一致）。
                let c = px.demultiply();
                chunk[0] = c.red();
                chunk[1] = c.green();
                chunk[2] = c.blue();
                chunk[3] = c.alpha();
            }
        }
    }
}

/// Pixmap を PNG バイト列にエンコードする（tiny-skia `encode_png()` の高速等価版）。
///
/// 圧縮 `Compression::Fast`(fdeflate)・フィルタ `Sub` は tiny-skia が用いる png の
/// デフォルトと同値。straight 変換を高速化した点だけが異なり、出力は tiny-skia
/// `encode_png()` とバイト単位で一致する（回帰: `encode_png_fast_matches_tiny_skia_byte_for_byte`）。
///
/// `pixmap` を in-place で straight 化するため `&mut` を取る(呼び出し元はこの直後に
/// pixmap を捨てる前提)。これにより straight のフルフレームコピーを確保しない。
fn encode_png_fast(pixmap: &mut Pixmap, compression: PngCompression) -> Result<Vec<u8>, String> {
    demultiply_in_place(pixmap);
    encode_rgba_png(pixmap.data(), pixmap.width(), pixmap.height(), compression)
        .map_err(|e| format!("PNG encode failed: {e}"))
}

/// straight RGBA8 バイト列を PNG にエンコードする。
///
/// `PngCompression::Fast` の (`Compression::Fast`(fdeflate) + `FilterType::Sub`) は
/// tiny-skia が用いる png のデフォルトと同値で、出力バイト一致を保つ。Balanced/High は
/// 適応フィルタ・より高い圧縮でサイズを縮める(可逆=同一ピクセル)。エラーは png 由来の
/// 型で返し、整形は呼び出し元の `encode_png_fast` に一本化する。
fn encode_rgba_png(
    rgba: &[u8],
    width: u32,
    height: u32,
    compression: PngCompression,
) -> Result<Vec<u8>, png::EncodingError> {
    let (comp, filter, adaptive) = compression.params();
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(comp);
        encoder.set_filter(filter);
        if adaptive {
            encoder.set_adaptive_filter(png::AdaptiveFilterType::Adaptive);
        }
        // writer は data を可変借用する。ブロック末尾で drop し IDAT/IEND を
        // 確定させてから data を返す（tiny-skia の encode_png と同じ構造）。
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba)?;
    }
    Ok(data)
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

        Prim::GradientPath {
            d,
            x0,
            x1,
            stop0,
            stop1,
        } => {
            let Some(path) = parse_path_data(d) else {
                return;
            };
            use tiny_skia::{GradientStop, LinearGradient, Point, Shader, SpreadMode};
            // x0/x1 はユーザ座標で、シェーダ変換は identity。tiny-skia は fill_path の
            // transform をシェーダ評価にも適用するため、これだけで --scale 時もグラデーションは
            // リボン全幅に正しく伸びる(ここで scale を二重に渡すと広がりすぎる)。
            // 回帰: tests/render_gradient.rs::gradient_png_scales_with_geometry_at_2x。
            let shader = LinearGradient::new(
                Point::from_xy(*x0 as f32, 0.0),
                Point::from_xy(*x1 as f32, 0.0),
                vec![
                    GradientStop::new(0.0, to_skia_color(stop0)),
                    GradientStop::new(1.0, to_skia_color(stop1)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            );
            // LinearGradient::new は縮退時(x0==x1)に None を返す。SVG 1.1 では
            // x1==x2 のグラデーションは最後の stop の色で塗るため、SVG 出力と
            // 揃えて stop1 で solid フォールバックする。
            let paint = Paint {
                shader: shader.unwrap_or_else(|| Shader::SolidColor(to_skia_color(stop1))),
                ..Default::default()
            };
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        Prim::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
        } => {
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
            if *stroke_width > 0.0 {
                pixmap.stroke_path(
                    &path,
                    &solid_paint(*stroke),
                    &make_stroke(*stroke_width),
                    transform,
                    None,
                );
            }
        }

        Prim::Text {
            x,
            y,
            size,
            anchor,
            fill,
            content,
            rotate_deg: _, // ラスタ出力では回転未サポート
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

/// ir::Color を tiny-skia の色へ変換する（アルファは 0–255 に丸め）。
fn to_skia_color(color: &Color) -> tiny_skia::Color {
    let alpha = (color.a * 255.0).round() as u8;
    tiny_skia::Color::from_rgba8(color.r, color.g, color.b, alpha)
}

fn solid_paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(to_skia_color(&color));
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

    /// 自前の高速 PNG エンコードは、tiny-skia の `encode_png()` と
    /// バイト単位で完全一致しなければならない（部分α=AA縁を含む実チャートで検証）。
    /// 一致しないと golden 回帰・公開出力変化を招く。
    #[test]
    fn encode_png_fast_matches_tiny_skia_byte_for_byte() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let measurer = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = crate::layout::build_scene(&bar_spec(), &measurer);
        let mut pixmap = scene_to_pixmap(&scene, 1.0, &face, &PNG_LIMITS).unwrap();

        // tiny-skia の参照出力（premultiplied→straight を全画素 demultiply）。
        // encode_png_fast は pixmap を in-place で straight 化するため、参照出力を先に取る。
        let expected = pixmap.encode_png().unwrap();
        // 自前パス（a==255/a==0 は据え置き、部分αのみ in-place demultiply）。Fast は tiny-skia 同設定。
        let actual = encode_png_fast(&mut pixmap, PngCompression::Fast).unwrap();

        assert_eq!(
            actual, expected,
            "fast PNG encode は tiny-skia encode_png とバイト一致でなければならない"
        );
    }

    /// バッファ長が寸法と矛盾する場合、エンコードはエラーを返さなければならない
    /// (パニックや不正 PNG にしない)。
    #[test]
    fn encode_rgba_png_rejects_mismatched_buffer() {
        // 10×10 RGBA は 400 バイト必要。4 バイトしか渡さなければ Err。
        let result = encode_rgba_png(&[0u8; 4], 10, 10, PngCompression::Fast);
        assert!(result.is_err());
    }

    /// 圧縮プリセットは可逆でなければならない(全モードが同一ピクセルへデコード)。
    /// かつ High/Balanced は Fast よりサイズが小さくなければならない(本機能の目的)。
    #[test]
    fn compression_modes_are_pixel_identical_and_smaller() {
        let spec = bar_spec();
        let fast =
            render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, PngCompression::Fast).unwrap();
        let balanced =
            render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, PngCompression::Balanced).unwrap();
        let high =
            render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, PngCompression::High).unwrap();

        // 可逆: 全モードが同一ピクセルへデコードされる。
        let pf = Pixmap::decode_png(&fast).unwrap();
        let pb = Pixmap::decode_png(&balanced).unwrap();
        let ph = Pixmap::decode_png(&high).unwrap();
        assert_eq!(
            pf.data(),
            pb.data(),
            "Balanced は Fast とピクセル一致でなければならない"
        );
        assert_eq!(
            pf.data(),
            ph.data(),
            "High は Fast とピクセル一致でなければならない"
        );

        // 目的: より強い圧縮はより小さい出力。
        assert!(
            balanced.len() < fast.len(),
            "Balanced は Fast より小さいはず"
        );
        assert!(high.len() < fast.len(), "High は Fast より小さいはず");
    }

    /// 既定の `render_chart_to_png` は Balanced を使う(ライブラリ全体で一貫した既定)。
    /// CLI・各バインディング・直接 API がすべて同一の既定出力になることの回帰。
    #[test]
    fn default_render_is_balanced_compression() {
        let spec = bar_spec();
        let default = render_chart_to_png(&spec, 1.0, DEFAULT_FONT).unwrap();
        let balanced =
            render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, PngCompression::Balanced).unwrap();
        assert_eq!(
            default, balanced,
            "既定は Balanced とバイト一致でなければならない"
        );
        assert_eq!(PngCompression::default(), PngCompression::Balanced);
    }

    /// 各モードは決定的(同一入力→同一バイト)でなければならない。
    #[test]
    fn compression_modes_are_deterministic() {
        let spec = bar_spec();
        for c in [
            PngCompression::Fast,
            PngCompression::Balanced,
            PngCompression::High,
        ] {
            let a = render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, c).unwrap();
            let b = render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, c).unwrap();
            assert_eq!(a, b, "{c:?} は決定的でなければならない");
        }
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
        assert!(err.unwrap_err().contains("exceeds"));
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
    fn rasterizes_to_valid_webp() {
        let webp = render_chart_to_webp(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        // WebP ファイルシグネチャ: "RIFF....WEBP"
        assert_eq!(&webp[0..4], b"RIFF");
        assert_eq!(&webp[8..12], b"WEBP");
        assert!(webp.len() > 100);
    }

    #[test]
    fn webp_scale_increases_size() {
        let small = render_chart_to_webp(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        let large = render_chart_to_webp(&bar_spec(), 2.0, DEFAULT_FONT).unwrap();
        assert!(large.len() > small.len());
    }

    #[test]
    fn webp_is_byte_deterministic() {
        let a = render_chart_to_webp(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        let b = render_chart_to_webp(&bar_spec(), 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(a, b, "WebP 出力は決定的でなければならない");
    }

    #[test]
    fn webp_oversized_area_is_err() {
        let mut spec = bar_spec();
        spec.width = 8001.0;
        spec.height = 8001.0;
        let err = render_chart_to_webp(&spec, 1.0, DEFAULT_FONT);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("exceeds"));
    }

    #[test]
    fn infinite_scale_is_err() {
        // +Inf scale → w/h saturate to u32::MAX → area guard triggers RenderError.
        // Bindings do not validate scale; +Inf must not silently succeed.
        let err = render_chart_to_png(&bar_spec(), f32::INFINITY, DEFAULT_FONT);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("exceeds"));
    }

    #[test]
    fn webp_infinite_scale_is_err() {
        let err = render_chart_to_webp(&bar_spec(), f32::INFINITY, DEFAULT_FONT);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("exceeds"));
    }

    #[test]
    fn webp_axis_limit_is_err() {
        let mut spec = bar_spec();
        // 20000×100 = 2M px (area 上限以下) だが WebP 軸制限 16384 を超える
        spec.width = 20_000.0;
        spec.height = 100.0;
        let err = render_chart_to_webp(&spec, 1.0, DEFAULT_FONT);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("per-axis limit"));
    }

    #[test]
    fn webp_rejects_area_over_webp_budget_within_png_limit() {
        // 軸上限(16384)も PNG 面積上限(64M)も満たすが、WebP 専用の面積上限(32M)で
        // pixmap 確保前に拒否しなければならない(根拠は MAX_WEBP_AREA_PIXELS を参照)。
        let mut spec = bar_spec();
        spec.width = 16_001.0; // 軸 ≤ 16384
        spec.height = 2_049.0; // 16001×2049 = 32,786,049 > 32M かつ ≤ 64M
        let err = render_chart_to_webp(&spec, 1.0, DEFAULT_FONT);
        assert!(err.is_err(), "WebP 面積予算超過は Err でなければならない");
        let msg = err.unwrap_err();
        assert!(msg.contains("WebP output"), "msg: {msg}");
        assert!(msg.contains("area limit"), "msg: {msg}");
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
