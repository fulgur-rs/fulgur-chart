# WASM バインディング (wasm-bindgen) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `crates/bindings/wasm` に wasm-bindgen の cdylib crate を新設し、ブラウザ / Node(wasm) から `build(spec).…render('svg'/'png')` の builder 形式 API で利用できる npm パッケージ `@fulgur-rs/chart-wasm` を作る（issue `fulgur-chart-ts1.4`）。

**Architecture:** Node.js バインディング（`crates/bindings/node`）を踏襲する。native（Rust, wasm-bindgen）は「IR 構築 + 描画 + エラー分類」の単一責務に縮小し、**例外を投げず** discriminated-return（`{ok, svg, png, code, message}`）を返す。builder API と階層エラークラス（`FulgurParseError` / `FulgurStrictError < ParseError` / `FulgurRenderError`）は手書きの JS/TS wrapper（`index.js` / `index.d.ts`）が担う。`render_inner` のロジックは Node の `src/lib.rs` をそのまま移植する。

**配布ターゲット（確定）:** `wasm-pack build --target web` の **単一ビルド**。利用前に一度だけ `await init()`（ブラウザ）／ `await init(bytes)`（Node、ファイル fetch 不可のためバイト列を渡す）が必要。チャート呼び出し自体（`build/render/schema/version`）は同期で、Node の builder 形式を踏襲する。

**Tech Stack:** Rust 2024 + `wasm-bindgen 0.2` / `wasm-pack 0.14` / コア `fulgur-chart`(path 依存) / `schemars` / `serde` / Node.js 20 の `node:test`（追加ランタイム依存なし）。

**前提（検証済み）:**
- コアは `wasm32-unknown-unknown` で `cargo check` 通過済み（PR: `feat/wasm-pure-rust-deps` / issue `fulgur-chart-v18`）。tiny-skia ラスタライズ（PNG）も wasm 上で動作する（`crates/fulgur-chart/tests/wasm_runtime.rs` が実証）。
- ローカル環境に `wasm32-unknown-unknown` ターゲットと `wasm-pack 0.14.0` 導入済み。

**既知のトレードオフ（非ブロッキング）:**
- コアの `DEFAULT_FONT`（Noto Sans JP OTF, 数 MB）が `include_bytes!` で `.wasm` に同梱されるため、生成 `.wasm` は数 MB になる。設計上バンドルは許容済み。将来「スリム／フォント差し替え専用ビルド」が必要なら別 issue 化する（Task 10）。

---

## 命名・参照規約（実装前に把握）

- crate 名 `fulgur-chart-wasm` → wasm-pack 生成物は `pkg/fulgur_chart_wasm.js` / `pkg/fulgur_chart_wasm_bg.wasm`（ハイフンはアンダースコアに変換される）。**Task 1 で実際の生成ファイル名を確認してから** wrapper の import パスを確定すること。
- パッケージ名 `@fulgur-rs/chart-wasm`（node binding `@fulgur-rs/chart-node` と対称。設計: `docs/plans/2026-06-22-node-builder-api-design.md` §パッケージ命名）。
- 踏襲元（熟読すること）:
  - `crates/bindings/node/src/lib.rs` — `render_inner` / DSL 検出 / エラー分類のロジック源
  - `crates/bindings/node/index.js` — builder + エラークラス + format 優先順
  - `crates/bindings/node/index.d.ts` / `__test__/types.ts` — 型定義
  - `crates/bindings/node/__test__/builder.test.mjs` / `fixtures.mjs` — テスト移植元
  - `docs/binding-api-contract.md` — 動作契約（DSL 自動判定 / RenderOptions / エラー分類 / 決定性 / font 非対称性）

---

## Task 1: crate scaffold + wasm-pack ビルド疎通（ツールチェーン & 生成ファイル名の確定）

**目的:** 最小 crate で `wasm-pack build --target web` が通ること、生成ファイル名を確認すること（後続の import パスを確定するための de-risk）。

**Files:**
- Create: `crates/bindings/wasm/Cargo.toml`
- Create: `crates/bindings/wasm/src/lib.rs`（この Task では `version` のみ）
- Create: `crates/bindings/wasm/.gitignore`

**Step 1: `Cargo.toml` を作成**

```toml
# Standalone workspace: this crate is excluded from the repo-root workspace
# (root Cargo.toml `exclude = ["crates/bindings"]`), mirroring the Node/Python bindings.
[workspace]

[package]
name = "fulgur-chart-wasm"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-bindgen = "0.2"
# Path dependency on the core crate (mirrors the Node/Python bindings).
fulgur-chart = { path = "../../fulgur-chart" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = { version = "1.2", features = ["preserve_order"] }
```

**Step 2: 最小 `src/lib.rs` を作成（version のみ）**

```rust
use wasm_bindgen::prelude::*;

/// Return the crate version string of the core (mirrors the CLI / other bindings).
#[wasm_bindgen]
pub fn version() -> String {
    fulgur_chart::version().to_string()
}
```

**Step 3: `.gitignore` を作成**（node binding を踏襲）

```gitignore
/target
/node_modules
# The crate path-depends on the in-repo core, whose version is bumped by release-plz on
# every release; a committed lock would go stale (and break `--locked` CI). Let it regenerate.
Cargo.lock

# wasm-pack output (regenerated in build / CI / prepublish):
/pkg
```

**Step 4: ビルド疎通を確認**

Run: `cd crates/bindings/wasm && wasm-pack build --target web --release`
Expected: ビルド成功。`pkg/` に `.js` / `_bg.wasm` / `.d.ts` が生成される。

**Step 5: 生成ファイル名を確認**

Run: `ls crates/bindings/wasm/pkg/`
Expected: `fulgur_chart_wasm.js`, `fulgur_chart_wasm_bg.wasm`, `fulgur_chart_wasm.d.ts`, `fulgur_chart_wasm_bg.wasm.d.ts`, `package.json` が存在する。
**もし名前が異なる場合**は実際の名前を記録し、Task 3 以降の import パス（`./pkg/<name>.js` / `./pkg/<name>_bg.wasm`）をその名前に合わせる。

**Step 6: `init` の呼び出し規約を生成 glue で実地確認（記憶で決め打ちしない）**

