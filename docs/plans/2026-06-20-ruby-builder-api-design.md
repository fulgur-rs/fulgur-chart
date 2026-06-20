# Ruby バインディング: builder 形式 API 設計

PR #11（ts1.5）の Ruby バインディングを、Fulgur(PDF) と同様の **builder 形式 API** に再設計する。

## 背景・動機

- 当初は契約（`docs/binding-api-contract.md`）の関数API（`render_svg` / `render_image` / `schema` / `version`）をモジュール関数として実装していた。
- maintainer の方針: Ruby は Fulgur(PDF) と同様の builder API（`FulgurChart.build(spec).…​.render(:svg)`）にする。描画は builder に一本化し、関数API（`render_svg`/`render_image`/`render_png`）は廃止する。
- 契約は「命名は各言語の慣用形に従うが、**動作は本仕様に準拠する**」としており、builder は Ruby 慣用の構造的選択。描画・オプション・エラー・決定性の **動作は契約どおり** 維持する。

## 公開 API

エントリ:
```ruby
b = FulgurChart.build(spec_json)   # spec_json: chart.js/Vega-Lite の JSON 文字列（必須）
```

チェーン可能セッター（self を返す。builder は再利用可・複数回 render 可）:

| メソッド | 引数 | 動作 |
|---|---|---|
| `.width(w)` / `.height(h)` | Float | spec の寸法を上書き（guard 前に適用） |
| `.scale(s)` | Float | ラスタスケール（svg では無視） |
| `.dsl(d)` | String/Symbol | `:chartjs`/`:vegalite`。未指定は自動判定 |
| `.font(bytes)` | binary String | TTF/OTF。未指定はバンドル Noto Sans JP |
| `.strict` / `.strict(bool)` | 省略時 true | 未知キー拒否 |
| `.format(f)` | String/Symbol | 終端 `render` の既定フォーマット |

終端:
```ruby
b.render(:svg)        # => String (UTF-8, "<svg" 始まり)
b.render(:png)        # => String (binary/ASCII-8BIT, "\x89PNG" 始まり)
b.format(:png).render # => binary
b.render              # => svg（既定）
```

- フォーマット優先順: `render` 引数 > `.format()` セッター > 既定 `:svg`
- 未知フォーマット → `FulgurChart::ParseError`
- エラー階層: `FulgurChart::ParseError < StandardError` / `StrictError < ParseError` / `RenderError < StandardError`
- フォント非対称性: svg 経路の不正フォント → ParseError、png 経路 → RenderError（描画経路差に忠実）
- 決定性: 同一 spec+format+opts → 同一バイト列

メタ情報（モジュール関数のまま）: `FulgurChart.schema(dsl)` / `FulgurChart.version`

公開サーフェス: `FulgurChart.methods(false) == [:build, :schema, :version]`（`Builder` クラス + 上記3関数）。

## アーキテクチャ

builder は純 Ruby、描画の重い処理は native の単一 private プリミティブに委譲。

```
lib/fulgur_chart.rb            FulgurChart.build + FulgurChart::Builder（純 Ruby）
ext/fulgur_chart/src/lib.rs    native: schema / version（public） + __render（private）
```

native（Rust）:
- 公開: `schema(dsl)`, `version()`
- 非公開: `__render(spec_json, format, **opts)` を1つ定義し `private_class_method :__render` で隠す。
  - 中身は現行の `build_ir`（DSL解決→非strict parse→strict再parse→width/height override→guard）を再利用し、format で分岐:
    - `"svg"` → `render::render_chart` / `render_chart_with_font`（フォント Err → ParseError）
    - `"png"` → `raster_direct::render_chart_to_png`（フォント Err → RenderError）
  - `opts` は kwargs（width/height/scale/strict/dsl/font）。`coerce_string` で dsl/format の Symbol を許容。
- 既存の `render_svg` / `render_image` モジュール関数は削除。

Ruby（builder）— `lib/fulgur_chart.rb`:
```ruby
module FulgurChart
  def self.build(spec_json) = Builder.new(spec_json)

  class Builder
    def initialize(spec_json)
      @spec = spec_json
      @opts = {}
    end
    def width(w)  = set(:width, w)
    def height(h) = set(:height, h)
    def scale(s)  = set(:scale, s)
    def dsl(d)    = set(:dsl, d)
    def font(b)   = set(:font, b)
    def strict(v = true) = set(:strict, v)
    def format(f) = set(:format, f)
    def render(fmt = nil)
      f = (fmt || @opts[:format] || :svg).to_s
      FulgurChart.__render(@spec, f, **@opts.reject { |k, _| k == :format })
    end

    private

    def set(k, v)
      @opts[k] = v
      self
    end
  end
end
```

理由: builder ロジックは Ruby で書くと自明・保守容易で、magnus の内部可変オブジェクト（`RefCell` ラップ + self 返し）の複雑さを回避できる。native は「IR構築＋描画」の単一責務に縮小。ユーザー視点の API は同一。

## テスト

新規 `test/test_builder.rb`:
- `build(spec).render(:svg)` → `<svg` / `.render(:png)` → `\x89PNG`(binary, ASCII-8BIT)
- フォーマット優先順（引数 > setter > 既定 svg）
- チェーン再利用（`.width(1234).render(:svg)` が `width="1234"`／複数回 render で決定性）
- String/Symbol 両対応（dsl/format）
- エラー（未知 format/dsl→ParseError、strict→StrictError、寸法超過→ParseError）
- フォント非対称性（svg→ParseError / png→RenderError）
- `.scale` で png が変化

公開サーフェスのロック:
- `FulgurChart.methods(false).sort == [:build, :schema, :version]`
- `refute respond_to?(:render_svg/:render_image/:render_png)`
- `refute respond_to?(:__render)` かつ `assert respond_to?(:__render, true)`

既存テストの扱い: `test_render_svg.rb` / `test_render_image.rb` を builder ベースに統合（`test_builder.rb`）。`test_schema.rb` / `test_smoke.rb` はそのまま。`test_acceptance.rb` を builder API のスモークに更新。

## ドキュメント

- `docs/binding-api-contract.md`: Ruby は builder API（`FulgurChart.build(...).render(...)`）である旨を明記（動作は本仕様準拠）。
- README を builder 例に全面更新。
