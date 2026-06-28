# WebP 出力フォーマット対応 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** PNG と同様に SVG を経由せず tiny-skia Pixmap から直接 WebP（ロスレス）を生成する。

**Architecture:** `raster_direct.rs` の `scene_to_png_with_face()` から Pixmap 生成部を `scene_to_pixmap()` として切り出し、WebP エンコードには `image` クレート（0.25）の `WebPEncoder::new_lossless()` を使う。CLI / Node / Ruby / WASM / Python の各バインディングにフォーマット文字列 `"webp"` を追加する。

**Tech Stack:** Rust, tiny-skia 0.11, image 0.25 (webp feature), ttf-parser 0.25

**Worktree:** `/home/ubuntu/fulgur-chart/.worktrees/feat/webp`

---

## Task 1: `image` クレートを依存に追加

**Files:**
- Modify: `Cargo.toml`（ワークスペースルート）
- Modify: `crates/fulgur-chart/Cargo.toml`

**Step 1: ワークスペース `Cargo.toml` に追加**

`[workspace.dependencies]` セクションに以下を追加する:

```toml
image = { version = "0.25", default-features = false, features = ["webp"] }
```

**Step 2: `crates/fulgur-chart/Cargo.toml` に追加**

`[dependencies]` セクションに追加する:

```toml
image = { workspace = true }
```

**Step 3: ビルド確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/webp
cargo build -p fulgur-chart 2>&1
```
Expected: コンパイル成功（`image` クレートが解決・ダウンロードされる）

**Step 4: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/webp
git add Cargo.toml Cargo.lock crates/fulgur-chart/Cargo.toml
git commit -m "chore: add image crate dependency for WebP encoding"
```

---

## Task 2: `raster_direct.rs` に `scene_to_pixmap()` を切り出す

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs`

現在の `scene_to_png_with_face()` は Pixmap 生成とエンコードが一体化している。WebP と PNG で Pixmap 生成を共有するために切り出す。

**Step 1: `scene_to_pixmap()` を追加**

`scene_to_png_with_face()` の直前に以下の内部関数を追加する。`MAX_PNG_AREA_PIXELS` チェックと Pixmap 確保・全描画を行う:

```rust
/// Scene を RGBA Pixmap にラスタライズする。PNG/WebP 共通。
fn scene_to_pixmap(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
) -> Result<Pixmap, String> {
    let scale = if scale > 0.0 { scale } else { 1.0 };

    let w = (scene.width as f32 * scale).round().max(1.0) as u32;
    let h = (scene.height as f32 * scale).round().max(1.0) as u32;

    let area = w as u64 * h as u64;
    if area > MAX_PNG_AREA_PIXELS {
        return Err(format!(
            "出力解像度 {w}×{h} px ({area} ピクセル) が上限 {MAX_PNG_AREA_PIXELS} px² を超えています"
        ));
    }

    let mut pixmap =
        Pixmap::new(w, h).ok_or_else(|| format!("Pixmap 確保失敗: 寸法 {w}x{h} が無効です"))?;

    let transform = Transform::from_scale(scale, scale);
    let mut glyph_cache: HashMap<ttf_parser::GlyphId, Option<tiny_skia::Path>> = HashMap::new();

    for prim in &scene.items {
        render_prim(&mut pixmap, prim, transform, face, &mut glyph_cache);
    }

    Ok(pixmap)
}
```

**Step 2: `scene_to_png_with_face()` を `scene_to_pixmap()` を使うように書き換える**

既存の `scene_to_png_with_face()` を以下に置き換える（scale のフォールバック処理と重複ロジックを削除し、`scene_to_pixmap()` に委譲する）:

```rust
fn scene_to_png_with_face(
    scene: &Scene,
    scale: f32,
    face: &ttf_parser::Face<'_>,
) -> Result<Vec<u8>, String> {
    let pixmap = scene_to_pixmap(scene, scale, face)?;
    pixmap
        .encode_png()
        .map_err(|e| format!("PNG エンコード失敗: {e}"))
}
```

**Step 3: テストが通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/webp
cargo test -p fulgur-chart 2>&1
```
Expected: 全テスト PASS（挙動変更なし）

