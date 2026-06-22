# Node.js バインディング: builder 形式 API 設計

`fulgur-chart-ts1.3`(Node.js バインディング / napi-rs)を、Ruby バインディング（`docs/plans/2026-06-20-ruby-builder-api-design.md`）と同様の **builder 形式 API** で実装する。

## 背景・動機

- 当初の acceptance criteria は契約（`docs/binding-api-contract.md`）の関数 API（`renderSvg(spec)` / `renderPng(spec)`）を要求していた。
- maintainer の方針: Node.js は Ruby と同様の builder API（`build(spec).…​.render('svg')`）にする。描画は builder に一本化し、スタンドアロンの `renderSvg` / `renderPng` 関数は設けない（低レベル `render(spec, format, opts)` プリミティブは公開）。
- 契約は「命名は各言語の慣用形に従うが、**動作は本仕様に準拠する**」としており、builder は JS 慣用の構造的選択。描画・オプション・エラー・決定性の **動作は契約どおり** 維持する。
- エラー表面も Ruby/Python と揃え、単一 `FulgurError + code` ではなく **階層クラス**にする（後述 §エラーモデル）。WASM（`ts1.4`）も同方針に統一する前提（契約 §3 の Node.js/WASM 共有列を一括で書換え）。

## 公開 API

エントリ:
```js
const { build, render, schema, version,
        FulgurParseError, FulgurStrictError, FulgurRenderError } = require('@fulgur-rs/chart-node')

const b = build(specJson)   // specJson: chart.js/Vega-Lite の JSON 文字列（必須）
```

チェーン可能セッター（`this` を返す。builder は再利用可・複数回 render 可）:

| メソッド | 引数 | 動作 |
|---|---|---|
| `.width(w)` / `.height(h)` | number | spec の寸法を上書き（guard 前に適用） |
| `.scale(s)` | number | ラスタスケール（svg では無視） |
| `.dsl(d)` | `'chartjs'` / `'vegalite'` | 未指定は自動判定 |
| `.font(bytes)` | `Buffer` / `Uint8Array` | TTF/OTF。未指定はバンドル Noto Sans JP |
| `.strict(v = true)` | boolean | 未知キー拒否 |
| `.format(f)` | `'svg'` / `'png'` | 終端 `render` の既定フォーマット |

終端:
```js
b.render('svg')        // => string (UTF-8, "<svg" 始まり)
b.render('png')        // => Buffer (binary, "\x89PNG" 始まり)
b.format('png').render() // => Buffer
b.render()             // => svg（既定）
```

- フォーマット優先順: **`render` 引数 > `.format()` セッター > 既定 `'svg'`**
- 未知フォーマット → `FulgurParseError`
- 低レベルプリミティブ（builder が内部で呼ぶ。直接も可）:
  ```js
  render(specJson, 'png', { width: 800 })  // => Buffer
  render(specJson, 'svg')                  // => string
  ```
- メタ情報: `schema(dsl)` / `version()`（モジュール関数）

公開サーフェス（厳密ロック）:
```js
module.exports === { build, render, schema, version,
                     FulgurParseError, FulgurStrictError, FulgurRenderError }
```
※ Ruby の top-level `Fulgur` 衝突制約は **Node には非該当**（モジュールスコープ require のため、グローバル名前空間を汚さない）。

## アーキテクチャ

builder + エラークラスは純 JS、描画の重い処理は native の単一プリミティブ `render` に委譲（Ruby 踏襲）。

```text
crates/bindings/node/
  Cargo.toml                 # 独立 [workspace]、fulgur-chart は path 依存
  src/lib.rs                 # native: render / schema / version（public）
  index.js                   # build()/Builder + エラークラス + schema/version 再エクスポート（手書き）
  index.d.ts                 # 公開 builder API の型（overload 含む、手書き）
  package.json               # name: "@fulgur-rs/chart-node"、binaryName: "chart-node"、@napi-rs/cli、optionalDependencies
  __test__/builder.test.mjs  # node:test（Ruby test_builder.rb 移植）
```

- `@napi-rs/cli` には native のローダ/型を内部用（`binding.js` / `binding.d.ts`）として吐かせ、公開 `index.js` / `index.d.ts` は手書きで wrapping する。
- `fulgur-chart` は **path 依存**（Python 踏襲: `fulgur-chart = { path = "../../fulgur-chart" }`）。crate は root workspace から exclude 済み、空 `[workspace]` で独立。

### native（Rust, `src/lib.rs`）

