# 不透明背景時に PNG/WebP repack の alpha scan を省略 実装計画

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 全面不透明背景を持つチャートで、PNG/WebP エンコード前の全画素 demultiply スキャンを丸ごと省き、単発 PNG レイテンシを ~0.4ms 削減する（`fulgur-chart-a7c`）。

**Architecture:** 背景 rect は非 AA（中心サンプリング, `raster_direct.rs:625`）で描かれ部分αを作らない。その上に source-over で描く全内容は opaque dest 上で α==255 を保つ。よって「全面不透明背景 かつ pixmap が背景 device 矩形に完全内包」される場合、pixmap の premultiplied バッファは全画素で straight と一致する。既存 `demultiply_in_place`（部分α画素 `0<a<255` のみ書き換え）を**呼ばずにスキップ**でき、`pixmap.data()` をそのままエンコーダへ渡せる（追加確保なし・byte 一致）。判定は 2 段: (1) semantic な `Scene::has_opaque_background()`、(2) encode 時の device 被覆ガード（scale が要るため）。両立時のみスキップ。fail-safe（判定 false は常に安全にスキャンへ fallback）。

**Tech Stack:** Rust 2024, tiny-skia 0.11, png 0.17, image(webp)。テストは crate 内 `#[cfg(test)]` と `tests/` 統合テスト。

**重要な設計判断（レビュー時に確認）:**
- design フィールドの「bare `to_vec()`」は旧 API（Vec 返却時代）の stale 用語。現行 in-place API では `to_vec()` を挟むと**逆に 1 フレーム分余計に確保**しピークメモリ回帰になる（`MAX_PNG_AREA_PIXELS` コメント `raster_direct.rs:29-31` の「ピークを 1 フレーム削減」を打ち消す）。よって実装は `demultiply_in_place` を**呼ばない 1 行分岐**のみ。
- Option 2（device 被覆ガード）を採用。「全面不透明背景」だけでは、frac(device 寸法)==0.5 の丸め上げ + AA 内容が canvas 端に届く病的ケースで silent に byte がずれる余地が残る（sweep が 0 だったのは既存 chart が pixmap 端に内容を置かない死角ゆえで、不変条件の証明ではない）。被覆ガードは算術のみで airtight にこれを排除する。
- 本番のホットパス（chart-server）は常に `scale=1.0`（`chart.rs:175`・`mcp.rs:293` でハードコード）、CLI も既定 1.0。整数 scale では被覆ガードは常に真 → **速度ロス皆無**。分数 `--scale` の丸め上げ時のみ正しい既存スキャンへ fallback（0.4ms 遅いだけ・出力は正しい）。
- 設計意図「Scene が不透明背景を知る」は、`Scene` に**メソッド** `has_opaque_background()` として持たせる（`items.first()` を検査）。フィールド追加だと ~30 の builder リテラル改修が必要になるが、メソッドなら churn ゼロで、公開 `scene_to_png` 経路でも同じく機能し、将来 index 0 規約が変わっても安全側(false)に劣化する。

---

## Task 1: `Scene::has_opaque_background()` を追加

**Files:**
- Modify: `crates/fulgur-chart/src/scene.rs`（`Scene` struct 定義直後に `impl Scene`）
- Test: 同ファイル `#[cfg(test)] mod tests`（新規追加）

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart/src/scene.rs` 末尾に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Color;

    fn full_rect(w: f64, h: f64, a: f32) -> Prim {
        Prim::Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
            fill: Color { r: 10, g: 20, b: 30, a },
        }
    }

    #[test]
    fn opaque_full_canvas_rect_is_opaque_background() {
        let s = Scene { width: 100.0, height: 50.0, items: vec![full_rect(100.0, 50.0, 1.0)] };
        assert!(s.has_opaque_background());
    }

    #[test]
    fn semi_transparent_bg_is_not_opaque() {
        let s = Scene { width: 100.0, height: 50.0, items: vec![full_rect(100.0, 50.0, 0.5)] };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn empty_scene_is_not_opaque() {
        let s = Scene { width: 100.0, height: 50.0, items: vec![] };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn partial_coverage_first_rect_is_not_opaque() {
        // 全幅に満たない先頭矩形は背景として扱わない。
        let s = Scene { width: 100.0, height: 50.0, items: vec![full_rect(80.0, 50.0, 1.0)] };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn non_rect_first_item_is_not_opaque() {
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Line {
                x1: 0.0, y1: 0.0, x2: 1.0, y2: 1.0,
                stroke: Color { r: 0, g: 0, b: 0, a: 1.0 },
                stroke_width: 1.0,
            }],
        };
        assert!(!s.has_opaque_background());
    }
}
```

**Step 2: テストが失敗することを確認**