**Step 4: コミット**

```bash
git add crates/fulgur-chart/src/raster_direct.rs
git commit -m "refactor: extract scene_to_pixmap() for PNG/WebP sharing"
```

---

## Task 3: `render_chart_to_webp()` を実装してテストを追加

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs`

**Step 1: テストを先に書く（TDD）**

`raster_direct.rs` のテストモジュール（`mod tests`）に追加する:

```rust
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
    assert!(err.unwrap_err().contains("上限"));
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart rasterizes_to_valid_webp 2>&1
```
Expected: `error[E0425]: cannot find function 'render_chart_to_webp'`

**Step 3: `render_chart_to_webp()` を実装する**

ファイル冒頭の `use` に `image` クレートを追加する:

```rust
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder};
```

そして公開エントリポイントセクションに以下を追加する:

```rust
/// ChartSpec を WebP バイト列に直接ラスタライズする（ロスレス）。
///
/// PNG と同様に SVG を経由しない。決定論性（同一入力 → 同一出力）を保証する。
///
/// **注意:** tiny-skia Pixmap は premultiplied alpha を使用するが、fulgur-chart は
/// 常に不透明な背景（A=255）を描画するため、全画素で premultiplied == straight alpha。
pub fn render_chart_to_webp(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let face =
        ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("フォント解析失敗: {e}"))?;
    let measurer =
        crate::text::TextMeasurer::new(font_bytes).map_err(|e| format!("計測初期化失敗: {e}"))?;
    let scene = crate::layout::build_scene(spec, &measurer);
    let pixmap = scene_to_pixmap(&scene, scale, &face)?;

    let mut buf = Vec::new();
    WebPEncoder::new_lossless(&mut buf)
        .write_image(
            pixmap.data(),
            pixmap.width(),
            pixmap.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("WebP エンコード失敗: {e}"))?;
    Ok(buf)
}
```

> **API 確認:** `image` 0.25 の `WebPEncoder` の正確なメソッド名は
> `cargo doc -p fulgur-chart --open` か `cargo check` で確認すること。
> `new_lossless` が存在しない場合は `new(writer)` や `new_with_quality(writer, 100.0)` を試す。

**Step 4: テストが通ることを確認**

```bash
cargo test -p fulgur-chart 2>&1
```
Expected: 全テスト PASS（新規テスト含む）

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/raster_direct.rs
git commit -m "feat: add render_chart_to_webp() lossless via image crate"
```

---

## Task 4: CLI に `--format webp` を追加

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs`

**Step 1: テストを先に書く**

`crates/fulgur-chart-cli/tests/cli.rs` に追加する:

```rust
#[test]
fn renders_to_webp_stdout() {
    // --format webp のとき、stdout 先頭バイトが RIFF シグネチャ。
    let out = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["render", "-", "-o", "-", "--format", "webp"])
        .write_stdin(BAR_JSON)
        .output()
        .unwrap();
    assert!(out.status.success(), "exit: {:?}\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    assert_eq!(&out.stdout[0..4], b"RIFF");
    assert_eq!(&out.stdout[8..12], b"WEBP");
}

#[test]
fn renders_bar_to_webp_file() {
    let dir = out_dir("renders_bar_to_webp_file");
    let out = dir.join("out.webp");
    let status = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["render", "-", "-o", out.to_str().unwrap()])
        .write_stdin(BAR_JSON)
        .status()
        .unwrap();
    assert!(status.success());
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}
```

`BAR_JSON` 定数が CLI テストに存在しない場合は既存の inline JSON を使うか定数を追加する。

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart-cli renders_to_webp_stdout 2>&1
```
Expected: テスト失敗 or `--format webp` が `error: invalid value 'webp'` で終了

**Step 3: `Format` enum に `Webp` を追加**

`main.rs` の `Format` enum を更新する:

