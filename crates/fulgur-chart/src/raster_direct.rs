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
/// WebP ロスレスのエンコードは、pixmap(in-place demultiply 済み, area×4)を生かした
/// まま `image_webp` 0.2.4 が次の 2 つをさらに確保するため、ピークは raw フレームの
/// 約 3 倍になりうる:
/// 1. 入力の可変複製 — `encode_frame` が `data.to_vec()` してロスレス変換を
///    in-place で行う(encoder.rs:443)。area×4。
/// 2. エンコード出力 — RIFF はチャンクサイズをデータの前に書くため、VP8L チャンク
///    全体を内部 Vec に貯めてから writer へ出す(encoder.rs:674-686)。圧縮しにくい
///    コンテンツでは output ≈ 入力サイズに迫る。最悪 area×4。
///
/// 実測(圧縮不能 RGBA): area=32M で約 369 MB。約 256 MB 予算に収めるには面積上限を
/// PNG の 1/3 にする(pixmap + 入力複製 + 出力 ≈ 3×area×4 ≦ 256 MB)。
/// 実測: area=21.3M で約 247 MB。21.3M px ≒ 4618×4618。
///
/// これはライブラリ既定の hard backstop。サーバ等が独自の(より厳しい)面積予算を
/// 設定する際の基準値として参照できるよう公開する。
pub const MAX_WEBP_AREA_PIXELS: u64 = MAX_PNG_AREA_PIXELS / 3;

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

    // WebPEncoder は straight alpha を要求する。全画素 α==255 が保証される
    // (不透明背景・完全被覆)ときは premult==straight のため demultiply を省く。
    // それ以外は in-place で straight 化する(別バッファを確保しない)。
    if !all_pixels_opaque(&scene, &pixmap, scale) {
        demultiply_in_place(&mut pixmap);
    }

    encode_pixmap_webp(&pixmap)
}