新しめの wasm-bindgen は `init({ module_or_path })`（オブジェクト形式）へ移行し、従来の位置引数 `init(bytes)` を deprecate しつつある。`wasm-bindgen = "0.2"` は現行版に追従するため、**どちらが生成されるかはビルドしないと分からない**。

Run: `sed -n '1,40p' crates/bindings/wasm/pkg/fulgur_chart_wasm.js && echo '--- dts ---' && cat crates/bindings/wasm/pkg/fulgur_chart_wasm.d.ts`
確認: `export default function init(...)` / `initSync(...)` の実シグネチャ（引数が `bytes` 直接か `{ module_or_path }` か）。
**この結果に合わせて Task 4 のテストと Task 8 README の `init(...)` 呼び出しを書くこと**（本 plan は位置引数 `init(bytes)` を仮定。オブジェクト形式なら `init({ module_or_path: bytes })` に置換）。これは「wrapper のバグに見えるが実は init 形式違い」で Task 4 を落とす唯一の地雷。

**Step 7: Commit**

```bash
git add crates/bindings/wasm/Cargo.toml crates/bindings/wasm/src/lib.rs crates/bindings/wasm/.gitignore
git commit -m "feat(wasm): scaffold wasm-bindgen crate (version primitive)"
```

---

## Task 2: native `src/lib.rs` 全実装（render / schema + discriminated-return）

**目的:** Node の `src/lib.rs` を wasm-bindgen に移植する。native は例外を投げず、結果オブジェクトを返す。

**Files:**
- Modify: `crates/bindings/wasm/src/lib.rs`

**設計差分（Node → wasm-bindgen）:**
- napi の `#[napi(object)] RenderOptions` 構造体の代わりに、native `render` は **位置引数で個別の Optional** を受ける（wasm-bindgen は JS オブジェクト → Rust 構造体の自動変換を持たないため）。オプションのアンパックは JS wrapper 側で行う。
- `RenderResult` / `SchemaResult` は `#[wasm_bindgen]` 構造体 + **明示的 getter** で公開する（`Option<Vec<u8>>` getter → JS では `Uint8Array | undefined`、`Option<String>` → `string | undefined`）。
- `font` は `Option<Vec<u8>>`（JS の `Uint8Array | undefined` をバイト忠実に受理）。

**Step 1: `src/lib.rs` を全置換**