```rust
#[derive(Clone, ValueEnum)]
enum Format {
    Svg,
    Png,
    Webp,
}
```

**Step 4: `detect_format()` に `.webp` 拡張子を追加**

```rust
fn detect_format(output: &str) -> Format {
    if output == "-" {
        return Format::Svg;
    }
    let ext = std::path::Path::new(output).extension();
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        Format::Png
    } else if ext.is_some_and(|e| e.eq_ignore_ascii_case("webp")) {
        Format::Webp
    } else {
        Format::Svg
    }
}
```

**Step 5: バッチモードの拡張子 match を更新**

```rust
let ext = match format {
    Format::Svg => "svg",
    Format::Png => "png",
    Format::Webp => "webp",
};
```

**Step 6: `render_one()` の match に `Webp` アームを追加**

```rust
Format::Webp => {
    let fb = font_bytes
        .as_deref()
        .unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    fulgur_chart::raster_direct::render_chart_to_webp(&spec_ir, args.scale, fb)
        .map_err(|e| (3, format!("error: WebP conversion failed: {e}")))
}
```

**Step 7: テストが通ることを確認**

```bash
cargo test --workspace 2>&1
```
Expected: 全テスト PASS

**Step 8: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): add --format webp with auto-detection from .webp extension"
```

---

## Task 5: Node バインディングに webp を追加

**Files:**
- Modify: `crates/bindings/node/src/lib.rs`

**Step 1: `Output` enum と `RenderResult` 構造体に webp を追加**

`Output` enum:
```rust
enum Output {
    Svg(String),
    Png(Vec<u8>),
    Webp(Vec<u8>),
}
```

`RenderResult` 構造体（既存の `svg`/`png` フィールドの隣に追加）:
```rust
pub webp: Option<Buffer>,
```

コンストラクタを追加:
```rust
fn webp(b: Vec<u8>) -> Self {
    Self {
        ok: true,
        svg: None,
        png: None,
        webp: Some(b.into()),
        code: None,
        message: None,
    }
}
```

既存の `png: None` 初期化がある箇所に `webp: None` を追加。

**Step 2: `render_inner()` の format match に `"webp"` アームを追加**

```rust
"webp" => {
    let fb = font.as_deref().unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    let webp = fulgur_chart::raster_direct::render_chart_to_webp(&ir, scale, fb)
        .map_err(|e| (RENDER_ERROR, e))?;
    Ok(Output::Webp(webp))
}
```

エラーメッセージを更新:
```rust
format!("unsupported format '{other}' (supported: svg, png, webp)"),
```

**Step 3: `render()` の match に `Output::Webp` を追加**

```rust
Ok(Output::Webp(b)) => RenderResult::webp(b),
```

**Step 4: ビルド確認**

```bash
cargo build -p fulgur-chart-node 2>&1
```
Expected: コンパイル成功

**Step 5: コミット**

```bash
git add crates/bindings/node/src/lib.rs
git commit -m "feat(node): add webp format to render()"
```

---

## Task 6: Ruby バインディングに webp を追加

**Files:**
- Modify: `crates/bindings/ruby/ext/fulgur_chart/src/lib.rs`

**Step 1: `Rendered` enum に `Webp` を追加**

```rust
enum Rendered {
    Svg(String),
    Png(Vec<u8>),
    Webp(Vec<u8>),
}
```

**Step 2: `render_pure()` の format match に `"webp"` アームを追加**

```rust
"webp" => {
    let fb = font.as_deref().map(str::as_bytes).unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    let webp = fulgur_chart::raster_direct::render_chart_to_webp(ir, scale, fb)
        .map_err(|e| render_err(ruby, e))?;
    Ok(Rendered::Webp(webp))
}
```

エラーメッセージを更新:
```rust
format!("unsupported format '{other}' (supported: svg, png, webp)"),
```

**Step 3: `render()` の `match result` に `Rendered::Webp` を追加**

```rust
Ok(Rendered::Webp(webp)) => Ok(ruby.str_from_slice(&webp)), // ASCII-8BIT (BINARY) String
```

**Step 4: ビルド確認**

```bash
cargo build -p fulgur-chart-ruby 2>&1
```
Expected: コンパイル成功

**Step 5: コミット**

```bash
git add crates/bindings/ruby/ext/fulgur_chart/src/lib.rs
git commit -m "feat(ruby): add webp format to FulgurChart.render()"
```

---

## Task 7: WASM バインディングに webp を追加

**Files:**
- Modify: `crates/bindings/wasm/src/lib.rs`

**Step 1: `Output` enum と `RenderResult` 構造体に webp を追加**

Node バインディングと同様の変更。`RenderResult` に `webp: Option<Vec<u8>>` フィールドを追加。`Output::Webp(Vec<u8>)` enum variant を追加。

```rust
pub webp: Option<Vec<u8>>,
```

コンストラクタ:
```rust
fn ok_webp(b: Vec<u8>) -> Self {
    Self {
        ok: true,
        svg: None,
        png: None,
        webp: Some(b),
        code: None,
        message: None,
    }
}
```

既存の `png: None` がある `Self { ... }` リテラル全部に `webp: None` を追加。

**Step 2: `render_inner()` に `"webp"` アームを追加**

```rust
"webp" => {
    let fb = font.as_deref().map(|s| s.as_bytes()).unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    let webp = fulgur_chart::raster_direct::render_chart_to_webp(&ir, scale, fb)
        .map_err(|e| (RENDER_ERROR, e))?;
    Ok(Output::Webp(webp))
}
```

エラーメッセージを更新:
```rust
format!("unsupported format '{other}' (supported: svg, png, webp)"),
```

**Step 3: `render()` の match に `Output::Webp` を追加**

```rust
Ok(Output::Webp(b)) => RenderResult::ok_webp(b),
```

**Step 4: ビルド確認**（WASM は通常の cargo build で型チェックできる）

```bash
cargo build -p fulgur-chart-wasm 2>&1
```
Expected: コンパイル成功

**Step 5: コミット**

```bash
git add crates/bindings/wasm/src/lib.rs
git commit -m "feat(wasm): add webp format to render()"
```

---

## Task 8: Python バインディングに webp を追加

**Files:**
- Modify: `crates/bindings/python/src/lib.rs`

**Step 1: `render_image()` の format ガードを更新**

既存のコード:
```rust
if format != "png" {
    return Err(parse_error(format!(
        "サポートされていないフォーマット: '{format}'"
    )));
}
```

WebP に対応するように分岐を追加:
```rust
match format {
    "png" => {
        let png_data = fulgur_chart::raster_direct::render_chart_to_png(&spec, scale as f32, font_bytes)
            .map_err(|e| render_error(format!("PNG 変換失敗: {e}")))?;
        Ok(pyo3::types::PyBytes::new(py, &png_data))
    }
    "webp" => {
        let webp_data = fulgur_chart::raster_direct::render_chart_to_webp(&spec, scale as f32, font_bytes)
            .map_err(|e| render_error(format!("WebP 変換失敗: {e}")))?;
        Ok(pyo3::types::PyBytes::new(py, &webp_data))
    }
    other => Err(parse_error(format!(
        "サポートされていないフォーマット: '{other}' (supported: png, webp)"
    ))),
}
```

> **注意:** Python バインディングの `render_image` 関数の実際の構造（`scale` の型、`font_bytes` の取得方法）を確認してから書き換えること。既存コードのパターンを踏襲する。

**Step 2: ビルド確認**

```bash
cargo build -p fulgur-chart-python 2>&1
```
Expected: コンパイル成功（pyo3 等の依存がある場合は時間がかかる）

**Step 3: コミット**

```bash
git add crates/bindings/python/src/lib.rs
git commit -m "feat(python): add webp format to render_image()"
```

---

## 最終確認

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/webp
cargo test --workspace 2>&1
```
Expected: 全テスト PASS
