# Binding API Surface Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** バインディング共通APIサーフェス仕様を `docs/binding-api-contract.md` として作成し、beads issueのnotesフィールドに参照を記録する。

**Architecture:** Rustコア（`fulgur_chart::render`, `raster_direct`, `frontend`, `guard`）から4つの公開API関数・オプション型・エラー変換規約を抽出し、言語非依存の仕様書として文書化する。コードは書かない—成果物はマークダウンドキュメントとbeads issueの更新。

**Tech Stack:** `bd` CLI, Markdown, Rust core crate (参照のみ)

---

### Task 1: バインディングAPI仕様ドキュメントを作成する

**Files:**
- Create: `docs/binding-api-contract.md`

**Step 1: 仕様ドキュメントを書く**

ファイル `docs/binding-api-contract.md` を以下の内容で作成する。

```markdown
# Fulgur Chart — Binding API Contract

各言語バインディング（Python / Node.js / WASM / Ruby）が実装すべき最小・安定APIの仕様。
命名は各言語の慣用形（snake_case / camelCase / PascalCase）に従うが、**動作は本仕様に準拠する**。

---

## 1. 公開API（4関数）

### 1.1 `render_svg`

```
render_svg(spec_json: str, options?: RenderOptions) -> str
```

- `spec_json`: chart.js v4 または Vega-Lite DSL の JSON 文字列。
- 戻り値: UTF-8 エンコードされた SVG 文字列（`<?xml` ヘッダ無し、`<svg ...>` 始まり）。
- DSL は `options.dsl` が未指定の場合、`mark` キー → Vega-Lite、`type` キー → chart.js と自動判定する。

### 1.2 `render_image`

```
render_image(spec_json: str, format: str, options?: RenderOptions) -> bytes
```

- `spec_json`: render_svg と同じ。
- `format`: ラスタフォーマット。現時点では `'png'` のみサポート。未知の値は `ParseError`。
- 戻り値: 指定フォーマットのバイト列。
- 描画経路: tiny-skia 直接ラスタライズ（SVG 文字列を経由しない）。
  SVG 出力と画素単位では一致しないが、決定性は保証する。
- **拡張方針:** 将来 `'jpeg'` / `'webp'` 等を `format` に追加することで後方互換を維持できる。

### 1.3 `schema`

```
schema(dsl: 'chartjs' | 'vegalite') -> str
```

- 指定 DSL の JSON Schema を JSON 文字列として返す。
- `dsl` が `'chartjs'` または `'vegalite'` 以外の場合は `ParseError` を送出。

### 1.4 `version`

```
version() -> str
```

- `Cargo.toml` の `package.version` 値を文字列で返す。例: `"0.1.0"`

---

## 2. RenderOptions

| フィールド | 型 | デフォルト | 説明 |
|---|---|---|---|
| `width` | `float \| None` | `None` | チャート幅(px)をオーバーライド。`None` = spec の値を使用 |
| `height` | `float \| None` | `None` | チャート高さ(px)をオーバーライド |
| `scale` | `float` | `1.0` | ラスタスケール係数。`render_svg` では無視する |
| `strict` | `bool` | `False` | 未知キーを拒否するストリクトモード |
| `dsl` | `'chartjs' \| 'vegalite' \| None` | `None` | DSL 強制指定。`None` = 自動判定 |
| `font` | `bytes \| None` | `None` | TrueType フォントバイト列。`None` = バンドル済みフォント（Noto Sans JP）を使用 |

**制約:**
- `width` / `height` は 1 px 以上 32768 px 以下（`DEFAULT_MIN_DIMENSION_PX` / `DEFAULT_MAX_DIMENSION_PX`）。
- `scale` は正の有限値（0 以下・非有限は Rustコアが 1.0 にフォールバック）。
- `font` が無効な TrueType バイト列の場合は `ParseError` を送出。

---

## 3. エラー変換規約

Rust コアは `Result<_, String>` でエラー文字列を返す。CLI は終了コードで 3 種類に分類しており、
バインディングも同じ分類を例外型にマップする。

| 内部分類 | CLIコード | 原因 | Python | Node.js / WASM | Ruby |
|---|---|---|---|---|---|
| Input/Parse Error | 1 | 不正 JSON、パース失敗、未知 DSL、寸法制限超過 | `FulgurParseError(ValueError)` | `FulgurError` (code=`'PARSE_ERROR'`) | `Fulgur::ParseError < StandardError` |
| Strict Violation | 2 | ストリクトモードでの未知キー | `FulgurStrictError(FulgurParseError)` | `FulgurError` (code=`'STRICT_ERROR'`) | `Fulgur::StrictError < Fulgur::ParseError` |
| Render/IO Error | 3 | PNG 変換失敗、IO エラー | `FulgurRenderError(RuntimeError)` | `FulgurError` (code=`'RENDER_ERROR'`) | `Fulgur::RenderError < StandardError` |

**共通規則:**
- エラーメッセージは Rust の `String` をそのまま伝播させる（英語 / 日本語混在に注意）。
- コア内部の `unwrap` パニックはバインディング層でキャッチしてはならない（不変条件違反は開発者バグ）。

---

## 4. 決定性保証

- **同一 `spec_json` + 同一 `format` + 同一 `options` → 同一出力バイト列**（SVG・image ともに）。
- `font` の決定性: バイト列が同一であれば同一フォントとみなす（ファイルパスは関係しない）。
- SVG と image は **互いにバイト一致しない**（描画経路が異なるため）。
- 乱数・タイムスタンプ・環境変数は出力に影響しない。

---

## 5. 実装チェックリスト（各バインディング共通）

- [ ] `render_svg` / `render_image` が `RenderOptions` の全フィールドを Rust コアに渡す
- [ ] `render_image` が未知の `format` 値で `ParseError` を送出する
- [ ] `schema` が `'chartjs'` / `'vegalite'` 以外の値で `ParseError` を送出する
- [ ] 3 種類のエラーコードを正しい例外型にマップする
- [ ] `version()` が `Cargo.toml` 版のバージョン文字列を返す
- [ ] `render_image('png')` の戻り値が PNG マジックバイト (`\x89PNG`) で始まることをテストする
- [ ] `render_svg` の戻り値が `<svg` で始まることをテストする
- [ ] 同一入力で同一出力バイト列になる決定性テストを含める

---

## 6. 参照コード

| 関数 | Rust コアの実装 |
|---|---|
| render_svg (デフォルトフォント) | `fulgur_chart::render::render_chart(&spec)` |
| render_svg (カスタムフォント) | `fulgur_chart::render::render_chart_with_font(&spec, font_bytes)` |
| render_image (format='png') | `fulgur_chart::raster_direct::render_chart_to_png(&spec, scale, font_bytes)` |
| schema | `schemars::schema_for!(fulgur_chart::schema::ChartJsSpec)` |
| version | `fulgur_chart::version()` |
| DSL parse (chartjs) | `fulgur_chart::frontend::chartjs::parse(json, strict)` |
| DSL parse (vegalite) | `fulgur_chart::frontend::vegalite::parse(json, strict)` |
| 入力検証 | `fulgur_chart::guard::validate_spec(&spec, &InputLimits::default())` |
```