```rust
use fulgur_chart::guard::{InputLimits, validate_spec};
use wasm_bindgen::prelude::*;

// --- error classification (by CALL SITE, never by parsing the message) ---
//
// The native layer NEVER throws: it returns a discriminated `RenderResult` and the JS
// wrapper maps `code` -> error class (FulgurParseError / StrictError / RenderError). This
// mirrors the Node binding and avoids constructing JS Error subclasses from Rust.
const PARSE_ERROR: &str = "PARSE_ERROR";
const STRICT_ERROR: &str = "STRICT_ERROR";
const RENDER_ERROR: &str = "RENDER_ERROR";

/// Discriminated render result. Exactly one of (svg, png) is set when `ok`; otherwise
/// (code, message) describe the failure. Exposed to JS via explicit getters so that
/// `png` surfaces as a `Uint8Array` (Vec<u8>) and the string fields as `string`.
#[wasm_bindgen]
pub struct RenderResult {
    ok: bool,
    svg: Option<String>,
    png: Option<Vec<u8>>,
    code: Option<String>,
    message: Option<String>,
}

#[wasm_bindgen]
impl RenderResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }
    #[wasm_bindgen(getter)]
    pub fn svg(&self) -> Option<String> {
        self.svg.clone()
    }
    /// `Uint8Array | undefined` on the JS side.
    #[wasm_bindgen(getter)]
    pub fn png(&self) -> Option<Vec<u8>> {
        self.png.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> Option<String> {
        self.code.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

impl RenderResult {
    fn ok_svg(s: String) -> Self {
        Self {
            ok: true,
            svg: Some(s),
            png: None,
            code: None,
            message: None,
        }
    }
    fn ok_png(b: Vec<u8>) -> Self {
        Self {
            ok: true,
            svg: None,
            png: Some(b),
            code: None,
            message: None,
        }
    }
    fn err(code: &str, message: String) -> Self {
        Self {
            ok: false,
            svg: None,
            png: None,
            code: Some(code.to_string()),
            message: Some(message),
        }
    }
}

// --- DSL detection + parse (mirrors the Node / Ruby bindings) ---

#[derive(serde::Deserialize)]
struct DslDetector {
    mark: Option<serde::de::IgnoredAny>,
    #[serde(rename = "type")]
    r#type: Option<serde::de::IgnoredAny>,
}

/// Infer DSL from spec JSON: `mark` key -> vegalite, `type` key -> chartjs, neither -> Err.
fn detect_dsl(json: &str) -> Result<&'static str, String> {
    let d: DslDetector = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    if d.mark.is_some() {
        return Ok("vegalite");
    }
    if d.r#type.is_some() {
        return Ok("chartjs");
    }
    Err("cannot auto-detect DSL: specify dsl: 'chartjs' or 'vegalite'".to_string())
}

/// Parse a spec JSON string to IR using the specified DSL.
fn parse_spec(json: &str, dsl: &str, strict: bool) -> Result<fulgur_chart::ir::ChartSpec, String> {
    match dsl {
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, strict),
        _ => fulgur_chart::frontend::chartjs::parse(json, strict), // "chartjs"
    }
}

enum Output {
    Svg(String),
    Png(Vec<u8>),
}

/// Build + validate the IR, then render. Mirrors the Node binding's `render_inner`.
/// Returns `(code, message)` on failure; classification is decided here, at the call site.
#[allow(clippy::too_many_arguments)]
fn render_inner(
    spec_json: &str,
    format: &str,
    width: Option<f64>,
    height: Option<f64>,
    scale: Option<f64>,
    strict: Option<bool>,
    dsl_opt: Option<String>,
    font: Option<&[u8]>,
) -> Result<Output, (&'static str, String)> {
    let strict = strict.unwrap_or(false);
    let scale = scale.unwrap_or(1.0) as f32;

    // 1. Resolve DSL: explicit OR auto-detect.
    let dsl: String = match dsl_opt {
        Some(d) => {
            if d != "chartjs" && d != "vegalite" {
                return Err((PARSE_ERROR, format!("unsupported DSL '{d}'")));
            }
            d
        }
        None => detect_dsl(spec_json)
            .map_err(|e| (PARSE_ERROR, e))?
            .to_string(),
    };

    // 2. Parse NON-strict -> IR (render from this).
    let mut ir = parse_spec(spec_json, &dsl, false).map_err(|e| (PARSE_ERROR, e))?;

    // 3. If strict, re-parse with strict=true (unknown key -> StrictError).
    if strict {
        parse_spec(spec_json, &dsl, true).map_err(|e| (STRICT_ERROR, e))?;
    }

    // 4. Apply width/height overrides BEFORE guard.
    if let Some(w) = width {
        ir.width = w;
    }
    if let Some(h) = height {
        ir.height = h;
    }

    // 5. Guard (failure -> ParseError).
    validate_spec(&ir, &InputLimits::default()).map_err(|e| (PARSE_ERROR, e))?;

    // 6. Render by format.
    match format {
        "svg" => {
            // Font present -> render_chart_with_font (Err -> ParseError on the SVG path);
            // else the bundled-font render.
            let svg = match font {
                Some(bytes) => fulgur_chart::render::render_chart_with_font(&ir, bytes)
                    .map_err(|e| (PARSE_ERROR, e))?,
                None => fulgur_chart::render::render_chart(&ir),
            };
            Ok(Output::Svg(svg))
        }
        "png" => {
            let fb: &[u8] = font.unwrap_or(fulgur_chart::font::DEFAULT_FONT);
            // Invalid font on the image path -> RenderError (the SVG path maps this to ParseError).
            let png = fulgur_chart::raster_direct::render_chart_to_png(&ir, scale, fb)
                .map_err(|e| (RENDER_ERROR, e))?;
            Ok(Output::Png(png))
        }
        other => Err((
            PARSE_ERROR,
            format!("unsupported format '{other}' (supported: svg, png)"),
        )),
    }
}

/// Low-level render primitive. Never throws; returns a discriminated `RenderResult`.
/// The JS `Builder` (`build(...)`) is the intended API and calls this under the hood.
/// Options are passed positionally (wasm-bindgen has no JS-object -> struct auto-map);
/// the JS wrapper unpacks its options object into these arguments.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render(
    spec_json: String,
    format: String,
    width: Option<f64>,
    height: Option<f64>,
    scale: Option<f64>,
    strict: Option<bool>,
    dsl: Option<String>,
    font: Option<Vec<u8>>,
) -> RenderResult {
    match render_inner(
        &spec_json,
        &format,
        width,
        height,
        scale,
        strict,
        dsl,
        font.as_deref(),
    ) {
        Ok(Output::Svg(s)) => RenderResult::ok_svg(s),
        Ok(Output::Png(b)) => RenderResult::ok_png(b),
        Err((code, message)) => RenderResult::err(code, message),
    }
}

/// Discriminated schema result (same never-throw convention as `RenderResult`).
#[wasm_bindgen]
pub struct SchemaResult {
    ok: bool,
    value: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[wasm_bindgen]
impl SchemaResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> Option<String> {
        self.value.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> Option<String> {
        self.code.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

/// Return the JSON Schema (compact JSON string) for the given DSL ("chartjs"/"vegalite").
/// Unknown DSL -> ParseError. Never throws.
#[wasm_bindgen]
pub fn schema(dsl: String) -> SchemaResult {
    let s = match dsl.as_str() {
        "chartjs" => schemars::schema_for!(fulgur_chart::schema::ChartJsSpec),
        "vegalite" => schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec),
        other => {
            return SchemaResult {
                ok: false,
                value: None,
                code: Some(PARSE_ERROR.to_string()),
                message: Some(format!(
                    "unsupported DSL '{other}' (supported: chartjs, vegalite)"
                )),
            };
        }
    };
    match serde_json::to_string(&s) {
        Ok(json) => SchemaResult {
            ok: true,
            value: Some(json),
            code: None,
            message: None,
        },
        Err(e) => SchemaResult {
            ok: false,
            value: None,
            code: Some(RENDER_ERROR.to_string()),
            message: Some(format!("schema serialization: {e}")),
        },
    }
}

/// Return the crate version string (mirrors the CLI / other bindings).
#[wasm_bindgen]
pub fn version() -> String {
    fulgur_chart::version().to_string()
}
```

**Step 2: wasm32 でビルドが通ることを確認**

Run: `cd crates/bindings/wasm && cargo build --target wasm32-unknown-unknown`
Expected: コンパイル成功（warning なし）。
- **もし `Option<Vec<u8>>` getter / 引数でエラーが出た場合**のフォールバック: getter は `#[wasm_bindgen(getter_with_clone)]` を struct 属性に付けて public フィールド化する方式に切替可（ただし上記の明示 getter が第一選択）。

**Step 3: wasm-pack build が通ることを確認**

Run: `cd crates/bindings/wasm && wasm-pack build --target web --release`
Expected: 成功。`pkg/fulgur_chart_wasm.d.ts` に `render` / `schema` / `version` / `RenderResult` / `SchemaResult` が出力される。

**Step 4: Commit**

```bash
git add crates/bindings/wasm/src/lib.rs
git commit -m "feat(wasm): implement native render/schema (discriminated-return)"
```

---

## Task 3: JS/TS wrapper（builder + エラークラス）+ パッケージ設定

**目的:** 手書き ESM wrapper で builder API と階層エラークラスを公開する。`init` は生成 glue の default export を再エクスポートする。

**Files:**
- Create: `crates/bindings/wasm/index.js`
- Create: `crates/bindings/wasm/index.d.ts`
- Create: `crates/bindings/wasm/package.json`
- Create: `crates/bindings/wasm/tsconfig.json`

> ⚠️ import パスの `fulgur_chart_wasm` は Task 1 Step 5 で確認した実際の生成名に合わせること。

**Step 1: `index.js` を作成**

