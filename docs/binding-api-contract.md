# Fulgur Chart — Binding API Contract

各言語バインディング（Python / Node.js / WASM / Ruby）が実装すべき最小・安定APIの仕様。
命名は各言語の慣用形（snake_case / camelCase / PascalCase）に従うが、**動作は本仕様に準拠する**。

---

## 1. 公開API（4関数）

### 1.1 `render_svg`

render_svg(spec_json: str, options?: RenderOptions) -> str

- `spec_json`: chart.js v4 または Vega-Lite DSL の JSON 文字列。
- 戻り値: UTF-8 エンコードされた SVG 文字列（`<?xml` ヘッダ無し、`<svg ...>` 始まり）。
- DSL は `options.dsl` が未指定の場合、`mark` キー → Vega-Lite、`type` キー → chart.js と自動判定する。

### 1.2 `render_image`

render_image(spec_json: str, format: str, options?: RenderOptions) -> bytes

- `spec_json`: render_svg と同じ。
- `format`: ラスタフォーマット。未知の値は Parse Error 系の例外（セクション3参照）を送出する。
  サポートする `format` 値:
  - `'png'` — PNG (現在唯一のサポート値)
- 戻り値: 指定フォーマットのバイト列。
- 描画経路: tiny-skia 直接ラスタライズ（SVG 文字列を経由しない）。
  SVG 出力と画素単位では一致しないが、決定性は保証する。
- **拡張方針:** 将来 `'jpeg'` / `'webp'` 等を `format` に追加することで後方互換を維持できる。

### 1.3 `schema`

schema(dsl: 'chartjs' | 'vegalite') -> str

- 指定 DSL の JSON Schema を JSON 文字列として返す。
- `dsl` が `'chartjs'` または `'vegalite'` 以外の場合は Parse Error 系の例外（セクション3参照）を送出。
- 返す JSON Schema の内容はライブラリバージョンと紐付いており、バージョン間の互換性は保証しない。キャッシュする場合は `version()` の値と合わせて管理すること。

### 1.4 `version`

version() -> str

- `Cargo.toml` の `package.version` 値を文字列で返す。例: "0.1.0"

## 2. RenderOptions

| フィールド | 型 | デフォルト | 説明 |
|---|---|---|---|
| `width` | `float \| None` | `None` | チャート幅(px)をオーバーライド。`None` = spec の値を使用 |
| `height` | `float \| None` | `None` | チャート高さ(px)をオーバーライド |
| `scale` | `float` | `1.0` | ラスタスケール係数。`render_svg` では無視する |
| `strict` | `bool` | `False` | 未知キーを拒否するストリクトモード |
| `dsl` | `'chartjs' \| 'vegalite' \| None` | `None` | DSL 強制指定。`None` = 自動判定 |
| `font` | `bytes \| None` | `None` | TrueType/OpenType フォントバイト列。`None` = バンドル済みフォント（Noto Sans JP OTF）を使用 |

**制約:**
- `width` / `height` は **1 px 以上 32768 px 以下**（コア定数: `DEFAULT_MIN_DIMENSION_PX` / `DEFAULT_MAX_DIMENSION_PX`）。
- `render_image` では追加で **PNG 面積上限** がある: `width × height × scale²` が **64,000,000 ピクセル**（`MAX_PNG_AREA_PIXELS`）を超えると Render Error（CLIコード 3）。例: 32768×32768 は約 10 億 px で上限超過。
- `scale` は正の有限値。コアの実際の動作:
  - `0` 以下・`NaN`: Rustコアが 1.0 にフォールバック（`scene_to_png_with_face` 内 `if scale > 0.0`）
  - `+Infinity`: フォールバックせずそのまま計算 → `width × height × scale²` が面積上限（64M ピクセル）を超えて Render Error になる
  - バインディング側では `scale` のバリデーションを行わず、コアの動作に委ねる。
- `font` が無効な TrueType/OpenType バイト列の場合のエラー種別:
  - `render_svg` で無効フォント → Parse Error（CLIコード 1）
  - `render_image` で無効フォント → Render Error（CLIコード 3）
  - ※ 描画経路が異なるためエラーコードが異なる。`render_image` は `raster_direct` 経路のエラーを Render Error として分類する。