**Step 2: ファイルが作成されたことを確認する**

```bash
ls -la docs/binding-api-contract.md
```

Expected: ファイルが存在すること。

**Step 3: Commit**

```bash
git add docs/binding-api-contract.md
git commit -m "docs: add binding API contract for multi-language bindings"
```

---

### Task 2: beads issue の notes フィールドを更新する

**Files:** なし（beads CLIを使用）

**Step 1: beads issue の notes を更新する**

```bash
bd update fulgur-chart-ts1.1 --notes="仕様書: docs/binding-api-contract.md を参照。

【公開API4種】render_svg / render_image(format) / schema / version

【RenderOptions】width(f64?) / height(f64?) / scale(f32=1.0) / strict(bool=false) / dsl(str?) / font(bytes?)

【エラー変換】
- コード1(Parse/Input) → ParseError/ValueError/FulgurError{code:'PARSE_ERROR'}/ParseError
- コード2(Strict) → StrictError(ParseErrorのサブクラス)
- コード3(Render/IO) → RenderError/RuntimeError

【決定性】同一spec_json+options → 同一出力バイト列。SVGとPNGは互いに一致しない。
【バンドルフォント】Noto Sans JP。font=Noneのとき使用。"
```

**Step 2: 更新を確認する**

```bash
bd show fulgur-chart-ts1.1
```

Expected: notes フィールドが設定されていること。

---

### Task 3: 動作確認

**Step 1: 現行テストが通ることを確認する**

```bash
cargo test
```

Expected: 31 tests passed, 0 failed（コードは変更していないので変化なし）。