```js
// Hand-written public wrapper (ESM). The native primitive lives in the wasm-pack
// generated glue (./pkg/fulgur_chart_wasm.js). This layer adds the builder API and the
// error-class hierarchy, mirroring the Node binding (pure-JS builder over a single
// native `render` primitive).
//
// `--target web`: the wasm must be instantiated once via the default-exported `init`
// before any call. In a browser `await init()` fetches the bundled .wasm; in Node (no
// file fetch) pass the bytes via the object form:
// `await init({ module_or_path: await readFile(wasmUrl) })`.
// (The current wasm-bindgen glue uses the object form; positional `init(bytes)` still
// works but logs a deprecation warning.)
import init, {
  render as nativeRender,
  schema as nativeSchema,
  version as nativeVersion,
} from './pkg/fulgur_chart_wasm.js'

export default init

// --- error hierarchy (mirrors Node/Ruby/Python: StrictError is a ParseError subclass) ---

export class FulgurParseError extends Error {
  constructor(message) {
    super(message)
    this.name = 'FulgurParseError'
  }
}

export class FulgurStrictError extends FulgurParseError {
  constructor(message) {
    super(message)
    this.name = 'FulgurStrictError'
  }
}

export class FulgurRenderError extends Error {
  constructor(message) {
    super(message)
    this.name = 'FulgurRenderError'
  }
}

// Map the native discriminant `code` -> error class (mechanical; no message parsing).
function makeError(code, message) {
  switch (code) {
    case 'STRICT_ERROR':
      return new FulgurStrictError(message)
    case 'RENDER_ERROR':
      return new FulgurRenderError(message)
    default: // 'PARSE_ERROR'
      return new FulgurParseError(message)
  }
}

// --- low-level render primitive (the builder calls this; also callable directly) ---

export function render(specJson, format, options) {
  const o = options ?? {}
  // Coerce the format to a string before crossing the wasm boundary (mirrors Node's
  // String(format)): non-string values like null/false become "null"/"false", which the
  // native layer rejects as an unsupported format (ParseError). The Builder resolves
  // `undefined` to a fallback first. Options are unpacked positionally.
  const r = nativeRender(
    specJson,
    String(format),
    o.width,
    o.height,
    o.scale,
    o.strict,
    o.dsl,
    o.font,
  )
  // `r` is a wasm-bindgen class instance (a handle into wasm linear memory), NOT a plain
  // object like napi's result. The getters copy svg/png out into JS-owned values, after
  // which the handle must be `free()`d — otherwise it lingers until GC runs the
  // FinalizationRegistry, piling up wasm-side allocations in a render loop.
  try {
    if (!r.ok) {
      throw makeError(r.code, r.message)
    }
    // Read each field out of wasm memory once (a getter clones; the unused one is
    // undefined). Exactly one of svg/png is set on success; png is a Uint8Array.
    const svg = r.svg
    const png = r.png
    return svg != null ? svg : png
  } finally {
    r.free()
  }
}

// --- fluent, reusable builder (setters mutate and return `this`) ---

class Builder {
  constructor(specJson) {
    this._spec = specJson
    this._opts = {}
  }

  width(value) {
    this._opts.width = value
    return this
  }

  height(value) {
    this._opts.height = value
    return this
  }

  scale(value) {
    this._opts.scale = value
    return this
  }

  dsl(value) {
    this._opts.dsl = value
    return this
  }

  font(bytes) {
    this._opts.font = bytes
    return this
  }

  strict(value = true) {
    this._opts.strict = value
    return this
  }

  format(value) {
    this._opts.format = value
    return this
  }

  // Format precedence: explicit argument > .format() setter > default 'svg'.
  // Presence is tested with `in` (not `??`) so `.format(null).render()` matches
  // `render(null)` (an explicit invalid value -> ParseError) rather than rendering svg.
  render(format) {
    const resolved =
      format !== undefined ? format : 'format' in this._opts ? this._opts.format : 'svg'
    const { format: _ignored, ...rest } = this._opts
    return render(this._spec, resolved, rest)
  }
}

export function build(specJson) {
  return new Builder(specJson)
}

export function schema(dsl) {
  // Coerce to a string before the wasm boundary (like render): non-string values become
  // e.g. "null" and are rejected as an unsupported DSL (FulgurParseError) instead of a raw
  // wasm-bindgen conversion error.
  const r = nativeSchema(String(dsl))
  try {
    if (!r.ok) {
      throw makeError(r.code, r.message)
    }
    return r.value
  } finally {
    r.free() // wasm-bindgen handle; free after copying `value` out (see render()).
  }
}

export function version() {
  return nativeVersion()
}
```

**Step 2: `index.d.ts` を作成**

```ts
// Public type definitions for the fulgur-chart WASM binding (builder API).
// Hand-written: the generated pkg/*.d.ts only types the low-level native primitive.

export type Dsl = 'chartjs' | 'vegalite'
export type Format = 'svg' | 'png'

/** Render options. All fields optional; omitted fields use the spec / core defaults. */
export interface RenderOptions {
  /** Chart width (px). Overrides the spec value. */
  width?: number
  /** Chart height (px). Overrides the spec value. */
  height?: number
  /** Raster scale factor. Ignored when rendering SVG. Default 1.0. */
  scale?: number
  /** Reject unknown keys (strict mode). Default false. */
  strict?: boolean
  /** Force the input DSL. Omit to auto-detect (`mark` -> vegalite, `type` -> chartjs). */
  dsl?: Dsl
  /** TrueType/OpenType font bytes. Omit to use the bundled Noto Sans JP. */
  font?: Uint8Array
}

/** Accepted wasm source for {@link init}. Kept to types available without the DOM lib. */
export type InitInput = Uint8Array | ArrayBuffer

/**
 * Instantiate the WebAssembly module. MUST be awaited once before any other call.
 * Browser: `await init()` (fetches the bundled .wasm). Node (no file fetch): pass the
 * bytes via the object form: `await init({ module_or_path: bytes })`.
 * Re-exported from the wasm-pack generated glue (`--target web`).
 */
export default function init(
  options?: { module_or_path: InitInput } | InitInput,
): Promise<unknown>

/** Input/parse failure: invalid JSON, parse error, unknown DSL/format, dimension limit. */
export declare class FulgurParseError extends Error {}
/** Strict-mode unknown-key violation. A subclass of {@link FulgurParseError}. */
export declare class FulgurStrictError extends FulgurParseError {}
/** Raster conversion / IO failure. */
export declare class FulgurRenderError extends Error {}

/**
 * Fluent, reusable builder. Setters mutate and return `this`; `render` may be called
 * multiple times and the builder may be reconfigured between calls.
 *
 * Type-only interface (the runtime value is constructed via {@link build}, never exported).
 */
export interface Builder {
  width(value: number): this
  height(value: number): this
  scale(value: number): this
  dsl(value: Dsl): this
  font(bytes: Uint8Array): this
  strict(value?: boolean): this
  format(value: Format): this
  /**
   * Render to the given format. Precedence: explicit argument > `.format()` setter >
   * default `'svg'`. `'svg'` returns a string and `'png'` a Uint8Array; a no-argument
   * call depends on the `.format()` state, so it is typed `string | Uint8Array`.
   */
  render(format: 'svg'): string
  render(format: 'png'): Uint8Array
  render(format?: Format): string | Uint8Array
}

/** Start a builder for the given chart.js v4 / Vega-Lite DSL JSON string. */
export declare function build(specJson: string): Builder

/**
 * Low-level render primitive (the builder calls this; also callable directly).
 * Unknown format -> {@link FulgurParseError}.
 */
export declare function render(specJson: string, format: 'svg', options?: RenderOptions): string
export declare function render(
  specJson: string,
  format: 'png',
  options?: RenderOptions,
): Uint8Array
export declare function render(
  specJson: string,
  format: Format,
  options?: RenderOptions,
): string | Uint8Array

/** Return the JSON Schema (as a JSON string) for the given DSL. Unknown DSL -> ParseError. */
export declare function schema(dsl: Dsl): string

/** Return the crate version string. */
export declare function version(): string
```