Run: `cargo test -p fulgur-chart --lib scene::tests 2>&1 | tail -20`
Expected: コンパイルエラー `no method named has_opaque_background`

**Step 3: 最小実装**

`crates/fulgur-chart/src/scene.rs` の `pub struct Scene { ... }` 定義の直後に追加:

```rust
impl Scene {
    /// 最背面(items[0])が canvas 全面を覆う不透明 Rect のとき true。
    ///
    /// `build_scene` は `theme.background` 指定時に全面矩形を index 0 へ挿入するため、
    /// これは「不透明背景が敷かれている」ことと一致する。背景なし・半透明背景・部分被覆の
    /// 先頭矩形では false（＝最適化を適用せず安全側）。PNG/WebP エンコードで
    /// demultiply スキャンを省ける（全画素 α==255 を前提にできる）ための **必要条件**。
    /// 十分条件は encode 時に scale 依存の device 被覆判定と合成する。
    pub fn has_opaque_background(&self) -> bool {
        matches!(
            self.items.first(),
            Some(Prim::Rect { x, y, w, h, fill })
                if *x <= 0.0
                    && *y <= 0.0
                    && *x + *w >= self.width
                    && *y + *h >= self.height
                    && fill.a >= 1.0
        )
    }
}
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib scene::tests 2>&1 | tail -20`
Expected: PASS（5 tests）

**Step 5: build_scene が theme 背景で正しく true になる回帰（統合）**

`crates/fulgur-chart/tests/render_theme.rs` 末尾に追加:

```rust
#[test]
fn opaque_theme_background_marks_scene_opaque() {
    use fulgur_chart::layout::build_scene;
    use fulgur_chart::text::TextMeasurer;
    let m = TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT).unwrap();

    let opaque = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
          "options":{"theme":{"backgroundColor":"#ff00ff"}}}"##,
        false,
    ).unwrap();
    assert!(build_scene(&opaque, &m).has_opaque_background());

    let semi = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
          "options":{"theme":{"backgroundColor":"rgba(255,0,255,0.5)"}}}"##,
        false,
    ).unwrap();
    assert!(!build_scene(&semi, &m).has_opaque_background());

    let none = chartjs::parse(
        r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
        false,
    ).unwrap();
    assert!(!build_scene(&none, &m).has_opaque_background());
}
```

Run: `cargo test -p fulgur-chart --test render_theme opaque_theme_background 2>&1 | tail -10`
Expected: PASS。失敗時は `Color::a` の閾値や `build_scene` の挿入位置を確認。

**Step 6: Commit**

```bash
git add crates/fulgur-chart/src/scene.rs crates/fulgur-chart/tests/render_theme.rs
git commit -m "feat(scene): add has_opaque_background() predicate (a7c)"
```

---

## Task 2: PNG 経路で opaque 時に demultiply をスキップ

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs`
  - `all_pixels_opaque()` を新規追加（`demultiply_in_place` の近く）
  - `encode_png_fast()` に `skip_demultiply: bool` 引数を追加（`raster_direct.rs:381`）
  - `scene_to_png_with_face()` で判定を合成し引数を渡す（`raster_direct.rs:326-334`）
  - 既存テスト `encode_png_fast_matches_tiny_skia_byte_for_byte` の呼び出しに `false` を追加（`raster_direct.rs:1201`）
- Test: 同ファイル `#[cfg(test)] mod tests`

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart/src/raster_direct.rs` の test mod に追加（`bar_spec()` の近く）:

```rust
    /// 不透明背景の spec（bar_spec に白背景を付与）。
    fn opaque_bar_spec() -> crate::ir::ChartSpec {
        chartjs::parse(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"label":"売上","data":[10,20,30]}]},"options":{"theme":{"backgroundColor":"#ffffff"}}}"#,
            false,
        )
        .unwrap()
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
        assert!(all_pixels_opaque(&op, &pm2, 2.0), "opaque+整数(2x)は skip 可");

        // 丸め上げ: width 800 → 800*1.000625=800.5 → round 801 > 800.5 → 被覆保証不可。
        let pmf = scene_to_pixmap(&op, 1.000625, &face, &PNG_LIMITS).unwrap();
        assert!(!all_pixels_opaque(&op, &pmf, 1.000625), "丸め上げは skip 不可");

        let non = crate::layout::build_scene(&bar_spec(), &measurer);
        let pmn = scene_to_pixmap(&non, 1.0, &face, &PNG_LIMITS).unwrap();
        assert!(!all_pixels_opaque(&non, &pmn, 1.0), "非 opaque は skip 不可");
    }