## 3. エラー変換規約

Rust コアは `Result<_, String>` でエラー文字列を返す。CLI は終了コードで 3 種類に分類しており、
バインディングも同じ分類を例外型にマップする。

| 内部分類 | CLIコード | 原因 | Python | Node.js / WASM | Ruby |
|---|---|---|---|---|---|
| Input/Parse Error | 1 | 不正 JSON、パース失敗、未知 DSL、寸法制限超過 | `FulgurParseError(ValueError)` | `FulgurError` (code=`'PARSE_ERROR'`) | `FulgurChart::ParseError < StandardError` |
| Strict Violation | 2 | ストリクトモードでの未知キー | `FulgurStrictError(FulgurParseError)` | `FulgurError` (code=`'STRICT_ERROR'`) | `FulgurChart::StrictError < FulgurChart::ParseError` |
| Render/IO Error | 3 | ラスタ変換失敗、IO エラー | `FulgurRenderError(RuntimeError)` | `FulgurError` (code=`'RENDER_ERROR'`) | `FulgurChart::RenderError < StandardError` |

> Ruby のモジュール名は `Fulgur` ではなく **`FulgurChart`**: top-level `Fulgur` は Fulgur(PDF) ライブラリと衝突するため、バインディングは `FulgurChart` 名前空間で公開する（`Fulgur` エイリアスは設けない）。

**共通規則:**
- エラーメッセージは Rust の `String` をそのまま伝播させる（英語 / 日本語混在に注意）。
- コア内部の `unwrap` パニックはバインディング層でキャッチしてはならない（不変条件違反は開発者バグ）。

## 4. 決定性保証

- **同一 `spec_json` + 同一 `format` + 同一 `options` → 同一出力バイト列**（SVG・image ともに）。
- `font` の決定性: バイト列が同一であれば同一フォントとみなす（ファイルパスは関係しない）。
- SVG と image は **互いにバイト一致しない**（描画経路が異なるため）。
- 乱数・タイムスタンプ・環境変数は出力に影響しない。
- **決定性の範囲:** 同一バイナリ・同一フォントバイト列・同一 OS/アーキテクチャでの保証。クロスプラットフォーム（x86_64 vs ARM64、OS 間など）での画素単位一致は保証しない。

## 5. 実装チェックリスト（各バインディング共通）

- [ ] `render_svg` / `render_image` が `RenderOptions` の各フィールドを適切にマッピングして Rust コアの処理に適用する
- [ ] `render_image` が未知の `format` 値で `ParseError` を送出する
- [ ] `schema` が `'chartjs'` / `'vegalite'` 以外の値で `ParseError` を送出する
- [ ] 3 種類のエラーコードを正しい例外型にマップする
- [ ] `version()` が `Cargo.toml` 版のバージョン文字列を返す
- [ ] `render_image('png')` の戻り値が PNG マジックバイト (`\x89PNG`) で始まることをテストする
- [ ] `render_svg` の戻り値が `<svg` で始まることをテストする
- [ ] 同一入力で同一出力バイト列になる決定性テストを含める

## 6. 参照コード

| 関数 | Rust コアの実装 |
|---|---|
| render_svg (デフォルトフォント) | `fulgur_chart::render::render_chart(&spec)` |
| render_svg (カスタムフォント) | `fulgur_chart::render::render_chart_with_font(&spec, font_bytes)` |
| render_image (format='png') | `fulgur_chart::raster_direct::render_chart_to_png(&spec, scale, font_bytes)` |
| schema (dsl='chartjs') | `schemars::schema_for!(fulgur_chart::schema::ChartJsSpec)` |
| schema (dsl='vegalite') | `schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec)` |
| version | `fulgur_chart::version()` |
| DSL parse (chartjs) | `fulgur_chart::frontend::chartjs::parse(json, strict)` |
| DSL parse (vegalite) | `fulgur_chart::frontend::vegalite::parse(json, strict)` |
| 入力検証 | `fulgur_chart::guard::validate_spec(&spec, &InputLimits::default())` |