**Step 3: `package.json` を作成**

```json
{
  "name": "@fulgur-rs/chart-wasm",
  "version": "0.1.0",
  "description": "Deterministic chart.js v4 / Vega-Lite JSON to SVG/PNG renderer (WebAssembly)",
  "type": "module",
  "main": "index.js",
  "module": "index.js",
  "types": "index.d.ts",
  "license": "MIT OR Apache-2.0",
  "files": [
    "index.js",
    "index.d.ts",
    "pkg/"
  ],
  "scripts": {
    "build": "wasm-pack build --target web --release",
    "test": "node --test",
    "typecheck": "tsc --noEmit -p tsconfig.json"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "typescript": "^5.0.0"
  }
}
```

**Step 4: `tsconfig.json` を作成**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "types": ["node"]
  },
  "include": ["index.d.ts", "__test__/types.ts"]
}
```

**Step 5: Commit**

```bash
git add crates/bindings/wasm/index.js crates/bindings/wasm/index.d.ts crates/bindings/wasm/package.json crates/bindings/wasm/tsconfig.json
git commit -m "feat(wasm): builder API wrapper + types + package config"
```

---

## Task 4: builder スモークテスト（`node:test`、Node binding 移植）

**目的:** 公開 API（wrapper + wasm）をエンドツーエンドで検証する。Node binding の `builder.test.mjs` を移植し、(1) `await init(bytes)` のセットアップ、(2) PNG が `Buffer` でなく `Uint8Array` で返る点、に適応する。

**Files:**
- Create: `crates/bindings/wasm/__test__/fixtures.mjs`
- Create: `crates/bindings/wasm/__test__/builder.test.mjs`

**Step 1: `__test__/fixtures.mjs` を作成**

```js
// Shared spec fixtures (mirrors the Node binding's fixtures.mjs).
export const BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
export const LINE = '{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,3,2]}]}}'
export const VEGALITE_BAR =
  '{"mark":"bar","data":{"values":[{"a":"x","b":1}]},"encoding":{"x":{"field":"a"},"y":{"field":"b"}}}'
// PNG magic \x89PNG as a plain Uint8Array (wasm returns Uint8Array, not Buffer).
export const PNG_MAGIC = Uint8Array.of(0x89, 0x50, 0x4e, 0x47)
```

**Step 2: `__test__/builder.test.mjs` を作成**（Node binding 版を移植。差分: init セットアップ + Uint8Array アサーション）

```js
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

import init, {
  build,
  render,
  schema,
  version,
  FulgurParseError,
  FulgurStrictError,
  FulgurRenderError,
} from '../index.js'
import { BAR, LINE, VEGALITE_BAR, PNG_MAGIC } from './fixtures.mjs'

// --target web: instantiate once before any call. Node has no file fetch, so pass bytes
// via the object form (positional init(bytes) is deprecated by the current glue).
const wasmUrl = new URL('../pkg/fulgur_chart_wasm_bg.wasm', import.meta.url)
await init({ module_or_path: await readFile(fileURLToPath(wasmUrl)) })

const isU8 = (v) => v instanceof Uint8Array
const bytesEqual = (a, b) =>
  isU8(a) && isU8(b) && a.length === b.length && Buffer.compare(Buffer.from(a), Buffer.from(b)) === 0
const startsWithPngMagic = (b) => isU8(b) && b.length >= 4 && bytesEqual(b.subarray(0, 4), PNG_MAGIC)

// --- meta ---

test('version() returns a semver string', () => {
  assert.equal(typeof version(), 'string')
  assert.match(version(), /^\d+\.\d+\.\d+/)
})

// --- rendering ---

test("build(spec).render('svg') returns an SVG string", () => {
  const out = build(BAR).render('svg')
  assert.equal(typeof out, 'string')
  assert.ok(out.startsWith('<svg'), `expected <svg, got ${out.slice(0, 20)}`)
})

test("build(spec).render('png') returns a PNG Uint8Array", () => {
  const out = build(BAR).render('png')
  assert.ok(isU8(out), 'expected a Uint8Array')
  assert.ok(startsWithPngMagic(out), 'expected PNG magic bytes')
})

test('vegalite is auto-detected', () => {
  assert.ok(build(VEGALITE_BAR).render('svg').startsWith('<svg'))
})

// --- format precedence: argument > .format() setter > default 'svg' ---

test("default format is 'svg'", () => {
  assert.ok(build(BAR).render().startsWith('<svg'))
})