```

**Step 2: テストが失敗することを確認**

Run: `cargo test -p fulgur-chart --lib raster_direct::tests::encode_png_fast_opaque 2>&1 | tail -20`
Expected: コンパイルエラー（`all_pixels_opaque` 未定義 / `encode_png_fast` の引数不一致）

**Step 3: 実装**

(3a) `demultiply_in_place`（`raster_direct.rs:349`）の直前に `all_pixels_opaque` を追加:

```rust
/// pixmap の全画素が α==255（部分α画素ゼロ）と **静的に保証**できるとき true。
///
/// 条件は 2 つ:
/// 1. `scene.has_opaque_background()` — 最背面に全面不透明 Rect が敷かれている。
///    背景 rect は非 AA（中心サンプリング）で描かれ部分αを作らず、その上の source-over は
///    opaque dest 上で α==255 を保つため、被覆された画素は必ず α==255 になる。
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
    (pixmap.width() as f32) <= scene.width as f32 * scale
        && (pixmap.height() as f32) <= scene.height as f32 * scale
}
```

(3b) `encode_png_fast`（`raster_direct.rs:381`）に引数追加。doc も更新:

```rust
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
```

(3c) `scene_to_png_with_face`（`raster_direct.rs:326-334`）で判定を渡す:

```rust
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
```

(3d) 既存テスト `encode_png_fast_matches_tiny_skia_byte_for_byte`（`raster_direct.rs:1201`）の呼び出しを更新:

```rust
        // bar_spec は背景なし → 非 opaque → skip=false（従来どおり全画素 demultiply）。
        let actual = encode_png_fast(&mut pixmap, PngCompression::Fast, false).unwrap();
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib raster_direct 2>&1 | tail -20`
Expected: 全 PASS（新規 2 + 既存 byte-exact テスト含む）

**Step 5: 既存 golden PNG が不変であること（本番 scale=1.0 の回帰）**

Run: `cargo test -p fulgur-chart --test golden_png 2>&1 | tail -15`
Expected: PASS（opaque 背景の golden があれば skip 経路で、なければ従来経路で、いずれもバイト不変）

**Step 6: Commit**

```bash
git add crates/fulgur-chart/src/raster_direct.rs
git commit -m "perf(png): skip alpha demultiply scan on opaque background (a7c)"
```

---

## Task 3: WebP 経路で opaque 時に demultiply をスキップ

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs`
  - WebP エンコードを `encode_pixmap_webp()` ヘルパへ抽出（`raster_direct.rs:197-206` を DRY 化）
  - `render_chart_to_webp()`（`raster_direct.rs:180-207`）で `all_pixels_opaque` により demultiply を分岐
- Test: 同ファイル `#[cfg(test)] mod tests`

**Step 1: 失敗するテストを書く**

test mod に追加:

```rust
    /// WebP も opaque 時に demultiply をスキップして byte 一致すること。
    /// 参照 = 全画素 demultiply 後にエンコード、実測 = スキップしてエンコード。
    #[test]
    fn webp_opaque_bg_skip_matches_full_demultiply_byte_for_byte() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        let measurer = crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = crate::layout::build_scene(&opaque_bar_spec(), &measurer);

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
```

**Step 2: テストが失敗することを確認**

Run: `cargo test -p fulgur-chart --lib raster_direct::tests::webp_opaque 2>&1 | tail -20`
Expected: コンパイルエラー `encode_pixmap_webp` 未定義

**Step 3: 実装**

(3a) `render_chart_to_webp` の直後あたりにヘルパを抽出:

```rust
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
```

(3b) `render_chart_to_webp`（`raster_direct.rs:191-206`）の本体を差し替え:

```rust
    // WebP 専用の上限(軸・面積)で pixmap 確保前に弾き OOM を防ぐ(→ WEBP_LIMITS)。
    let mut pixmap = scene_to_pixmap(&scene, scale, &face, &WEBP_LIMITS)?;

    // WebPEncoder は straight alpha を要求する。全画素 α==255 が保証される
    // (不透明背景・完全被覆)ときは premult==straight のため demultiply を省く。
    // それ以外は in-place で straight 化する(別バッファを確保しない)。
    if !all_pixels_opaque(&scene, &pixmap, scale) {
        demultiply_in_place(&mut pixmap);
    }

    encode_pixmap_webp(&pixmap)
```