/// straight RGBA8 の Pixmap をロスレス WebP バイト列へエンコードする。
/// `render_chart_to_webp` と回帰テストで共有する（DRY）。
fn encode_pixmap_webp(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
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

/// stamp cache を使う最小連続マーカー数。これ未満の uniform circle run は
/// per-prim 描画にフォールバックする。実測 break-even ~66(stroke込み)、tunable、
/// bench で再確認(Task 6)。`usize::MAX` で stamping を無効化(= pure fill_path)できる。
const STAMP_MIN_RUN: usize = 128;

/// stamp 化するマーカーの device 半径上限(px)。これを超える大きなマーカーは stamp 1 枚が
/// 巨大化して OOM/`Pixmap::new` 失敗を招くため per-prim にフォールバックする(per-prim は
/// 円をキャンバスにクリップして描くので安全)。通常マーカー(r_dev 3〜6)には無影響で、
/// 病的な巨大半径/スケールのみガードする。
const STAMP_MAX_DEVICE_R: f64 = 64.0;

/// Scene を RGBA Pixmap にラスタライズする（PNG/WebP 共通）。
fn scene_to_pixmap(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
    limits: &RasterLimits,
) -> Result<Pixmap, String> {
    scene_to_pixmap_with(scene, scale, face, limits, STAMP_MIN_RUN)
}

/// `scene_to_pixmap` の本体。`min_run` で stamp cache を使う最小 run 長を指定する。
/// `min_run = usize::MAX` で stamping を無効化し、全マーカーを per-prim
/// (`render_prim` → `fill_path`/`stroke_path`)で描く。テストはこれを参照出力に使う。
fn scene_to_pixmap_with(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
    limits: &RasterLimits,
    min_run: usize,
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

    let mut i = 0;
    while i < scene.items.len() {
        let run = uniform_circle_run_len(&scene.items, i);
        // 正半径かつ device サイズが上限内の同一 appearance circle が長く続く run のみ
        // stamp する。それ以外(閾値未満/巨大半径/非円)は per-prim。負の stroke は描画
        // されない(下の stroke_width>0 ガード)ので、サイズ判定では max(0) でクランプする。
        let stampable = run >= min_run
            && matches!(
                &scene.items[i],
                Prim::Circle { r, stroke_width, .. }
                    if *r > 0.0
                        && (*r + stroke_width.max(0.0) / 2.0) * scale as f64 <= STAMP_MAX_DEVICE_R
            );
        if stampable {
            if let Prim::Circle {
                r,
                fill,
                stroke,
                stroke_width,
                ..
            } = &scene.items[i]
            {
                let key = MarkerKey {
                    r: *r,
                    fill: *fill,
                    stroke: *stroke,
                    stroke_width: *stroke_width,
                };
                let set = build_stamp_set(&key, scale);
                for it in &scene.items[i..i + run] {
                    if let Prim::Circle { cx, cy, .. } = it {
                        blit_stamp(&mut pixmap, &set, *cx as f32 * scale, *cy as f32 * scale);
                    }
                }
            }
            i += run;
        } else if run > 0 {
            // stamp しない uniform circle run(閾値未満/巨大半径/r<=0)も run 全体を
            // まとめて描画して i += run で進める。1 要素ずつ進めると毎回フルスキャンが
            // 走り O(n^2) になる(閾値直下の均一 scatter で顕著)。
            for prim in &scene.items[i..i + run] {
                render_prim(&mut pixmap, prim, transform, face, &mut glyph_cache);
            }
            i += run;
        } else {
            render_prim(
                &mut pixmap,
                &scene.items[i],
                transform,
                face,
                &mut glyph_cache,
            );
            i += 1;
        }
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
    let skip = all_pixels_opaque(scene, &pixmap, scale);
    encode_png_fast(&mut pixmap, compression, skip)
}

/// pixmap の全画素が α==255（部分α画素ゼロ）と **静的に保証**できるとき true。
///
/// 条件は 2 つ:
/// 1. `scene.has_opaque_background()` — 最背面に全面不透明 Rect が敷かれている。
///    背景 rect は非 AA（中心サンプリング）で描かれ部分αを作らず、その上の source-over は
///    opaque dest 上で α==255 を保つため、被覆された画素は必ず α==255 になる。
///    skip 正当性は全内容が source-over 合成である前提に依存する（本 crate は既定 source-over のみ）。
/// 2. pixmap が背景の device 矩形 `[0,width*scale]×[0,height*scale]` に**完全内包**される。
///    丸め上げ（`round(w*scale) > w*scale`）だと右/下端に未被覆の列/行が生じ、そこへ AA 内容が
///    届くと部分αが出る余地が残る。内包していれば未被覆画素が存在せず原理的に起きない。
///
/// 両立時のみ `demultiply_in_place`（部分α画素の書き換え）は no-op と確定するので省ける。
/// どちらか false でも安全側で従来スキャンへ fallback する（fail-safe）。
fn all_pixels_opaque(scene: &Scene, pixmap: &Pixmap, scale: f32) -> bool {
    if !scene.has_opaque_background() {
        return false;
    }
    // scene_to_pixmap と同一の scale フォールバック（<=0/非有限は 1.0）。
    let scale = if scale > 0.0 { scale } else { 1.0 };
    // f32 のまま比較するのは **意図的**。`pixmap.width()` は scene_to_pixmap が
    // `(scene.width as f32 * scale).round().max(1.0) as u32` で確保した値なので、
    // 右辺と同一の f32 式に対する `round(e) <= e`（丸め上げでない＝完全被覆）へ厳密に還元される。
    // `pixmap.width() as f32` は f32 由来整数の f32↔u32 往復ゆえ無損失で、64M px 上限
    // (>2^24) でも round down しない（round(e) は常に f32 表現可能な整数）。f64 化すると
    // 逆に「確保に使った f32 積」「tiny-skia が背景を描いた f32 変換」と判定がズレ、
    // 実 pixmap 幾何と乖離しうる。健全性は 8888 万組の掃引で unsound skip ゼロを確認済み。
    (pixmap.width() as f32) <= scene.width as f32 * scale
        && (pixmap.height() as f32) <= scene.height as f32 * scale
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
/// 圧縮 `Compression::Fast`(fdeflate)・フィルタ `Sub` は tiny-skia の png デフォルトと同値。
/// straight 変換を高速化した点だけが異なり、出力は tiny-skia `encode_png()` とバイト一致する
/// （回帰: `encode_png_fast_matches_tiny_skia_byte_for_byte`）。
///
/// `skip_demultiply == true`（呼び出し元が全画素 α==255 を保証: [`all_pixels_opaque`]）のときは
/// demultiply スキャンを丸ごと省き、premultiplied バッファ(`pixmap.data()`)をそのまま
/// straight としてエンコードする。追加バッファは確保しない（ピークメモリ不変）。
///
/// `pixmap` を in-place で straight 化するため `&mut` を取る(呼び出し元はこの直後に
/// pixmap を捨てる前提)。これにより straight のフルフレームコピーを確保しない。
fn encode_png_fast(
    pixmap: &mut Pixmap,
    compression: PngCompression,
    skip_demultiply: bool,
) -> Result<Vec<u8>, String> {
    if !skip_demultiply {
        demultiply_in_place(pixmap);
    }
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

/// items[start] から始まる、同一 appearance(r/fill/stroke/stroke_width) の
/// 連続 Prim::Circle の個数を返す。items[start] が Circle でなければ 0。
///
/// # 事前条件
/// `start < items.len()` でなければならない（`items[start]` を直接添字するため）。
/// 防御的な境界チェックは置かない: 唯一の呼び出し元（後続タスクの描画ループ）が
/// in-bounds を保証する。
fn uniform_circle_run_len(items: &[Prim], start: usize) -> usize {
    let Prim::Circle {
        r,
        fill,
        stroke,
        stroke_width,
        ..
    } = &items[start]
    else {
        return 0;
    };
    let mut n = 0;
    for it in &items[start..] {
        match it {
            Prim::Circle {
                r: r2,
                fill: f2,
                stroke: s2,
                stroke_width: w2,
                ..
            }
                // r2 == r / w2 == stroke_width は f64 の厳密等価。これらは未加工の
                // レイアウト定数（演算結果ではない）なので、ビット単位一致こそが
                // 「同じ見た目」の正しい意味論であり、意図どおり。clippy::float_cmp は
                // 発火しない。
                if r2 == r && f2 == fill && s2 == stroke && w2 == stroke_width =>
            {
                n += 1
            }
            _ => break,
        }
    }
    n
}

const STAMP_B: u32 = 8; // サブピクセル分解能 (8x8=64 stamps/key)

/// stamp キャッシュのキー。マーカーの見た目(半径・塗り・線色・線幅)を表す。
#[derive(Clone, Copy, PartialEq)]
struct MarkerKey {
    r: f64,
    fill: Color,
    stroke: Color,
    stroke_width: f64,
}

/// 1 つの `MarkerKey` に対する B×B 個のサブピクセル stamp 集合。
struct StampSet {
    stamps: Vec<Pixmap>,
    pad: i32,
    b: u32,
}

/// key のマーカーを device 空間で B×B サブピクセル位置に焼いた stamp 集合を作る。
/// 各 stamp は per-point 描画(`Prim::Circle` arm)と同一エンジン:
/// `fill_path`(+ `stroke_width > 0` なら `stroke_path`)で焼く。
fn build_stamp_set(key: &MarkerKey, scale: f32) -> StampSet {
    let r_dev = key.r as f32 * scale;
    let stroke_dev = key.stroke_width as f32 * scale;
    // pad は描画範囲(stroke は r+sw/2 まで張り出す)+ AA/サブピクセル余白。負の r/stroke は
    // 描画されない(from_circle が r<0 で None、stroke は下で stroke_width>0 ガード)ので、
    // サイズ計算では max(0) でクランプし、pad>=2(size>=6 で Pixmap::new は常に Some)を保証する。
    // r_dev 自体は from_circle にそのまま渡す(r<0→None=透明 を per-point と一致させる)。
    let pad = ((r_dev.max(0.0) + stroke_dev.max(0.0) / 2.0).ceil() as i32 + 2).max(2);
    let size = (2 * pad + 2) as u32;
    let anchor = pad as f32;

    let mut stamps = Vec::with_capacity((STAMP_B * STAMP_B) as usize);
    for sy in 0..STAMP_B {
        for sx in 0..STAMP_B {
            // pad >= 2 のため size >= 6 で Pixmap::new は常に Some。
            let mut pm = Pixmap::new(size, size).expect("stamp pixmap サイズは正");
            let cx = anchor + sx as f32 / STAMP_B as f32;
            let cy = anchor + sy as f32 / STAMP_B as f32;
            // per-point の Prim::Circle arm と同一に処理する。from_circle は
            // r_dev<0 でのみ None(負半径=不正→何も焼かず完全透明)。r_dev==0 は
            // 退化円の Some(fill は面積0で無描画、stroke は微小点) を返す。
            // None のとき stamp は完全透明のまま push する(.expect() の panic 回避)。
            if let Some(path) = PathBuilder::from_circle(cx, cy, r_dev) {
                pm.fill_path(
                    &path,
                    &solid_paint(key.fill),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
                if key.stroke_width > 0.0 {
                    pm.stroke_path(
                        &path,
                        &solid_paint(key.stroke),
                        &make_stroke(key.stroke_width * scale as f64),
                        Transform::identity(),
                        None,
                    );
                }
            }
            stamps.push(pm);
        }
    }

    StampSet {
        stamps,
        pad,
        b: STAMP_B,
    }
}

/// device 座標 (cx_dev,cy_dev) に対し、整数描画位置 (ix,iy) と
/// 選択するサブピクセル stamp の添字 idx を返す。
///
/// 小数部を B 段階に量子化して stamp を選ぶ。量子化が B に丸まった場合は
/// 次の整数画素に繰り上げる(kx==B → kx=0, ix+=1)。`blit_stamp` と
/// byte-identity テストの双方がこの pick を共有し、選択ロジックの一致を保証する。
fn pick_stamp(set: &StampSet, cx_dev: f32, cy_dev: f32) -> (i32, i32, usize) {
    let mut ix = cx_dev.floor() as i32;
    let mut kx = ((cx_dev - ix as f32) * set.b as f32).round() as i32;
    if kx as u32 == set.b {
        kx = 0;
        ix += 1;
    }
    let mut iy = cy_dev.floor() as i32;
    let mut ky = ((cy_dev - iy as f32) * set.b as f32).round() as i32;
    if ky as u32 == set.b {
        ky = 0;
        iy += 1;
    }
    // 通常の carry 後は [0, b-1] に収まるが、巨大/異常座標での丸め飽和に備えて
    // クランプし、idx が必ず stamps(長さ b*b) の範囲内になるようにする。
    let kx = kx.clamp(0, set.b as i32 - 1);
    let ky = ky.clamp(0, set.b as i32 - 1);
    let idx = (ky as u32 * set.b + kx as u32) as usize;
    (ix, iy, idx)
}

/// device 座標 (cx_dev,cy_dev) のマーカーを、サブピクセル stamp を選び整数位置に
/// premultiplied source-over で blit する。canvas 外はピクセル単位でクリップする。
///
/// `draw_pixmap`(identity + Nearest + SourceOver) とバイト一致するが、Pattern
/// シェーダ/パイプラインの再構築を避けるため手書きする(計測で ~7 倍速)。
/// premultiplied 同士の source-over: `out_c = s_c + (d_c*(255 - s_a) + 127) / 255`。
/// s_a==0 の画素はスキップ(恒等変換)。
fn blit_stamp(pm: &mut Pixmap, set: &StampSet, cx_dev: f32, cy_dev: f32) {
    // 非有限座標(NaN/±Inf)は描画しない。per-point の from_circle が非有限円を弾くのと
    // 同じ挙動に揃え、i32 への飽和キャストによる OOB index も防ぐ。
    if !cx_dev.is_finite() || !cy_dev.is_finite() {
        return;
    }
    let (ix, iy, idx) = pick_stamp(set, cx_dev, cy_dev);
    let stamp = &set.stamps[idx];
    let ss = stamp.width() as i32;
    let src = stamp.data();
    // 位置は i64 で計算する。巨大な有限座標(例 cx=1e38)で ix/iy が i32::MAX に飽和すると
    // `oy + sy` が i32 で桁あふれし、クリップ判定前に debug panic する(codex 指摘)。
    let ox = ix as i64 - set.pad as i64;
    let oy = iy as i64 - set.pad as i64;
    // pm.data_mut() で借用する前に寸法を確定させる。
    let w = pm.width() as i64;
    let h = pm.height() as i64;
    let dst = pm.data_mut();

    for sy in 0..ss {
        let dy = oy + sy as i64;
        if dy < 0 || dy >= h {
            continue;
        }
        for sx in 0..ss {
            let dx = ox + sx as i64;
            if dx < 0 || dx >= w {
                continue;
            }
            let si = ((sy * ss + sx) * 4) as usize;
            let s_a = src[si + 3];
            if s_a == 0 {
                continue;
            }
            let di = ((dy * w + dx) * 4) as usize;
            let inv = 255 - s_a as u32;
            for c in 0..4 {
                let s_c = src[si + c] as u32;
                let d_c = dst[di + c] as u32;
                dst[di + c] = (s_c + (d_c * inv + 127) / 255) as u8;
            }
        }
    }
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

    /// 不透明背景の spec（bar_spec に白背景を付与）。
    /// 色コード `#ffffff` を含むため `"#` を含まない `r##"..."##` デリミタを使う。
    fn opaque_bar_spec() -> crate::ir::ChartSpec {
        chartjs::parse(
            r##"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"label":"売上","data":[10,20,30]}]},"options":{"theme":{"backgroundColor":"#ffffff"}}}"##,
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
        // bar_spec は背景なし → 非 opaque → skip=false（従来どおり全画素 demultiply）。
        let actual = encode_png_fast(&mut pixmap, PngCompression::Fast, false).unwrap();

        assert_eq!(
            actual, expected,
            "fast PNG encode は tiny-skia encode_png とバイト一致でなければならない"
        );
    }

    /// opaque 背景では demultiply をスキップしても tiny-skia の全画素 demultiply と
    /// バイト一致しなければならない（全画素 α==255 で premult==straight）。
    #[test]
    fn encode_png_fast_opaque_bg_skip_matches_tiny_skia_byte_for_byte() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let measurer = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = crate::layout::build_scene(&opaque_bar_spec(), &measurer);
        assert!(scene.has_opaque_background());

        let mut pixmap = scene_to_pixmap(&scene, 1.0, &face, &PNG_LIMITS).unwrap();
        // 参照: tiny-skia の全画素 demultiply（&self、pixmap を破壊しない）。
        let expected = pixmap.encode_png().unwrap();
        // スキップ経路（demultiply を呼ばない）。opaque なので一致するはず。
        let actual = encode_png_fast(&mut pixmap, PngCompression::Fast, true).unwrap();
        assert_eq!(
            actual, expected,
            "opaque skip 経路は tiny-skia encode_png とバイト一致でなければならない"
        );
    }

    /// 部分α内容(AA 縁)で誤って skip すると tiny-skia の全 demultiply と食い違う。
    /// = demultiply は load-bearing であり、opaque skip テストの一致は guard が
    /// 部分α画素の存在を排除しているからこそ意味を持つ、ことの証明。
    #[test]
    fn encode_png_fast_wrong_skip_on_partial_alpha_diverges() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let measurer = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        // bar_spec は背景なし → AA 縁に部分α画素がある。
        let scene = crate::layout::build_scene(&bar_spec(), &measurer);
        let mut pixmap = scene_to_pixmap(&scene, 1.0, &face, &PNG_LIMITS).unwrap();
        let expected = pixmap.encode_png().unwrap(); // 全画素 demultiply（正しい出力）
        let wrongly_skipped = encode_png_fast(&mut pixmap, PngCompression::Fast, true).unwrap();
        assert_ne!(
            wrongly_skipped, expected,
            "部分α内容で skip すると出力は食い違うはず（demultiply は load-bearing）"
        );
    }

    /// スキップ判定: opaque + 整数 scale は true、非 opaque は false、
    /// opaque + 丸め上げ分数 scale は false（安全側で従来スキャンへ）。
    #[test]
    fn all_pixels_opaque_decision() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let measurer = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();

        let op = crate::layout::build_scene(&opaque_bar_spec(), &measurer);
        let pm1 = scene_to_pixmap(&op, 1.0, &face, &PNG_LIMITS).unwrap();
        assert!(all_pixels_opaque(&op, &pm1, 1.0), "opaque+整数は skip 可");
        let pm2 = scene_to_pixmap(&op, 2.0, &face, &PNG_LIMITS).unwrap();
        assert!(
            all_pixels_opaque(&op, &pm2, 2.0),
            "opaque+整数(2x)は skip 可"
        );

        // op.width に依存せず丸め上げを強制: width*scale = width+0.5 →
        // round=width+1 > width+0.5 → 被覆保証不可（安全側で skip 不可）。
        let round_up_scale = 1.0 + 0.5 / op.width as f32;
        let pmf = scene_to_pixmap(&op, round_up_scale, &face, &PNG_LIMITS).unwrap();
        assert!(
            !all_pixels_opaque(&op, &pmf, round_up_scale),
            "丸め上げは skip 不可"
        );

        let non = crate::layout::build_scene(&bar_spec(), &measurer);
        let pmn = scene_to_pixmap(&non, 1.0, &face, &PNG_LIMITS).unwrap();
        assert!(
            !all_pixels_opaque(&non, &pmn, 1.0),
            "非 opaque は skip 不可"
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

    /// WebP も opaque 時に demultiply をスキップして byte 一致すること。
    /// 参照 = 全画素 demultiply 後にエンコード、実測 = スキップしてエンコード。
    #[test]
    fn webp_opaque_bg_skip_matches_full_demultiply_byte_for_byte() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let measurer = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = crate::layout::build_scene(&opaque_bar_spec(), &measurer);
        assert!(scene.has_opaque_background());

        let mut ref_pm = scene_to_pixmap(&scene, 1.0, &face, &WEBP_LIMITS).unwrap();
        let skip_pm = ref_pm.clone();
        demultiply_in_place(&mut ref_pm);
        let expected = encode_pixmap_webp(&ref_pm).unwrap();
        let actual = encode_pixmap_webp(&skip_pm).unwrap();

        assert_eq!(
            actual, expected,
            "opaque WebP skip 経路は全 demultiply 経路とバイト一致でなければならない"
        );
    }

    /// WebP と PNG は同一 spec を同一ピクセルへエンコードすること(end-to-end)。
    /// 非 opaque(demultiply 分岐)と opaque(skip 分岐)の両方を通し、guard 誤配線
    /// (例: `!` 脱落で premultiplied のまま WebP 出力)を AA 縁のズレとして捕捉する。
    ///
    /// 両経路とも同じ straight RGBA を lossless に書くが、デコーダの alpha 規約が違う:
    /// `image` の WebP デコードは straight を返し、tiny-skia の `decode_png` は
    /// `premultiply_u8` で premultiplied 化して返す。straight→premult→straight の
    /// round-trip は低 α 縁で丸め損失があるため、WebP 側を tiny-skia と同一の
    /// `premultiply_u8`(= `ColorU8::premultiply`)で premultiplied 空間へ揃えてから
    /// 厳密比較する。両者が同じ straight 値を書いていれば byte 一致する。
    #[test]
    fn webp_and_png_decode_to_identical_pixels() {
        for (name, spec) in [("non_opaque", bar_spec()), ("opaque", opaque_bar_spec())] {
            let webp = render_chart_to_webp(&spec, 1.0, DEFAULT_FONT).unwrap();
            let png =
                render_chart_to_png_with(&spec, 1.0, DEFAULT_FONT, PngCompression::Fast).unwrap();
            let webp_rgba = image::load_from_memory_with_format(&webp, image::ImageFormat::WebP)
                .unwrap()
                .to_rgba8();
            let png_pm = tiny_skia::Pixmap::decode_png(&png).unwrap();
            assert_eq!(webp_rgba.width(), png_pm.width(), "{name} width");
            assert_eq!(webp_rgba.height(), png_pm.height(), "{name} height");

            // WebP の straight RGBA を tiny-skia と同一の premultiply で揃える。
            let webp_premul: Vec<u8> = webp_rgba
                .pixels()
                .flat_map(|p| {
                    let pm = tiny_skia::ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
                    [pm.red(), pm.green(), pm.blue(), pm.alpha()]
                })
                .collect();
            assert_eq!(
                webp_premul.as_slice(),
                png_pm.data(),
                "{name}: WebP と PNG は同一ピクセルへエンコードすべき（premultiplied 空間で厳密一致）"
            );
        }
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
        // 軸上限(16384)も PNG 面積上限(64M)も満たすが、WebP 専用の面積上限
        // (PNG の 1/3 ≈ 21.3M)で pixmap 確保前に拒否しなければならない
        // (根拠は MAX_WEBP_AREA_PIXELS を参照)。
        let mut spec = bar_spec();
        spec.width = 16_001.0; // 軸 ≤ 16384
        spec.height = 2_049.0; // 16001×2049 = 32,786,049 > 21.3M かつ ≤ 64M
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

    fn circle(cx: f64, cy: f64, r: f64, fill: Color) -> Prim {
        Prim::Circle {
            cx,
            cy,
            r,
            fill,
            stroke: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            },
            stroke_width: 1.0,
        }
    }

    const RED: Color = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 1.0,
    };
    const BLUE: Color = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 1.0,
    };

    #[test]
    fn uniform_circle_run_len_counts_consecutive_same_appearance() {
        // 同一 appearance(r=3.0/fill=RED)の円が 3 個続き、4 個目は r が異なる。
        // cx/cy が違っても appearance が同じなら 1 つの run に数える。
        let items = [
            circle(0.0, 0.0, 3.0, RED),
            circle(10.0, 0.0, 3.0, RED),
            circle(20.0, 0.0, 3.0, RED),
            circle(30.0, 0.0, 5.0, RED), // r が異なる → run を切る
        ];
        assert_eq!(uniform_circle_run_len(&items, 0), 3);
    }

    #[test]
    fn uniform_circle_run_len_from_break_point_returns_one() {
        // run を切る 4 個目から開始すると、その円 1 個だけが数えられる。
        let items = [
            circle(0.0, 0.0, 3.0, RED),
            circle(10.0, 0.0, 3.0, RED),
            circle(20.0, 0.0, 3.0, RED),
            circle(30.0, 0.0, 5.0, RED),
        ];
        assert_eq!(uniform_circle_run_len(&items, 3), 1);
    }

    #[test]
    fn uniform_circle_run_len_breaks_on_differing_fill() {
        // fill が違えば appearance が異なるため run が切れる。
        let items = [
            circle(0.0, 0.0, 3.0, RED),
            circle(10.0, 0.0, 3.0, RED),
            circle(20.0, 0.0, 3.0, BLUE), // fill が異なる → run を切る
        ];
        assert_eq!(uniform_circle_run_len(&items, 0), 2);
    }

    #[test]
    fn uniform_circle_run_len_breaks_on_differing_stroke() {
        // r/fill/stroke_width が同一でも stroke 色が違えば appearance が異なり run が切れる。
        let red_stroke = Prim::Circle {
            cx: 0.0,
            cy: 0.0,
            r: 3.0,
            fill: RED,
            stroke: RED,
            stroke_width: 1.0,
        };
        let blue_stroke = Prim::Circle {
            cx: 20.0,
            cy: 0.0,
            r: 3.0,
            fill: RED,
            stroke: BLUE, // stroke 色のみ異なる → run を切る
            stroke_width: 1.0,
        };
        let items = [red_stroke.clone(), red_stroke, blue_stroke];
        assert_eq!(uniform_circle_run_len(&items, 0), 2);
    }

    #[test]
    fn uniform_circle_run_len_breaks_on_differing_stroke_width() {
        // r/fill/stroke が同一でも stroke_width が違えば appearance が異なり run が切れる。
        let stroke = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 1.0,
        };
        let thin = Prim::Circle {
            cx: 0.0,
            cy: 0.0,
            r: 3.0,
            fill: RED,
            stroke,
            stroke_width: 1.0,
        };
        let thick = Prim::Circle {
            cx: 10.0,
            cy: 0.0,
            r: 3.0,
            fill: RED,
            stroke,
            stroke_width: 2.0, // stroke_width のみ異なる → run を切る
        };
        let items = [thin.clone(), thin, thick];
        assert_eq!(uniform_circle_run_len(&items, 0), 2);
    }

    #[test]
    fn uniform_circle_run_len_zero_for_non_circle_start() {
        // 開始要素が Circle でなければ 0。
        let items = [
            Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: 5.0,
                h: 5.0,
                fill: RED,
            },
            circle(10.0, 0.0, 3.0, RED),
        ];
        assert_eq!(uniform_circle_run_len(&items, 0), 0);
    }

    // --- build_stamp_set (B=8 サブピクセル stamp ビルダ) -----------------------

    /// 非透明(alpha>0)画素数を数える。RGBA8 の 4 バイト目が alpha。
    fn nonzero_alpha_count(pm: &Pixmap) -> usize {
        pm.data().chunks_exact(4).filter(|px| px[3] > 0).count()
    }

    fn stamp_key(r: f64, stroke_width: f64) -> MarkerKey {
        MarkerKey {
            r,
            fill: RED,
            stroke: BLUE,
            stroke_width,
        }
    }

    #[test]
    fn build_stamp_set_count_and_size() {
        let key = stamp_key(3.0, 1.0);
        let scale = 1.0_f32;
        let set = build_stamp_set(&key, scale);

        // B×B = 64 stamps。
        assert_eq!(set.b, STAMP_B);
        assert_eq!(set.stamps.len(), (STAMP_B * STAMP_B) as usize);

        // pad / size の期待値を仕様どおり再計算。
        let r_dev = key.r as f32 * scale;
        let stroke_dev = key.stroke_width as f32 * scale;
        let expected_pad = (r_dev + stroke_dev / 2.0).ceil() as i32 + 2;
        let expected_size = (2 * expected_pad + 2) as u32;
        assert_eq!(set.pad, expected_pad);

        for pm in &set.stamps {
            assert_eq!(pm.width(), expected_size);
            assert_eq!(pm.height(), expected_size);
        }
    }

    /// (sx=0,sy=0) stamp は、同一エンジンで pad 位置に手で焼いた Pixmap と
    /// バイト一致する。pad/center/順序(fill→stroke)が正しいことを保証する。
    fn assert_zero_offset_byte_identity(key: &MarkerKey, scale: f32) {
        let set = build_stamp_set(key, scale);
        let zero = &set.stamps[0]; // sy*B + sx = 0

        let r_dev = key.r as f32 * scale;
        let stroke_dev = key.stroke_width as f32 * scale;
        let pad = (r_dev + stroke_dev / 2.0).ceil() as i32 + 2;
        let size = (2 * pad + 2) as u32;
        let anchor = pad as f32;

        let mut expected = Pixmap::new(size, size).unwrap();
        let path = PathBuilder::from_circle(anchor, anchor, r_dev).unwrap();
        expected.fill_path(
            &path,
            &solid_paint(key.fill),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        if key.stroke_width > 0.0 {
            expected.stroke_path(
                &path,
                &solid_paint(key.stroke),
                &make_stroke(key.stroke_width * scale as f64),
                Transform::identity(),
                None,
            );
        }

        assert_eq!(zero.data(), expected.data());
    }

    #[test]
    fn build_stamp_set_zero_offset_byte_identical_with_stroke() {
        assert_zero_offset_byte_identity(&stamp_key(3.0, 1.0), 1.0);
    }

    #[test]
    fn build_stamp_set_zero_offset_byte_identical_no_stroke() {
        assert_zero_offset_byte_identity(&stamp_key(3.0, 0.0), 1.0);
    }

    #[test]
    fn build_stamp_set_bakes_stroke() {
        // stroke 色(BLUE, 不透明) != fill。stroke 有りは無しより非透明画素が多い。
        let with_stroke = build_stamp_set(&stamp_key(3.0, 1.0), 1.0);
        let without_stroke = build_stamp_set(&stamp_key(3.0, 0.0), 1.0);

        let with_count: usize = nonzero_alpha_count(&with_stroke.stamps[0]);
        let without_count: usize = nonzero_alpha_count(&without_stroke.stamps[0]);

        assert!(
            with_count > without_count,
            "stroke 有り({with_count}) は無し({without_count}) より非透明画素が多いはず"
        );
    }

    #[test]
    fn build_stamp_set_subpixel_stamps_differ() {
        // (sx=0,sy=0)=index 0 と (sx=7,sy=0)=index 7 はサブピクセル位置が違うため
        // AA 結果(.data())が異なる。
        let set = build_stamp_set(&stamp_key(3.0, 1.0), 1.0);
        let idx_00 = 0; // sy*B + sx = 0
        let idx_70 = 7; // sy=0, sx=7 → 0*B + 7
        assert_ne!(set.stamps[idx_00].data(), set.stamps[idx_70].data());
    }

    #[test]
    fn build_stamp_set_zero_radius_no_panic() {
        // .expect() を置き換えた fix の確認。tiny-skia の from_circle は
        // r<0 で None(描画なし)、r==0 では Some(退化パス) を返すため、
        // r=0 では fill は面積0で何も描かないが stroke は微小な点を焼く
        // (per-point Circle arm と同一挙動)。よって「panic しない」ことと
        // 「stamp が per-point 描画とバイト一致する」ことを確認する。
        let key = stamp_key(0.0, 1.0); // r=0, stroke_width=1.0
        let set = build_stamp_set(&key, 1.0);
        assert_eq!(set.stamps.len(), (STAMP_B * STAMP_B) as usize);

        // sx=sy=0 stamp は per-point の Prim::Circle arm 出力とバイト一致。
        // r=0 でも from_circle=Some なので退化円が焼かれる(stroke の微小点)。
        let pad = set.pad;
        let size = set.stamps[0].width();
        let anchor = pad as f32;
        let mut expected = Pixmap::new(size, size).unwrap();
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let mut cache = HashMap::new();
        render_prim(
            &mut expected,
            &Prim::Circle {
                cx: anchor as f64,
                cy: anchor as f64,
                r: 0.0,
                fill: key.fill,
                stroke: key.stroke,
                stroke_width: key.stroke_width,
            },
            Transform::identity(),
            &face,
            &mut cache,
        );
        assert_eq!(set.stamps[0].data(), expected.data());

        // r<0 は from_circle=None → 完全透明(描画なし)。
        let neg = build_stamp_set(&stamp_key(-1.0, 1.0), 1.0);
        assert_eq!(neg.stamps.len(), (STAMP_B * STAMP_B) as usize);
        for pm in &neg.stamps {
            assert_eq!(nonzero_alpha_count(pm), 0, "r<0 stamp は完全透明のはず");
        }
    }

    #[test]
    fn build_stamp_set_scale_two() {
        // scale=2 で半径・線幅が device 空間で 2 倍になり pad/size に反映される。
        // r_dev=6, stroke_dev=2 → pad = ceil(6 + 2/2) + 2 = ceil(7) + 2 = 9,
        // size = 2*9 + 2 = 20。radius/stroke への * scale 取りこぼしを検出する。
        let set = build_stamp_set(&stamp_key(3.0, 1.0), 2.0);
        assert_eq!(set.stamps.len(), (STAMP_B * STAMP_B) as usize);
        assert_eq!(set.pad, 9);
        assert_eq!(set.stamps[0].width(), 20);
        assert_eq!(set.stamps[0].height(), 20);
        // サブピクセル stamp は scale 適用後も互いに異なる。
        assert_ne!(set.stamps[0].data(), set.stamps[7].data());
    }

    #[test]
    fn build_stamp_set_pad_size_literal() {
        // pad/size の式をリテラルでピン留めする(既存テストは式を再計算するため
        // 式変更に追従してしまう)。r=3, stroke=1, scale=1 →
        // pad = ceil(3 + 1/2) + 2 = ceil(3.5) + 2 = 4 + 2 = 6, size = 2*6 + 2 = 14。
        let set = build_stamp_set(&stamp_key(3.0, 1.0), 1.0);
        assert_eq!(set.pad, 6);
        assert_eq!(set.stamps[0].width(), 14);
    }

    // --- blit_stamp (手書き premultiplied source-over blit) --------------------

    /// 半透明 fill のマーカーキー（stroke なし）。
    fn stamp_key_semi(stroke_width: f64) -> MarkerKey {
        MarkerKey {
            r: 3.0,
            fill: Color {
                r: 255,
                g: 0,
                b: 0,
                a: 0.5,
            },
            stroke: BLUE,
            stroke_width,
        }
    }

    /// `blit_stamp` と、その pick を再現した `draw_pixmap` が
    /// 非空（半透明背景）の dest 上でバイト一致することを検証する。
    /// これが本機能の correctness lock。
    fn assert_blit_matches_draw_pixmap(key: &MarkerKey, cx: f32, cy: f32) {
        let set = build_stamp_set(key, 1.0);
        let (w, h) = (40u32, 40u32);
        // 半透明背景（premultiplied で格納）→ source-over を非空 dest 上で行使する。
        let bg = tiny_skia::Color::from_rgba8(0, 128, 0, 128);

        let mut pm_a = Pixmap::new(w, h).unwrap();
        pm_a.fill(bg);
        blit_stamp(&mut pm_a, &set, cx, cy);

        let mut pm_b = Pixmap::new(w, h).unwrap();
        pm_b.fill(bg);
        // blit_stamp 自身の pick を再現して draw_pixmap に与える。
        let (ix, iy, idx) = pick_stamp(&set, cx, cy);
        pm_b.draw_pixmap(
            ix - set.pad,
            iy - set.pad,
            set.stamps[idx].as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );

        assert_eq!(
            pm_a.data(),
            pm_b.data(),
            "blit_stamp は draw_pixmap とバイト一致でなければならない (key.r={}, cx={cx}, cy={cy})",
            key.r
        );
    }

    #[test]
    fn blit_stamp_byte_identical_to_draw_pixmap_opaque_integer() {
        // 不透明 fill・整数位置。
        assert_blit_matches_draw_pixmap(&stamp_key(3.0, 1.0), 20.0, 20.0);
    }

    #[test]
    fn blit_stamp_byte_identical_to_draw_pixmap_semi_fractional() {
        // 半透明 fill・小数位置（サブピクセル stamp が選ばれる）。
        assert_blit_matches_draw_pixmap(&stamp_key_semi(1.0), 20.3, 20.7);
    }

    #[test]
    fn blit_stamp_byte_identical_to_draw_pixmap_opaque_fractional() {
        // 不透明 fill・小数位置でも一致する。
        assert_blit_matches_draw_pixmap(&stamp_key(3.0, 1.0), 18.6, 21.4);
    }

    #[test]
    fn blit_stamp_byte_identical_to_draw_pixmap_carry_to_next_pixel() {
        // 小数部が B に丸まる位置（round(0.95*8)=8）。pick_stamp の繰り上げ
        // 分岐(k==b → k=0, i+=1)を通っても draw_pixmap と一致する。
        assert_blit_matches_draw_pixmap(&stamp_key(3.0, 1.0), 20.95, 20.95);
    }

    #[test]
    fn blit_stamp_overlap_composites_not_overwrites() {
        // 半透明 stamp を重ねて blit すると、重なり画素は単独より不透明になる
        // (source-over の累積であって上書きコピーではない)。
        let key = stamp_key_semi(0.0); // stroke なし → 中心は純 fill
        let set = build_stamp_set(&key, 1.0);
        let (w, h) = (40u32, 40u32);

        // 単独 blit。
        let mut single = Pixmap::new(w, h).unwrap();
        blit_stamp(&mut single, &set, 20.0, 20.0);

        // 2 つを重ねて blit（x を 2px ずらす → 中心付近で重なる）。
        let mut overlap = Pixmap::new(w, h).unwrap();
        blit_stamp(&mut overlap, &set, 20.0, 20.0);
        blit_stamp(&mut overlap, &set, 22.0, 20.0);

        // 両 stamp が覆う内部画素 (20,20) の alpha を比較する。
        let alpha_at =
            |pm: &Pixmap, x: u32, y: u32| -> u8 { pm.data()[((y * w + x) * 4 + 3) as usize] };
        let single_a = alpha_at(&single, 20, 20);
        let overlap_a = alpha_at(&overlap, 20, 20);

        assert!(single_a > 0, "単独 blit で (20,20) は描画されているはず");
        assert!(
            overlap_a > single_a,
            "重なり画素 alpha({overlap_a}) は単独 alpha({single_a}) より大きい(累積)はず"
        );
    }

    #[test]
    fn blit_stamp_off_canvas_clips_without_panic() {
        let set = build_stamp_set(&stamp_key(3.0, 1.0), 1.0);

        // 左上にはみ出す（ox/oy が負）→ panic せず、内側の画素は描かれる。
        let mut pm_tl = Pixmap::new(10, 10).unwrap();
        blit_stamp(&mut pm_tl, &set, 0.0, 0.0);
        assert!(
            nonzero_alpha_count(&pm_tl) > 0,
            "一部内側のはみ出しでも in-bounds 画素は描かれるはず"
        );

        // 右下に w/h を超えてはみ出す → panic せず、内側の画素は描かれる。
        let mut pm_br = Pixmap::new(10, 10).unwrap();
        blit_stamp(&mut pm_br, &set, 10.0, 10.0);
        assert!(
            nonzero_alpha_count(&pm_br) > 0,
            "右下はみ出しでも in-bounds 画素は描かれるはず"
        );

        // 完全に外側 → dest は不変（全透明のまま）。
        let mut pm_out = Pixmap::new(10, 10).unwrap();
        blit_stamp(&mut pm_out, &set, -100.0, -100.0);
        let untouched = Pixmap::new(10, 10).unwrap();
        assert_eq!(
            pm_out.data(),
            untouched.data(),
            "完全に外側なら dest は不変のはず"
        );
    }

    // --- scene_to_pixmap_with: stamp cache 配線 + フォールバック ----------------

    /// JSON spec を build_scene パイプライン(parse → measurer → scene)に通す。
    fn scene_for(json: &str) -> Scene {
        let spec = crate::frontend::chartjs::parse(json, false).unwrap();
        let m = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        crate::layout::build_scene(&spec, &m)
    }

    fn face() -> ttf_parser::Face<'static> {
        ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap()
    }

    /// いずれかのチャンネル絶対差が 4 を超えるピクセルの割合
    /// (`tests/golden_png.rs` と同一指標)。u8 同士の減算は debug で
    /// アンダーフロー panic するため i16 にキャストする。
    fn diff_fraction(a: &Pixmap, b: &Pixmap) -> f64 {
        let diff = a
            .data()
            .chunks_exact(4)
            .zip(b.data().chunks_exact(4))
            .filter(|(x, y)| {
                x.iter()
                    .zip(y.iter())
                    .any(|(xc, yc)| (*xc as i16 - *yc as i16).abs() > 4)
            })
            .count();
        let total = (a.width() as u64 * a.height() as u64) as f64;
        diff as f64 / total
    }

    /// n 点の uniform scatter(単色・既定 pointRadius)。stamp path を踏む形。
    fn uniform_scatter_json(n: usize) -> String {
        let pts = (0..n)
            .map(|i| format!(r#"{{"x":{i},"y":{}}}"#, (i * 37 + 13) % 100))
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"type":"scatter","data":{{"datasets":[{{"label":"d","data":[{pts}]}}]}}}}"#)
    }

    /// per-point カラー(backgroundColor 配列)で塗りが点ごとに変わる scatter。
    /// `fill_at` が cyclic に色を引くため隣接点で fill が異なり run が切れる。
    fn percolor_scatter_json(n: usize) -> String {
        let pts = (0..n)
            .map(|i| format!(r#"{{"x":{i},"y":{}}}"#, (i * 37 + 13) % 100))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r##"{{"type":"scatter","data":{{"datasets":[{{"label":"d","backgroundColor":["#ff0000","#0000ff"],"data":[{pts}]}}]}}}}"##
        )
    }

    /// per-point 半径(point.r)で半径が点ごとに変わる bubble。隣接点で r が異なり
    /// run が切れる。r は常に正(2..=10)。
    fn bubble_json(n: usize) -> String {
        let pts = (0..n)
            .map(|i| {
                format!(
                    r#"{{"x":{i},"y":{},"r":{}}}"#,
                    (i * 37 + 13) % 100,
                    2 + i % 9
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"type":"bubble","data":{{"datasets":[{{"label":"d","data":[{pts}]}}]}}}}"#)
    }

    /// per-point カラー scatter(>=128 点)は stamp path を踏まずフォールバックする。
    /// `min_run=STAMP_MIN_RUN` と `min_run=usize::MAX`(stamping 無効=純 fill_path)が
    /// バイト一致することで、色の変化が run を 128 未満に切ったことを自己検証する。
    #[test]
    fn fallback_per_point_color_is_byte_identical() {
        let scene = scene_for(&percolor_scatter_json(200));
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, usize::MAX).unwrap();
        assert_eq!(
            stamp.data(),
            reference.data(),
            "per-point カラーは stamp path を踏まずバイト一致のはず"
        );
    }

    /// per-point 半径 bubble(>=128 点)は stamp path を踏まずフォールバックする。
    /// 半径の変化が run を切ることをバイト一致で自己検証する(定数 r なら run が
    /// 128 以上のまま stamp され、ここが失敗する)。
    #[test]
    fn fallback_bubble_per_point_radius_is_byte_identical() {
        let scene = scene_for(&bubble_json(200));
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, usize::MAX).unwrap();
        assert_eq!(
            stamp.data(),
            reference.data(),
            "per-point 半径は stamp path を踏まずバイト一致のはず"
        );
    }

    /// 128 点未満の uniform scatter は run < min_run でフォールバックする。
    #[test]
    fn fallback_short_uniform_run_is_byte_identical() {
        let scene = scene_for(&uniform_scatter_json(100));
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, usize::MAX).unwrap();
        assert_eq!(
            stamp.data(),
            reference.data(),
            "短い run(100 < 128)は stamp path を踏まずバイト一致のはず"
        );
    }

    /// 128 点以上の uniform scatter は stamp path を踏む。stamp 出力(fill+stroke)は
    /// 参照(per-prim fill_path)と視覚的に同等(差分画素 2% 未満)で、かつバイト一致
    /// ではない(stamp path が実際に走ったことの証明)。差分が 2% を大きく超えるなら
    /// 位置/座標バグ。
    #[test]
    fn stamp_uniform_scatter_within_tolerance() {
        let scene = scene_for(&uniform_scatter_json(200));
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, usize::MAX).unwrap();

        assert_ne!(
            stamp.data(),
            reference.data(),
            "uniform scatter(200 点)は stamp path を踏むはず(バイト一致ではない)"
        );

        let frac = diff_fraction(&stamp, &reference);
        assert!(
            frac < 0.02,
            "stamp 出力は参照と視覚的に同等(差分 {frac:.6} < 0.02)のはず"
        );
    }

    /// scale != 1 (retina) でも stamp 出力が fill_path 参照の許容内であること。
    /// stamp は `from_circle(r*scale)` を identity で焼く一方、参照は `from_circle(r)` を
    /// `transform=scale` で変換するため、ベジェ近似の差で scale=1 より乖離し得る。device
    /// 幾何は一致するので差は小さいはずだが、retina PNG/WebP の視覚回帰をガードする。
    #[test]
    fn stamp_uniform_scatter_within_tolerance_at_scale_2() {
        let scene = scene_for(&uniform_scatter_json(200));
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 2.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 2.0, &f, &PNG_LIMITS, usize::MAX).unwrap();

        assert_ne!(
            stamp.data(),
            reference.data(),
            "scale=2 でも uniform scatter(200 点)は stamp path を踏むはず"
        );

        let frac = diff_fraction(&stamp, &reference);
        assert!(
            frac < 0.02,
            "scale=2 の stamp 出力は参照と視覚的に同等(差分 {frac:.6} < 0.02)のはず"
        );
    }

    /// device 半径が STAMP_MAX_DEVICE_R を超える巨大マーカーは stamp せず per-prim に
    /// フォールバックする(stamp 1 枚巨大化による OOM/panic 回避)。出力は fill_path 参照と
    /// バイト一致する(= 確かにフォールバックしている)。
    #[test]
    fn huge_radius_markers_fall_back_to_fillpath() {
        // pointRadius=70 → device 半径 70 > 64(上限)。130 点で run>=128 だが stamp しない。
        let pts = (0..130)
            .map(|i| format!(r#"{{"x":{},"y":{}}}"#, i, (i * 37 + 13) % 100))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{"type":"scatter","data":{{"datasets":[{{"label":"d","pointRadius":70,"data":[{pts}]}}]}}}}"#
        );
        let scene = scene_for(&json);
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, usize::MAX).unwrap();
        assert_eq!(
            stamp.data(),
            reference.data(),
            "巨大半径マーカーは per-prim フォールバックで出力不変のはず"
        );
    }

    /// 負の stroke_width でも build_stamp_set は panic しない(pad を max(0) でクランプ)。
    #[test]
    fn build_stamp_set_negative_stroke_no_panic() {
        let set = build_stamp_set(&stamp_key(3.0, -1000.0), 1.0);
        assert_eq!(set.stamps.len(), (STAMP_B * STAMP_B) as usize);
        assert!(set.pad >= 2, "pad は 2 以上にクランプされるはず");
    }

    /// 非有限座標(NaN/±Inf)の blit は何も描画せず panic しない
    /// (per-point の from_circle が非有限円を弾くのと同じ挙動)。
    #[test]
    fn blit_stamp_non_finite_coords_no_panic() {
        let set = build_stamp_set(&stamp_key(3.0, 1.0), 1.0);
        let mut pm = Pixmap::new(40, 40).unwrap();
        let before = pm.data().to_vec();
        blit_stamp(&mut pm, &set, f32::NAN, 20.0);
        blit_stamp(&mut pm, &set, 20.0, f32::INFINITY);
        blit_stamp(&mut pm, &set, f32::NEG_INFINITY, f32::NAN);
        // 巨大な有限座標(ix/iy が i32 飽和)でも桁あふれ panic しない(完全に画面外)。
        blit_stamp(&mut pm, &set, 1.0e38, 1.0e38);
        blit_stamp(&mut pm, &set, -1.0e38, 5.0);
        assert_eq!(
            pm.data(),
            &before[..],
            "非有限/巨大座標は何も描画しないはず"
        );
    }

    /// 巨大な有限座標でも pick_stamp の idx は stamps の範囲内(クランプで OOB 回避)。
    #[test]
    fn pick_stamp_huge_coords_index_in_range() {
        let set = build_stamp_set(&stamp_key(3.0, 1.0), 1.0);
        let (_, _, idx) = pick_stamp(&set, 1.0e9, 1.0e9);
        assert!(
            idx < (STAMP_B * STAMP_B) as usize,
            "idx は stamps(長さ b*b) の範囲内のはず"
        );
    }

    /// 128 点以上の line チャート(マーカー = 一様な fill-only circle)も stamp path を
    /// 踏み、参照と視覚的に同等(差分 2% 未満)。
    #[test]
    fn stamp_line_markers_within_tolerance() {
        let pts = (0..200)
            .map(|i| ((i * 37 + 13) % 100).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let labels = (0..200)
            .map(|i| format!("\"L{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{"type":"line","data":{{"labels":[{labels}],"datasets":[{{"label":"d","data":[{pts}]}}]}}}}"#
        );
        let scene = scene_for(&json);
        let f = face();
        let stamp = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, STAMP_MIN_RUN).unwrap();
        let reference = scene_to_pixmap_with(&scene, 1.0, &f, &PNG_LIMITS, usize::MAX).unwrap();

        assert_ne!(
            stamp.data(),
            reference.data(),
            "line マーカー(200 点)は stamp path を踏むはず"
        );
        let frac = diff_fraction(&stamp, &reference);
        assert!(
            frac < 0.02,
            "line マーカーの stamp 出力は参照と視覚的に同等(差分 {frac:.6} < 0.02)のはず"
        );
    }
}