test('.format() setter is used when no argument', () => {
  assert.ok(startsWithPngMagic(build(BAR).format('png').render()))
})

test('render argument overrides .format() setter', () => {
  const out = build(BAR).format('png').render('svg')
  assert.ok(out.startsWith('<svg'), "render('svg') must win over .format('png')")
})

test('undefined argument falls back to setter or default', () => {
  assert.ok(build(BAR).render(undefined).startsWith('<svg'))
  assert.ok(startsWithPngMagic(build(BAR).format('png').render(undefined)))
})

test('explicit null/false format is invalid, not silently rendered', () => {
  assert.throws(() => build(BAR).render(null), FulgurParseError)
  assert.throws(() => build(BAR).render(false), FulgurParseError)
  assert.throws(() => build(BAR).format('png').render(null), FulgurParseError)
})

test('an explicitly stored format(null) is forwarded, not defaulted to svg', () => {
  assert.throws(() => build(BAR).format(null).render(), FulgurParseError)
})

// --- chainable setters: width/height/scale/dsl/strict ---

test('width/height override', () => {
  const big = build(BAR).width(1234).height(567).render('svg')
  assert.ok(big.includes('width="1234"'))
  assert.ok(big.includes('height="567"'))
})

test('scale changes the png output', () => {
  const a = build(BAR).scale(1.0).render('png')
  const b = build(BAR).scale(2.0).render('png')
  assert.ok(!bytesEqual(a, b), 'scale should change the rasterized output')
})

test('dsl override switches the parser', () => {
  assert.ok(build(VEGALITE_BAR).render('svg').startsWith('<svg'))
  assert.throws(() => build(VEGALITE_BAR).dsl('chartjs').render('svg'), FulgurParseError)
})

// --- builder is reusable; setters chain; renders are deterministic ---

test('setters return this for chaining', () => {
  const b = build(BAR)
  assert.equal(b.width(800), b)
  assert.equal(b.strict(false), b)
})

test('builder reuse is deterministic', () => {
  const b = build(BAR)
  assert.equal(b.render('svg'), b.render('svg'))
  assert.ok(bytesEqual(b.render('png'), b.render('png')))
})

test('builder is reconfigurable between renders', () => {
  const b = build(BAR)
  const small = b.width(400).render('svg')
  const big = b.width(1234).render('svg')
  assert.ok(small.includes('width="400"'))
  assert.ok(big.includes('width="1234"'))
})

// --- errors (call-site classification preserved) ---

test('unknown format raises ParseError', () => {
  assert.throws(() => build(BAR).render('zzz'), FulgurParseError)
})

test('invalid JSON raises ParseError', () => {
  assert.throws(() => build('not json').render('svg'), FulgurParseError)
})

test('undetectable DSL raises ParseError', () => {
  assert.throws(() => build('{"labels":[]}').render('svg'), FulgurParseError)
})

test('unknown dsl raises ParseError', () => {
  assert.throws(() => build(BAR).dsl('zzz').render('svg'), FulgurParseError)
})

test('strict mode unknown key raises StrictError', () => {
  const spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
  assert.throws(() => build(spec).strict().render('svg'), FulgurStrictError)
})

test('StrictError is a ParseError subclass', () => {
  const spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
  assert.throws(() => build(spec).strict().render('svg'), FulgurParseError)
  assert.ok(FulgurStrictError.prototype instanceof FulgurParseError)
})

test('dimension over the limit raises ParseError', () => {
  assert.throws(() => build(BAR).width(40000).render('svg'), FulgurParseError)
})

// font-error asymmetry: SVG path -> ParseError, image path -> RenderError
test('invalid font on the svg path raises ParseError', () => {
  assert.throws(() => build(BAR).font(Uint8Array.of(1, 2, 3, 4)).render('svg'), FulgurParseError)
})

test('invalid font on the image path raises RenderError', () => {
  assert.throws(() => build(BAR).font(Uint8Array.of(1, 2, 3, 4)).render('png'), FulgurRenderError)
})

// --- low-level render primitive (the builder calls it) ---

test('direct render primitive', () => {
  assert.ok(render(BAR, 'svg').startsWith('<svg'))
  assert.ok(startsWithPngMagic(render(BAR, 'png')))
  assert.ok(render(BAR, 'svg', { width: 800 }).includes('width="800"'))
})

test('direct render equals builder render', () => {
  assert.ok(bytesEqual(render(BAR, 'png', { width: 640 }), build(BAR).width(640).render('png')))
  assert.equal(render(LINE, 'svg'), build(LINE).render('svg'))
})

// --- schema / version meta functions ---

test('schema(dsl) returns JSON schema strings', () => {
  assert.ok(JSON.parse(schema('chartjs')))
  assert.ok(JSON.parse(schema('vegalite')))
})

test('schema unknown dsl raises ParseError', () => {
  assert.throws(() => schema('zzz'), FulgurParseError)
})

test('schema with a non-string dsl raises ParseError (never a raw wasm error)', () => {
  assert.throws(() => schema(null), FulgurParseError)
  assert.throws(() => schema(false), FulgurParseError)
})

// --- public surface lock ---

test('public surface is exactly the documented exports', async () => {
  // index.js is ESM; introspect via dynamic import. The default export (init) is part of
  // the public surface, so the locked key set is the 7 named exports + 'default'.
  const pkg = await import('../index.js')
  assert.deepEqual(
    Object.keys(pkg).sort(),
    [
      'default', // init
      'FulgurParseError',
      'FulgurRenderError',
      'FulgurStrictError',
      'build',
      'render',
      'schema',
      'version',
    ].sort(),
  )
})
```

> ⚠️ **init を公開サーフェスに含める判断:** `init`(default export) は WASM 利用に必須なので公開サーフェスの一部と位置づけ、ロック集合を **8 キー**（名前付き 7 + `default`）とする。`await import()` でも `require()`(Node の require-of-ESM) でも `default` キーが現れる。

**Step 3: テスト実行（ビルド済み前提）**

Run: `cd crates/bindings/wasm && npm install && npm run build && npm test`
Expected: 全テスト PASS。
- `npm install` で `typescript` / `@types/node` を取得。
- `npm run build` で `pkg/` を生成（テストが `pkg/..._bg.wasm` を読むため必須）。

**Step 4: Commit**

```bash
git add crates/bindings/wasm/__test__/
git commit -m "test(wasm): port builder smoke tests (init + Uint8Array)"
```

---

## Task 5: 型チェック（`tsc`）+ `__test__/types.ts`

**目的:** `index.d.ts` の overload を型レベルで固定する（Node binding の `types.ts` 移植、`Buffer` → `Uint8Array`）。

**Files:**
- Create: `crates/bindings/wasm/__test__/types.ts`

**Step 1: `__test__/types.ts` を作成**

```ts
// Type-level checks for index.d.ts (compiled with `tsc --noEmit`, never executed).
import init, {
  build,
  render,
  schema,
  version,
  FulgurParseError,
  FulgurStrictError,
  FulgurRenderError,
} from '../index.js'