- 公開: `render(spec_json, format, options?)`, `schema(dsl)`, `version()`
- `build_ir`（DSL 解決 → 非 strict parse → strict 再 parse → width/height override → guard）は Ruby の `ext/fulgur_chart/src/lib.rs` をそのまま移植。
- **エラー境界 = discriminated-return**（napi のカスタムエラーコード marshaling に賭けない）。分類は Rust の call-site で決定（font 非対称含む）し、結果オブジェクトで JS に返す:
  ```rust
  #[napi(object)]
  pub struct RenderResult {
    pub ok: bool,
    pub svg: Option<String>,   // ok && svg 経路
    pub png: Option<Buffer>,   // ok && png 経路
    pub code: Option<String>,  // 'PARSE_ERROR' | 'STRICT_ERROR' | 'RENDER_ERROR'
    pub message: Option<String>,
  }
  ```
  - native は **例外を投げない**。`schema` の未知 DSL も同方式で `ok:false, code:'PARSE_ERROR'` を返す。
  - `Buffer` がオブジェクトフィールドで扱いづらい場合は `Option<Vec<u8>>` で返し、wrapper 側で `Buffer.from` する。
- options は `#[napi(object)]` 構造体（`width?`/`height?`/`scale?`/`strict?`/`dsl?`/`font?`）。`font` は `Buffer`（→ Rust で `Vec<u8>`）。
- **代替案（採用しない／spike 次第の将来最適化）**: napi-rs が `#[napi]` fn の `Err` に **任意のカスタム `.code` 文字列**を載せられると検証できた場合は native-throws-with-code に切替可能。ただし baseline は discriminated-return とし、ドキュメント／実装はこれに賭けない。

### JS wrapper（`index.js`）

```js
const binding = require('./binding.js') // @napi-rs/cli 生成の native ローダ

class FulgurParseError extends Error { constructor(m){ super(m); this.name='FulgurParseError' } }
class FulgurStrictError extends FulgurParseError { constructor(m){ super(m); this.name='FulgurStrictError' } }
class FulgurRenderError extends Error { constructor(m){ super(m); this.name='FulgurRenderError' } }

function makeError(code, message) {
  switch (code) {
    case 'STRICT_ERROR': return new FulgurStrictError(message)
    case 'RENDER_ERROR': return new FulgurRenderError(message)
    default:             return new FulgurParseError(message) // 'PARSE_ERROR'
  }
}

// 低レベルプリミティブ（builder が内部で呼ぶ）
function render(specJson, format, options = {}) {
  const r = binding.render(specJson, format, options)
  if (!r.ok) throw makeError(r.code, r.message)
  return r.svg !== undefined ? r.svg : r.png // png は Buffer
}

class Builder {
  constructor(specJson) { this._spec = specJson; this._opts = {} }
  width(v)  { this._opts.width  = v; return this }
  height(v) { this._opts.height = v; return this }
  scale(v)  { this._opts.scale  = v; return this }
  dsl(v)    { this._opts.dsl    = v; return this }
  font(v)   { this._opts.font   = v; return this }
  strict(v = true) { this._opts.strict = v; return this }
  format(v) { this._opts.format = v; return this }
  render(fmt) {
    // undefined（引数なし）→ 未指定扱い。null/false/その他は転送 → 不正なら ParseError。
    const resolved = (fmt === undefined) ? (this._opts.format ?? 'svg') : fmt
    const { format, ...rest } = this._opts
    return render(this._spec, resolved, rest)
  }
}

function build(specJson) { return new Builder(specJson) }
function schema(dsl) { const r = binding.schema(dsl); if (!r.ok) throw makeError(r.code, r.message); return r.value }
function version() { return binding.version() }

module.exports = { build, render, schema, version,
                   FulgurParseError, FulgurStrictError, FulgurRenderError }
```

理由: builder/エラーロジックは JS で書くと自明・保守容易で、napi のカスタムエラーコード marshaling の不確実性を完全に回避できる。native は「IR 構築 + 描画 + 分類」の単一責務に縮小。ユーザー視点の API は契約どおり。

## エラーモデル

```text
FulgurParseError  extends Error
FulgurStrictError extends FulgurParseError   // StrictError instanceof FulgurParseError === true
FulgurRenderError extends Error
```

| 内部分類 | code | 原因 | 例外クラス |
|---|---|---|---|
| Input/Parse | `PARSE_ERROR` | 不正 JSON、パース失敗、未知 DSL、未知 format、寸法制限超過 | `FulgurParseError` |
| Strict Violation | `STRICT_ERROR` | strict モードでの未知キー | `FulgurStrictError` |
| Render/IO | `RENDER_ERROR` | ラスタ変換失敗、IO エラー | `FulgurRenderError` |

- **font 非対称性**（描画経路に忠実）: svg 経路の不正フォント → `FulgurParseError`、png 経路 → `FulgurRenderError`。
- エラーメッセージは Rust の `String` をそのまま伝播。**分類は call-site（Rust）で決定し、JS はメッセージを解析しない**（code → クラスの機械マップのみ）。