（`use image::...` は既存のまま。既存の `let mut buf = Vec::new(); WebPEncoder...` ブロックは削除しヘルパへ移す。）

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib raster_direct::tests::webp 2>&1 | tail -20`
Expected: PASS

**Step 5: WebP 全体の回帰**

Run: `cargo test -p fulgur-chart --lib raster_direct 2>&1 | tail -15`
Expected: 全 PASS

**Step 6: Commit**

```bash
git add crates/fulgur-chart/src/raster_direct.rs
git commit -m "perf(webp): skip alpha demultiply scan on opaque background (a7c)"
```

---

## Task 4: 「不透明背景 → 部分αゼロ」不変条件の回帰テスト

**Files:**
- Create: `crates/fulgur-chart/tests/opaque_bg_no_partial_alpha.rs`

**目的:** 最適化の安全網。将来どの chart 種別が入っても、不透明背景では部分α画素(`0<a<255`)が出ないことを固定する。ここが崩れると skip 経路が byte 破壊するため、最適化の前提そのものを守る。CI 時間を抑えるため scale は代表値のみ。

**Step 1: テストを書く**

```rust
//! 不変条件: 不透明背景のチャートは部分α画素(0<a<255)を持たない。
//! これは opaque skip 最適化(a7c)の前提であり、崩れると PNG/WebP が byte 破壊する。
use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png;
use tiny_skia::Pixmap;

fn partial_alpha_count(png: &[u8]) -> usize {
    let pm = Pixmap::decode_png(png).unwrap();
    pm.pixels().iter().filter(|p| { let a = p.alpha(); a != 0 && a != 255 }).count()
}

#[test]
fn opaque_background_produces_no_partial_alpha() {
    let cases = [
        r##"{"type":"bar","data":{"labels":["a","b","c"],"datasets":[{"data":[3,1,2]}]},
          "options":{"theme":{"backgroundColor":"#ff00ff"}}}"##,
        r##"{"type":"line","data":{"labels":["a","b","c","d"],"datasets":[{"data":[1,3,2,4]}]},
          "options":{"theme":{"backgroundColor":"#00aa88"}}}"##,
        r##"{"type":"pie","data":{"labels":["a","b","c"],"datasets":[{"data":[3,1,2]}]},
          "options":{"theme":{"backgroundColor":"#123456"}}}"##,
    ];
    // 整数・分数・丸め上げ(800*1.000625=800.5)を含む代表 scale。
    let scales = [1.0f32, 2.0, 1.5, 1.000625];
    for json in &cases {
        let spec = chartjs::parse(json, false).unwrap();
        for &scale in &scales {
            let png = render_chart_to_png(&spec, scale, fulgur_chart::font::DEFAULT_FONT).unwrap();
            assert_eq!(
                partial_alpha_count(&png), 0,
                "不透明背景で部分α画素が出た (scale={scale}, json={json})"
            );
        }
    }
}
```

**Step 2: テストを実行**

Run: `cargo test -p fulgur-chart --test opaque_bg_no_partial_alpha 2>&1 | tail -10`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/fulgur-chart/tests/opaque_bg_no_partial_alpha.rs
git commit -m "test: pin opaque-bg zero-partial-alpha invariant (a7c)"
```

---

## Task 5: 品質ゲートと最終検証

**Files:** なし（検証のみ）

**Step 1: crate 全体のテスト**

Run: `cargo test -p fulgur-chart 2>&1 | tail -25`
Expected: 全 PASS（golden / snapshot 含む）

**Step 2: chart-server / CLI の回帰（scale=1.0 経路が壊れていないこと）**

Run: `cargo test --workspace 2>&1 | tail -25`
Expected: 全 PASS

**Step 3: clippy**

Run: `cargo clippy --workspace --all-targets 2>&1 | tail -25`
Expected: 警告・エラーなし（`float_cmp` は既存慣例どおり該当箇所で発火しないこと）

**Step 4: fmt**

Run: `cargo fmt --all --check 2>&1 | tail -10`
Expected: 差分なし（あれば `cargo fmt --all` で整形しコミット）

**Step 5: エンコード時間の目視確認（任意・効果測定）**

既存 bench があれば opaque 背景ケースで確認。なければスキップ可（本 task の合否条件ではない）:

Run: `cargo test -p fulgur-chart --test bench_cases 2>&1 | tail -15` もしくは既存 membench/bench の該当ケース。
Expected: opaque 経路で PNG エンコードが短縮（回帰でないこと）。

**Step 6: 検証完了の宣言前に superpowers:verification-before-completion を使用**

全ゲート PASS を実出力で確認してから完了とする。

**Step 7: 最終 Commit（fmt 差分等が残っていれば）**

```bash
git add -A
git commit -m "chore: fmt/clippy cleanup for opaque-bg skip (a7c)"
```

---

## 完了時チェックリスト

- [ ] Task 1–5 の全コミット済み
- [ ] `cargo test --workspace` 全 PASS
- [ ] `cargo clippy --workspace --all-targets` クリーン
- [ ] `cargo fmt --all --check` 差分なし
- [ ] 既存 golden/snapshot に**意図しない差分がない**（byte 一致維持の確認）
- [ ] REQUIRED SUB-SKILL: superpowers:finishing-a-development-branch でブランチ処理
- [ ] `bd close fulgur-chart-a7c`（ユーザー確認後）