const SPEC = '{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}'

// init is the default export and returns a Promise (no-arg + object form both type-check).
const p: Promise<unknown> = init()
const p2: Promise<unknown> = init({ module_or_path: new Uint8Array() })

// Builder return-type overloads.
const a: string = build(SPEC).width(800).height(600).dsl('chartjs').strict().render('svg')
const b: Uint8Array = build(SPEC).scale(2).format('png').render('png')
// No-argument render() depends on the .format() state, so it is typed string | Uint8Array.
const c: string | Uint8Array = build(SPEC).render()
const cPng: Uint8Array = build(SPEC).format('png').render('png')

// Low-level primitive overloads.
const d: string = render(SPEC, 'svg', { width: 800 })
const e: Uint8Array = render(SPEC, 'png')

// Meta.
const f: string = schema('chartjs')
const g: string = version()

// Error classes are Errors; StrictError is assignable to ParseError.
const h: Error = new FulgurRenderError('x')
const i: FulgurParseError = new FulgurStrictError('x')

// @ts-expect-error png returns Uint8Array, not string
const wrong1: string = build(SPEC).render('png')
// @ts-expect-error unknown dsl is rejected
build(SPEC).dsl('zzz')
// @ts-expect-error unknown format is rejected
render(SPEC, 'jpeg')

void [p, p2, a, b, c, cPng, d, e, f, g, h, i, wrong1]
```

**Step 2: 型チェック実行**

Run: `cd crates/bindings/wasm && npm run typecheck`
Expected: エラー 0（`@ts-expect-error` が全て期待どおり機能）。

**Step 3: Commit**

```bash
git add crates/bindings/wasm/__test__/types.ts
git commit -m "test(wasm): type-level checks for index.d.ts overloads"
```

---

## Task 6: CI ジョブ追加（`wasm-binding`）

**目的:** `.github/workflows/ci.yml` に node-binding を踏襲した `wasm-binding` ジョブを追加。fmt/clippy ゲート（root workspace 外のため）+ wasm-pack build + typecheck + node:test。

**Files:**
- Modify: `.github/workflows/ci.yml`（`node-binding` ジョブの直後、`perf` ジョブの前に追加）

**Step 1: 既存の `wasm` ジョブ（コア wasm32 テスト, line 118–140）と `node-binding` ジョブ（line 238–282）を読み、構造を確認**

**Step 2: `node-binding` ジョブの直後に以下を追加**

```yaml
  wasm-binding:
    name: WASM binding (build + smoke)
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: crates/bindings/wasm
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
          targets: wasm32-unknown-unknown

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: crates/bindings/wasm
          key: wasm-binding

      - uses: actions/setup-node@v4
        with:
          node-version: "20"

      - uses: taiki-e/install-action@wasm-pack

      # The native crate is excluded from the root workspace, so the root fmt/clippy jobs
      # don't cover it — gate it here. No --locked: the crate's Cargo.lock is gitignored
      # (the path-dependent core version is bumped by release-plz each release).
      - name: Format check
        run: cargo fmt --manifest-path Cargo.toml -- --check

      - name: Clippy
        run: cargo clippy --manifest-path Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings

      - name: Install npm deps
        run: npm install

      - name: Build wasm package
        run: npm run build

      - name: Typecheck (index.d.ts)
        run: npm run typecheck

      - name: Test
        run: npm test
```

**Step 3: ローカルで各ステップ相当を再現確認**

Run（worktree ルートから）:
```bash
cd crates/bindings/wasm
cargo fmt --manifest-path Cargo.toml -- --check
cargo clippy --manifest-path Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings
npm install && npm run build && npm run typecheck && npm test
```
Expected: 全て成功。

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add wasm-binding job (build + smoke)"
```

---

## Task 7: 契約ドキュメント更新（横断的影響）

**目的:** `docs/binding-api-contract.md` を WASM の builder + 階層エラークラス方針に合わせる（node design doc の blast radius §1・§3。node binding 実装時に未反映のまま残っている）。

**Files:**
- Modify: `docs/binding-api-contract.md`

**Step 1: §1 冒頭の callout 群（Ruby / Python の後）に Node.js / WASM の builder callout を追加**

挿入する内容（既存 Ruby/Python callout と同じスタイルの blockquote）:

```markdown
> **Node.js / WASM の API 形態:** 両バインディングは **builder 形式**で公開する
> （`build(specJson).width(…).dsl('chartjs').render('svg')`、`render('png')` でラスタ）。
> 低レベル `render(specJson, format, options?)` プリミティブも公開し、`schema(dsl)` / `version()`
> はモジュール関数。エラーは単一 `FulgurError + code` ではなく **階層クラス**
> （`FulgurParseError` / `FulgurStrictError < ParseError` / `FulgurRenderError`）で公開する。
> **動作（DSL自動判定・RenderOptions・エラー分類・決定性・フォント非対称性）は本仕様に準拠する。**
> WASM は `wasm-pack --target web` 配布のため利用前に一度 `await init()` が必要
> （Node では `await init({ module_or_path: bytes })`）。詳細: `docs/plans/2026-06-22-node-builder-api-design.md`。
```

**Step 2: §3 のエラー変換表の「Node.js / WASM」列を階層クラスに書き換える**

- Input/Parse 行: `FulgurError (code='PARSE_ERROR')` → `FulgurParseError`
- Strict 行: `FulgurError (code='STRICT_ERROR')` → `FulgurStrictError < FulgurParseError`
- Render 行: `FulgurError (code='RENDER_ERROR')` → `FulgurRenderError`