## 決定性

契約 §4 のとおり。同一 `specJson` + 同一 `format` + 同一 `options` → 同一出力バイト列。SVG と PNG は互いにバイト一致しない（描画経路が異なる）。`font` はバイト列同一なら同一フォント扱い。

## テスト（`node:test` 組込み、追加依存なし）

Ruby `test_builder.rb` を移植:
- `build(spec).render('svg')` → `<svg` 始まり / `typeof === 'string'`
- `.render('png')` → `Buffer.isBuffer` かつ PNG マジック（`\x89PNG`）始まり
- Vega-Lite 自動判定 / `.dsl('chartjs')` 強制で parser 切替（vegalite spec → `FulgurParseError`）
- フォーマット優先順（引数 > setter > 既定 svg）、`render(undefined)`→フォールバック、`render(null)`/`render(false)`→`FulgurParseError`
- `.width(1234).height(567)` が `width="1234"` / `height="567"` 反映、`.scale` で png 変化
- セッターが `this` を返す（チェーン）、builder 再利用が決定的、render 間で再設定可
- エラー: 不正 JSON・未知 DSL・未知 format・寸法超過 → `FulgurParseError`、strict 未知キー → `FulgurStrictError`、`StrictError instanceof FulgurParseError`
- font 非対称: svg→`FulgurParseError` / png→`FulgurRenderError`
- 低レベル `render(spec,'png',{width})` が builder の結果と一致
- **サーフェスロック**: `module.exports` のキー集合が厳密に `{build, render, schema, version, FulgurParseError, FulgurStrictError, FulgurRenderError}`

## CI / 配布（スコープ明示）

- **ts1.3 をクローズするもの**: `.github/workflows/ci.yml` に `node-binding`（build + smoke）ジョブを追加（`ruby-binding` を踏襲）。
  - Node 20 + `dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache`（workspaces: `crates/bindings/node`）
  - ext crate は root workspace 外なので **fmt / clippy をこのジョブで gate**(`cargo fmt --check` / `cargo clippy -- -D warnings`)
  - `npm ci` → `napi build`（または `npm run build`）→ `node --test`
- **別 issue へ deferred**: マルチプラットフォーム prebuild + npm publish は別途リリースワークフロー（`ruby-gem-release.yml` が別なのと同様）。今回は作らない。

## パッケージ命名

`@fulgur-rs` スコープ下で公開する。`@fulgur-rs/cli` は既に Fulgur(PDF) の「HTML to PDF CLI」が使用中のため、chart 側は `chart-*` 系で衝突を避ける。

- **Node binding（本 issue）**: `@fulgur-rs/chart-node`（`binaryName: "chart-node"`）。
  - prebuild（ts1.6）: napi 慣例で `@fulgur-rs/chart-node-{triple}`（例 `@fulgur-rs/chart-node-linux-x64-gnu`）。
  - 将来 WASM（ts1.4）は `@fulgur-rs/chart-wasm` と対称。
- **npx CLI（別 issue・別パッケージ）**: `@fulgur-rs/chart-cli`。binding(napi) とは独立し、既存 Rust CLI（`fulgur-chart-cli`, bin: `fulgur-chart`）のプリビルド実行バイナリを PDF の `@fulgur-rs/cli` と同方式で配布する。
  - メタパッケージ `@fulgur-rs/chart-cli`（`bin: { "fulgur-chart": "bin/fulgur-chart" }` + JS launcher のみ）。
  - optionalDependencies: `@fulgur-rs/chart-cli-{linux-x64,linux-x64-musl,linux-arm64,darwin-arm64,darwin-x64,win32-x64}`（各に Rust 実行バイナリ同梱）。
  - launcher は platform 検出（musl 判定込み）→ 該当 optionalDep の `bin/fulgur-chart` を `spawnSync(stdio:'inherit')`。`npx @fulgur-rs/chart-cli render spec.json -o out.svg` で利用。

## 横断的影響（blast radius）

実装時に併せて反映する（すべて maintainer が「拡張側」を選択済み）:

1. `docs/binding-api-contract.md` **§1**: Node.js/WASM に builder 形式の callout 追加（現状 Ruby のみ）。
2. `docs/binding-api-contract.md` **§3** 例外列: Node.js/WASM 共有列を `FulgurParseError` / `FulgurStrictError`（< ParseError）/ `FulgurRenderError` に書換え。
3. **ts1.3 acceptance criteria**: 現行の `renderSvg`/`renderPng` 要求 → builder ベースに更新（bd で更新済み）。
4. **ts1.4（WASM）**: builder + 階層クラス前提に設計制約が付く旨を bd に記録。
5. 横断決定を bd memory（`bd remember`）に記録。