（内部的に native は `code` discriminant を返し、JS wrapper が code → クラスへマップする点を表下に一文補足してよい。）

**Step 3: 確認**

Run: `grep -n "FulgurError\|builder\|init(bytes)" docs/binding-api-contract.md`
Expected: §3 に旧 `FulgurError (code=...)` が残っていない（Node.js/WASM 列）。§1 に WASM callout がある。

**Step 4: Commit**

```bash
git add docs/binding-api-contract.md
git commit -m "docs(contract): Node/WASM builder callout + hierarchy error classes"
```

---

## Task 8: README（WASM 利用ガイド）

**目的:** init の必要性（ブラウザ vs Node）が非自明なため、最小の利用ガイドを置く。

**Files:**
- Create: `crates/bindings/wasm/README.md`

**Step 1: `README.md` を作成**

````markdown
# @fulgur-rs/chart-wasm

Deterministic chart.js v4 / Vega-Lite JSON → SVG/PNG renderer, compiled to WebAssembly.

Built with `wasm-pack --target web`: **you must `await init()` once before any call.**

## Browser

```js
import init, { build } from '@fulgur-rs/chart-wasm'

await init() // fetches the bundled .wasm once
const spec = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
const svg = build(spec).width(800).render('svg') // string
const png = build(spec).render('png') // Uint8Array
```

## Node.js

Node has no file `fetch`, so pass the wasm bytes to `init`:

```js
import init, { build } from '@fulgur-rs/chart-wasm'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const wasmUrl = new URL(
  '../node_modules/@fulgur-rs/chart-wasm/pkg/fulgur_chart_wasm_bg.wasm',
  import.meta.url,
)
await init({ module_or_path: await readFile(fileURLToPath(wasmUrl)) })

const svg = build('{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}').render('svg')
```

## API

- `build(specJson)` → Builder: `.width/.height/.scale/.dsl/.font/.strict/.format` (chainable) → `.render('svg')` (string) / `.render('png')` (Uint8Array)
- `render(specJson, format, options?)` — low-level primitive
- `schema('chartjs' | 'vegalite')` → JSON Schema string
- `version()` → core version string

Errors: `FulgurParseError`, `FulgurStrictError` (`< FulgurParseError`), `FulgurRenderError`.

Behavior (DSL auto-detection, options, error classification, determinism, font asymmetry)
follows `docs/binding-api-contract.md`.
````

> import パス内の `fulgur_chart_wasm_bg.wasm` は Task 1 で確認した実名に合わせること。

**Step 2: Commit**

```bash
git add crates/bindings/wasm/README.md
git commit -m "docs(wasm): usage README (browser + node init)"
```

---

## Task 9: 最終検証（全ゲート再実行）

**目的:** PR 前に acceptance criteria を満たすことをまとめて確認する。

**REQUIRED SUB-SKILL:** `superpowers:verification-before-completion`

**Step 1: バインディングの全ゲートを通す**

```bash
cd crates/bindings/wasm
cargo fmt --manifest-path Cargo.toml -- --check
cargo clippy --manifest-path Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings
npm install
npm run build
npm run typecheck
npm test
```
Expected: すべて成功。`npm test` で `render('svg')` → string、`render('png')` → Uint8Array、schema/version が検証される（= acceptance criteria）。

**Step 2: コア側のリグレッションが無いことを確認**

```bash
# worktree ルートから
cargo check --workspace --locked --target wasm32-unknown-unknown
```
Expected: 成功（コアは無変更だが、念のため）。

**Step 3: git status が意図どおりか確認**

Run: `git status`
Expected: `crates/bindings/wasm/{Cargo.toml,src/lib.rs,index.js,index.d.ts,package.json,tsconfig.json,.gitignore,README.md,__test__/*}`、`.github/workflows/ci.yml`、`docs/binding-api-contract.md`、`docs/plans/2026-06-23-wasm-binding.md` のみ。`pkg/` / `target/` / `node_modules/` / `Cargo.lock` は ignore されている。

---

## Task 10: フォローアップ issue（任意・非ブロッキング）

実装中に判明した将来作業を beads に起票する（クローズ条件ではない）。

- **npm 配布**: `@fulgur-rs/chart-wasm` のタグ駆動 publish ワークフロー（node の ts1.6 / `node-npm-release.yml` と同様に CI とは分離）。本 plan のスコープ外。
- **スリムビルド**: `DEFAULT_FONT` 同梱で `.wasm` が数 MB になる。フォント差し替え必須の「no-default-font」build を feature flag で提供する案。
- **ブラウザ実機テスト**: 現状 CI は `node:test`(Node 上の wasm) のみ。`wasm-pack test --headless --chrome` 等でブラウザ実機スモークを足す案。
- **ハンドル寿命テスト**: `RenderResult`/`SchemaResult` の `r.free()`(wasm linear memory のハンドル解放) が render ループ下でリークしないことは現状未テスト。napi 版に対応物が無い wasm 固有挙動。回帰検知が要るなら専用テストを追加する案（品質レビュー指摘・必須ではない）。

```bash
bd create --title="@fulgur-rs/chart-wasm の npm 配布 (タグ駆動 publish)" --type=task --priority=3 --description="ts1.4 完了後。wasm-pack --target web の成果物(index.js/index.d.ts/pkg/)を npm publish するリリースワークフロー。node の ts1.6 と対称。" 
# 他 2 件も同様に起票（必要に応じて）
```

---

## 完了条件（acceptance criteria 対応表）

| acceptance | 充足する Task |
|---|---|
| `wasm-pack build` が通る | Task 1, 2（build 疎通）/ Task 9 |
| ブラウザ/Node(wasm) で `render_svg` が文字列 | Task 4（`render('svg')` → string）/ README Task 8 |
| `render_png` が `Uint8Array` を返す | Task 2（native `Vec<u8>` getter）/ Task 4（`isU8` アサート） |
| CI で wasm32 ビルド + スモークテストが通る | Task 6（`wasm-binding` ジョブ）/ Task 9 |
| builder 形式 + 階層エラークラス（notes 制約） | Task 2, 3, 4 / 契約 Task 7 |
