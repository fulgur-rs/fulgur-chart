# Ruby バインディング (magnus / rb-sys) Implementation Plan

> **改訂 (PR #11 レビュー後):** 本プラン中の Ruby モジュール名・エラー名前空間は当初 `Fulgur`（`FulgurChart` はエイリアス）としていたが、top-level `Fulgur` が Fulgur(PDF) ライブラリと衝突するため、**正準モジュールを `FulgurChart` に変更**した（`Fulgur` 名前空間・エイリアスは定義しない）。以下のコードブロック中の `Fulgur` / `Fulgur::*` / `FulgurChart = Fulgur` は履歴として残すが、実装・契約(`docs/binding-api-contract.md`)・README は `FulgurChart::ParseError/StrictError/RenderError` が正。

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (または subagent-driven-development) to implement this plan task-by-task.

**Goal:** fulgur-chart コアを magnus + rb-sys でネイティブ拡張としてラップし、`require 'fulgur_chart'` で `FulgurChart.render_svg/render_image/render_png/schema/version` が使える gem を作る。

**Architecture:** `crates/bindings/ruby/` に rb-sys 標準レイアウトの gem を置く。gem ルートの `Cargo.toml` を独立した workspace ルートにし、ルート workspace の `--workspace` 系コマンド（wasm check / musl / MSRV / publish-dry-run）から完全に分離する。ネイティブ crate `ext/fulgur_chart` は `fulgur-chart` コアを path 依存し、magnus で Ruby モジュール `Fulgur`（`FulgurChart` はエイリアス）を定義する。CLI の `render_one`（`crates/fulgur-chart-cli/src/main.rs:259`）を正準リファレンスとして、**エラー分類は call site ベース**で忠実に移植する。

**Tech Stack:** Rust (cdylib), magnus 0.7, rb-sys 0.9, rake-compiler, Ruby 3.3, minitest。

---

## 重要な前提・契約（全タスク共通で厳守）

正準仕様: `docs/binding-api-contract.md`。参照実装: `crates/fulgur-chart-cli/src/main.rs` の `render_one`（259-313行）/ `detect_dsl`（357-367行）/ `run_schema`（369-385行）。

### 公開 API（モジュール `Fulgur`、`FulgurChart` はエイリアス）

| Ruby メソッド | 戻り値 | コア呼び出し |
|---|---|---|
| `render_svg(spec_json, **opts)` | `String`(UTF-8, `<svg` 始まり) | `render::render_chart` / `render::render_chart_with_font` |
| `render_image(spec_json, format:, **opts)` | `String`(ASCII-8BIT, バイナリ) | `raster_direct::render_chart_to_png` |
| `render_png(spec_json, **opts)` | 同上 | `render_image(format: 'png')` の薄いラッパ |
| `schema(dsl)` | `String`(JSON) | `schemars::schema_for!` |
| `version()` | `String` | `fulgur_chart::version()` |

### RenderOptions（kwargs）

`width: Float?` / `height: Float?` / `scale: Float=1.0` / `strict: Bool=false` / `dsl: String?` / `font: String(binary)?`。`scale` は `render_svg` では無視。バリデーションはコア委譲（バインディング側で scale 検証しない）。

### エラー分類（**call site ベース。文字列パースしない**）

| 例外 | 親 | 発生箇所 |
|---|---|---|
| `Fulgur::ParseError` | `StandardError` | 不正JSON / DSL自動判定失敗 / 未知DSL / 非strict parse 失敗 / guard 寸法超過 / 未知 format / **SVG経路の無効フォント** |
| `Fulgur::StrictError` | `Fulgur::ParseError` | strict 再 parse での未知キー |
| `Fulgur::RenderError` | `StandardError` | PNG ラスタ失敗 / **image経路の無効フォント** |

**非対称性に注意**: 同じ無効フォントでも `render_svg`(=`render_chart_with_font` Err)→ ParseError、`render_image`(=`render_chart_to_png` のフォント解析 Err)→ RenderError。CLI のコード割当（1/2/3）と一致させる。

### 処理順（`render_one` を踏襲）

1. DSL 解決: `opts[:dsl]` 明示 or 自動判定（`mark`→vegalite / `type`→chartjs / どちらも無し→ParseError）
2. 非 strict で parse → IR 取得（失敗→ParseError）。**描画はこの非strict IR を使う**
3. `strict` なら strict=true で**再** parse（IR は捨てる。未知キー→StrictError）
4. `opts[:width]/[:height]` を IR に上書き（**guard より前**）
5. `guard::validate_spec(&ir, &InputLimits::default())`（失敗→ParseError）
6. format 分岐で描画

### 決定性

同一 `spec_json`+`format`+`opts` → 同一バイト列。SVG と image は互いに一致しない。

---

## Task 1: workspace 分離 + gem スケルトン（version() のみ動く最小形）

**Files:**
- Modify: `Cargo.toml`（ルート, 6行目 members の下に exclude 追加）
- Create: `crates/bindings/ruby/Cargo.toml`（独立 workspace ルート）
- Create: `crates/bindings/ruby/ext/fulgur_chart/Cargo.toml`（cdylib crate）
- Create: `crates/bindings/ruby/ext/fulgur_chart/extconf.rb`
- Create: `crates/bindings/ruby/ext/fulgur_chart/src/lib.rs`（最小: `Fulgur` モジュール + `version`）
- Create: `crates/bindings/ruby/lib/fulgur_chart.rb`
- Create: `crates/bindings/ruby/fulgur_chart.gemspec`
- Create: `crates/bindings/ruby/Gemfile`
- Create: `crates/bindings/ruby/Rakefile`
- Create: `crates/bindings/ruby/.gitignore`
- Create: `crates/bindings/ruby/test/test_smoke.rb`

**Step 1: ルート workspace から bindings を除外**

`Cargo.toml`（ルート）を編集:

```toml
[workspace]
resolver = "2"
members = ["crates/fulgur-chart", "crates/fulgur-chart-cli"]
exclude = ["crates/bindings"]
```

**Step 2: gem ルート Cargo.toml（独立 workspace）**

`crates/bindings/ruby/Cargo.toml`:

```toml
[workspace]
members = ["ext/fulgur_chart"]
resolver = "2"
```

**Step 3: ネイティブ crate Cargo.toml**

`crates/bindings/ruby/ext/fulgur_chart/Cargo.toml`:

```toml
[package]
name = "fulgur_chart"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
magnus = "0.7"
fulgur-chart = { path = "../../../../fulgur-chart" }
serde_json = "1"
schemars = { version = "1.2", features = ["preserve_order"] }

[build-dependencies]
rb-sys-env = "0.1"
```

> 注: `edition = "2021"`（rb-sys ツールチェーンとの相性、MSRV 制約はルートと独立）。path は `ext/fulgur_chart/` から見て `../../../../fulgur-chart` = `crates/fulgur-chart`。

**Step 4: extconf.rb（rb-sys mkmf）**

`crates/bindings/ruby/ext/fulgur_chart/extconf.rb`:

```ruby
require "mkmf"
require "rb_sys/mkmf"

create_rust_makefile("fulgur_chart/fulgur_chart")
```

**Step 5: 最小 lib.rs（Fulgur モジュール + version）**

`crates/bindings/ruby/ext/fulgur_chart/src/lib.rs`:

```rust
use magnus::{function, prelude::*, Error, Ruby};

fn version() -> String {
    fulgur_chart::version().to_string()
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("Fulgur")?;
    module.define_module_function("version", function!(version, 0))?;
    Ok(())
}
```

**Step 6: lib/fulgur_chart.rb（native ロード + FulgurChart エイリアス + エラー定義）**

`crates/bindings/ruby/lib/fulgur_chart.rb`:

```ruby
# frozen_string_literal: true

require_relative "fulgur_chart/fulgur_chart" # native ext (Init_fulgur_chart -> module Fulgur)

# 契約準拠のエラー階層（native 側で raise されるクラス）。
# native init が未定義の場合に備え Ruby 側でも定義する（冪等）。
module Fulgur
  class ParseError < StandardError; end unless const_defined?(:ParseError)
  class StrictError < ParseError; end unless const_defined?(:StrictError)
  class RenderError < StandardError; end unless const_defined?(:RenderError)
end

# 受け入れ基準が要求する FulgurChart.* を Fulgur のエイリアスとして提供。
FulgurChart = Fulgur unless defined?(FulgurChart)
```

> 設計判断: 正準モジュールは契約の `Fulgur::`（`Fulgur::ParseError` 等）。受け入れ基準の `FulgurChart.render_svg` はエイリアス `FulgurChart = Fulgur` で満たす。エラークラスは **native 側(lib.rs)で定義するのを正**とし、この Ruby 定義は安全網（Task 2 で native 定義を追加したら `const_defined?` ガードで二重定義を防ぐ）。

**Step 7: gemspec**

`crates/bindings/ruby/fulgur_chart.gemspec`:

```ruby
# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name = "fulgur_chart"
  spec.version = "0.1.0"
  spec.authors = ["Fulgur"]
  spec.summary = "Render chart.js / Vega-Lite specs to deterministic SVG/PNG (Rust core)"
  spec.description = spec.summary
  spec.homepage = "https://github.com/fulgur-rs/fulgur-chart"
  spec.license = "MIT OR Apache-2.0"
  spec.required_ruby_version = ">= 3.0"

  spec.files = Dir["lib/**/*.rb", "ext/**/*.{rs,toml,rb,lock}", "README.md"]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/fulgur_chart/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9"
end
```

**Step 8: Gemfile / Rakefile / .gitignore**

`crates/bindings/ruby/Gemfile`:

```ruby
# frozen_string_literal: true

source "https://rubygems.org"
gemspec

gem "rake"
gem "rake-compiler"
gem "minitest"
```

`crates/bindings/ruby/Rakefile`:

```ruby
# frozen_string_literal: true

require "rake/testtask"
require "rb_sys/extensiontask"

GEMSPEC = Gem::Specification.load("fulgur_chart.gemspec")

RbSys::ExtensionTask.new("fulgur_chart", GEMSPEC) do |ext|
  ext.lib_dir = "lib/fulgur_chart"
end

Rake::TestTask.new(test: :compile) do |t|
  t.libs << "test" << "lib"
  t.test_files = FileList["test/test_*.rb"]
  t.warning = false
end

task default: %i[compile test]
```

`crates/bindings/ruby/.gitignore`:

```
/target/
/tmp/
/lib/fulgur_chart/*.so
/lib/fulgur_chart/*.bundle
/Gemfile.lock
*.gem
```

**Step 9: スモークテスト（version のみ）**

`crates/bindings/ruby/test/test_smoke.rb`:

```ruby
# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

class TestSmoke < Minitest::Test
  def test_version_is_string
    assert_kind_of String, FulgurChart.version
    assert_match(/\A\d+\.\d+\.\d+/, FulgurChart.version)
  end
end
```

**Step 10: ルート workspace が ruby crate を含まないことを検証（最重要）**

Run（リポジトリルートから）:
```bash
cargo check --workspace --locked --quiet 2>&1 | tail -5
cargo metadata --format-version 1 --no-deps | grep -c '"name":"fulgur_chart"'
```
Expected: check が成功し、`fulgur_chart`（ruby crate）が workspace メンバに **含まれない**（grep 結果 0）。wasm/MSRV/publish CI が緑のまま。

**Step 11: gem ビルド + スモーク**

Run（`crates/bindings/ruby/` から）:
```bash
bundle install
bundle exec rake compile 2>&1 | tail -20
bundle exec rake test 2>&1 | tail -20
```
Expected: compile 成功、`TestSmoke` 1 test green。

**Step 12: Commit**

```bash
git add Cargo.toml crates/bindings/ruby
git commit -m "feat(ruby): scaffold magnus/rb-sys gem with version() (ts1.5)"
```

---

## Task 2: render_svg + エラークラス（call site 分類）

> **Task1 レビュー指摘の引き継ぎ（このタスクで対応）:**
> - **#2 dead build-dep 削除**: `ext/fulgur_chart/Cargo.toml` の `[build-dependencies] rb-sys-env = "0.1"` を削除（build.rs 無しで未使用。magnus 自身の build.rs が cfg を活性化する）。
> - **#4 エラー定義の単一化**: エラークラスは **native(lib.rs) を唯一の正**とする。`lib/fulgur_chart.rb` の Ruby 側クラス再定義（`module Fulgur ... class ParseError ...`）は**撤去**し、`FulgurChart = Fulgur` エイリアスとコメントのみ残す（二源・`const_defined?` inherit footgun を解消）。

**Files:**
- Modify: `crates/bindings/ruby/ext/fulgur_chart/Cargo.toml`（build-dep 削除）
- Modify: `crates/bindings/ruby/ext/fulgur_chart/src/lib.rs`
- Modify: `crates/bindings/ruby/lib/fulgur_chart.rb`（Ruby 側エラー定義撤去）
- Create: `crates/bindings/ruby/test/test_render_svg.rb`

**Step 1: 失敗するテストを書く**

`crates/bindings/ruby/test/test_render_svg.rb`:

```ruby
# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'

class TestRenderSvg < Minitest::Test
  def test_returns_svg_string
    out = FulgurChart.render_svg(BAR)
    assert_kind_of String, out
    assert out.start_with?("<svg"), "expected <svg, got #{out[0, 20].inspect}"
  end

  def test_invalid_json_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.render_svg("not json") }
  end

  def test_undetectable_dsl_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.render_svg('{"labels":[]}') }
  end

  def test_strict_unknown_key_raises_strict_error
    spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
    assert_raises(Fulgur::StrictError) { FulgurChart.render_svg(spec, strict: true) }
  end

  def test_strict_error_is_parse_error_subclass
    assert Fulgur::StrictError.ancestors.include?(Fulgur::ParseError)
  end

  def test_invalid_font_on_svg_path_raises_parse_error
    assert_raises(Fulgur::ParseError) do
      FulgurChart.render_svg(BAR, font: "not a font".b)
    end
  end

  def test_width_height_override
    big = FulgurChart.render_svg(BAR, width: 1234.0, height: 567.0)
    assert_includes big, "1234"
  end

  def test_dimension_over_limit_raises_parse_error
    assert_raises(Fulgur::ParseError) do
      FulgurChart.render_svg(BAR, width: 40000.0)
    end
  end
end
```

**Step 2: 失敗を確認**

Run（`crates/bindings/ruby/`）: `bundle exec rake test TEST=test/test_render_svg.rb`
Expected: `NoMethodError: render_svg` 等で FAIL。

**Step 3: 実装**

`src/lib.rs` を以下に拡張（version は維持）。magnus の正確なシグネチャはコンパイルで確認しつつ調整する。

```rust
use magnus::{
    function, prelude::*, scan_args::scan_args, scan_args::get_kwargs, Error, RString, Ruby, Value,
};
use fulgur_chart::guard::{validate_spec, InputLimits};

// --- エラー分類ヘルパ（call site で例外クラスを選ぶ） ---

fn parse_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "ParseError"), msg.into())
}
fn strict_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "StrictError"), msg.into())
}
fn render_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "RenderError"), msg.into())
}

// Fulgur::<name> を ExceptionClass として取得（init で定義済み）。
fn exc_class(ruby: &Ruby, name: &str) -> magnus::ExceptionClass {
    let module = ruby.define_module("Fulgur").unwrap();
    module
        .const_get::<_, magnus::ExceptionClass>(name)
        .expect("error class defined in init")
}

// --- DSL 自動判定（CLI の detect_dsl 移植） ---

#[derive(serde::Deserialize)]
struct DslDetector {
    mark: Option<serde::de::IgnoredAny>,
    #[serde(rename = "type")]
    r#type: Option<serde::de::IgnoredAny>,
}

fn detect_dsl(json: &str) -> Result<&'static str, String> {
    let d: DslDetector =
        serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    if d.mark.is_some() {
        return Ok("vegalite");
    }
    if d.r#type.is_some() {
        return Ok("chartjs");
    }
    Err("cannot auto-detect DSL: specify dsl: 'chartjs' or 'vegalite'".to_string())
}

fn parse_spec(json: &str, dsl: &str, strict: bool) -> Result<fulgur_chart::ir::ChartSpec, String> {
    match dsl {
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, strict),
        _ => fulgur_chart::frontend::chartjs::parse(json, strict),
    }
}

// --- オプション ---

#[derive(Default)]
struct Opts {
    width: Option<f64>,
    height: Option<f64>,
    scale: f32,
    strict: bool,
    dsl: Option<String>,
    font: Option<Vec<u8>>,
}

// kwargs から Opts を取り出す。font は binary String を Vec<u8> へ。
fn parse_opts(ruby: &Ruby, kw: magnus::RHash) -> Result<Opts, Error> {
    let args = get_kwargs::<
        _,
        (),
        (
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<bool>,
            Option<String>,
            Option<RString>,
        ),
        (),
    >(kw, &[], &["width", "height", "scale", "strict", "dsl", "font"])?;
    let (width, height, scale, strict, dsl, font) = args.optional;
    if let Some(d) = &dsl {
        if d != "chartjs" && d != "vegalite" {
            return Err(parse_err(ruby, format!("unsupported DSL '{d}'")));
        }
    }
    let font = font.map(|s| unsafe { s.as_slice().to_vec() });
    Ok(Opts {
        width,
        height,
        scale: scale.map(|s| s as f32).unwrap_or(1.0),
        strict: strict.unwrap_or(false),
        dsl,
        font,
    })
}

// IR 構築まで（DSL 解決 / 非strict parse / strict 再parse / override / guard）を共通化。
fn build_ir(ruby: &Ruby, spec_json: &str, opts: &Opts) -> Result<fulgur_chart::ir::ChartSpec, Error> {
    let dsl: &str = match &opts.dsl {
        Some(d) => d.as_str(),
        None => detect_dsl(spec_json).map_err(|e| parse_err(ruby, e))?,
    };
    let mut ir = parse_spec(spec_json, dsl, false).map_err(|e| parse_err(ruby, e))?;
    if opts.strict {
        parse_spec(spec_json, dsl, true).map_err(|e| strict_err(ruby, e))?;
    }
    if let Some(w) = opts.width {
        ir.width = w;
    }
    if let Some(h) = opts.height {
        ir.height = h;
    }
    validate_spec(&ir, &InputLimits::default()).map_err(|e| parse_err(ruby, e))?;
    Ok(ir)
}

// --- render_svg（可変長引数: spec_json + kwargs） ---

fn render_svg(ruby: &Ruby, args: &[Value]) -> Result<RString, Error> {
    let scanned = scan_args::<(String,), (), (), (), magnus::RHash, ()>(args)?;
    let (spec_json,) = scanned.required;
    let opts = parse_opts(ruby, scanned.keywords)?;
    let ir = build_ir(ruby, &spec_json, &opts)?;
    let svg = match &opts.font {
        Some(bytes) => fulgur_chart::render::render_chart_with_font(&ir, bytes)
            .map_err(|e| parse_err(ruby, e))?, // SVG 経路の無効フォント → ParseError
        None => fulgur_chart::render::render_chart(&ir),
    };
    Ok(ruby.str_new(&svg))
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("Fulgur")?;

    // エラー階層（契約準拠）。
    let std_err = ruby.exception_standard_error();
    let parse = module.define_error("ParseError", std_err)?;
    module.define_error("StrictError", parse)?;
    module.define_error("RenderError", std_err)?;

    module.define_module_function("version", function!(version, 0))?;
    module.define_module_function("render_svg", function!(render_svg, -1))?;
    Ok(())
}

fn version() -> String {
    fulgur_chart::version().to_string()
}
```

> magnus API 不確実箇所（`scan_args`/`get_kwargs` の型パラメータ、`define_error`/`exception_standard_error`/`const_get`/`str_new`/`as_slice` の正確名）はコンパイルエラーで確定させる。font の `as_slice` は即時 `to_vec()` でコピーするため安全。`-1` arity は可変長で `&[Value]` を受ける。

**Step 4: lib/fulgur_chart.rb を単一ソース化（#4 対応）**

native(init) が `Fulgur::ParseError/StrictError/RenderError` を定義するので、Ruby 側の再定義を撤去する。`lib/fulgur_chart.rb` を以下に置換:

```ruby
# frozen_string_literal: true

require_relative "fulgur_chart/fulgur_chart" # native ext (Init_fulgur_chart -> module Fulgur)

# 公開 API・エラー階層（Fulgur::ParseError < StandardError /
# Fulgur::StrictError < Fulgur::ParseError / Fulgur::RenderError < StandardError）は
# すべて native(ext) 側で定義される。ここでは受け入れ基準が要求する FulgurChart.* を
# Fulgur のエイリアスとして提供するのみ（定義の単一ソース化）。
FulgurChart = Fulgur unless defined?(FulgurChart)
```

> require_relative が native(.so) のロードに失敗すれば例外で停止するため、Ruby 側フォールバックは到達不能（dead code）。単一ソース化のため撤去する。

**Step 5: テストが通ることを確認**

Run: `bundle exec rake test TEST=test/test_render_svg.rb`
Expected: 全 PASS。

**Step 6: Commit**

```bash
git add crates/bindings/ruby/ext/fulgur_chart/src/lib.rs crates/bindings/ruby/test/test_render_svg.rb
git commit -m "feat(ruby): render_svg with call-site error classification (ts1.5)"
```

---

## Task 3: render_image + render_png

**Files:**
- Modify: `crates/bindings/ruby/ext/fulgur_chart/src/lib.rs`
- Create: `crates/bindings/ruby/test/test_render_image.rb`

**Step 1: 失敗するテスト**

`crates/bindings/ruby/test/test_render_image.rb`:

```ruby
# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
PNG_MAGIC = "\x89PNG".b

class TestRenderImage < Minitest::Test
  def test_render_png_magic_bytes
    out = FulgurChart.render_png(BAR)
    assert_kind_of String, out
    assert_equal Encoding::ASCII_8BIT, out.encoding
    assert out.start_with?(PNG_MAGIC), "expected PNG magic"
  end

  def test_render_image_png_equals_render_png
    assert_equal FulgurChart.render_image(BAR, format: "png"), FulgurChart.render_png(BAR)
  end

  def test_unknown_format_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.render_image(BAR, format: "zzz") }
  end

  def test_invalid_font_on_image_path_raises_render_error
    assert_raises(Fulgur::RenderError) do
      FulgurChart.render_png(BAR, font: "not a font".b)
    end
  end

  def test_png_determinism
    assert_equal FulgurChart.render_png(BAR), FulgurChart.render_png(BAR)
  end

  def test_svg_and_png_differ
    refute_equal FulgurChart.render_svg(BAR).b, FulgurChart.render_png(BAR)
  end
end
```

**Step 2: 失敗を確認**

Run: `bundle exec rake test TEST=test/test_render_image.rb` → FAIL。

**Step 3: 実装（lib.rs に追加）**

> **⚠ Task2 レビュー指摘 #1（必読）**: `format` を取り出す `get_kwargs` は **必ず splat(`RHash`) を指定する**こと。splat 無し（`Splat = ()`）だと magnus が `rb_get_kwargs` に非負の optional 数を渡し、Ruby 側が**残余キー（width/height/... の RenderOptions）で ArgumentError を送出**する。同じ `kw` を二度 scan するのは安全（`get_kwargs` は元の `RHash` を破壊しない）。`parse_opts` 側は既存の `RHash` splat で `format` を無害に無視する。

PNG 描画の重複を `render_png_string` ヘルパに集約する:

```rust
// spec_json + Opts から PNG バイナリ String を生成（render_image / render_png 共通）。
fn render_png_string(ruby: &Ruby, spec_json: &str, opts: &Opts) -> Result<RString, Error> {
    let ir = build_ir(ruby, spec_json, opts)?;
    let fb: &[u8] = opts.font.as_deref().unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    let png = fulgur_chart::raster_direct::render_chart_to_png(&ir, opts.scale, fb)
        .map_err(|e| render_err(ruby, e))?; // image 経路の無効フォント → RenderError
    Ok(ruby.str_from_slice(&png)) // ASCII-8BIT バイナリ String
}

fn render_image(ruby: &Ruby, args: &[Value]) -> Result<RString, Error> {
    let scanned = scan_args::<(String,), (), (), (), RHash, ()>(args)?;
    let (spec_json,) = scanned.required;
    // 必須 kwarg `format` を取り出す。splat(RHash) で RenderOptions キーを許容（#1 対策）。
    let kw = get_kwargs::<_, (String,), (), RHash>(scanned.keywords, &["format"], &[])?;
    let (format,) = kw.required;
    if format != "png" {
        return Err(parse_err(ruby, format!("unsupported format '{format}' (supported: png)")));
    }
    let opts = parse_opts(ruby, scanned.keywords)?; // 同じ kw を再 scan（format は無視される）
    render_png_string(ruby, &spec_json, &opts)
}

fn render_png(ruby: &Ruby, args: &[Value]) -> Result<RString, Error> {
    // render_image(spec, format: 'png', **opts) と等価。format チェック不要。
    let scanned = scan_args::<(String,), (), (), (), RHash, ()>(args)?;
    let (spec_json,) = scanned.required;
    let opts = parse_opts(ruby, scanned.keywords)?;
    render_png_string(ruby, &spec_json, &opts)
}
```

> - `str_from_slice` がバイナリ(ASCII-8BIT) String を返すことをテスト(`test_render_png_magic_bytes`)で保証。magnus 0.7 の正確な API 名はコンパイルで確定（`ruby.str_from_slice(&[u8])` 想定）。
> - `render_err` / `Opts::scale` の `#[allow(dead_code)]` は本タスクで実消費されるので**除去**する。

init に登録追加:
```rust
module.define_module_function("render_image", function!(render_image, -1))?;
module.define_module_function("render_png", function!(render_png, -1))?;
```

**Step 4: テスト PASS を確認**

Run: `bundle exec rake test TEST=test/test_render_image.rb` → 全 PASS。

**Step 5: Commit**

```bash
git add crates/bindings/ruby/ext/fulgur_chart/src/lib.rs crates/bindings/ruby/test/test_render_image.rb
git commit -m "feat(ruby): render_image/render_png with binary String output (ts1.5)"
```

---

## Task 4: schema + version（全 API 完成）

**Files:**
- Modify: `crates/bindings/ruby/ext/fulgur_chart/src/lib.rs`
- Create: `crates/bindings/ruby/test/test_schema.rb`

**Step 1: 失敗するテスト**

`crates/bindings/ruby/test/test_schema.rb`:

```ruby
# frozen_string_literal: true

require "minitest/autorun"
require "json"
require "fulgur_chart"

class TestSchema < Minitest::Test
  def test_chartjs_schema_is_json
    s = FulgurChart.schema("chartjs")
    assert_kind_of String, s
    assert_kind_of Hash, JSON.parse(s)
  end

  def test_vegalite_schema_is_json
    assert_kind_of Hash, JSON.parse(FulgurChart.schema("vegalite"))
  end

  def test_unknown_dsl_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.schema("nope") }
  end
end
```

**Step 2: 失敗確認** → `bundle exec rake test TEST=test/test_schema.rb`

**Step 3: 実装（lib.rs）**

```rust
fn schema(ruby: &Ruby, dsl: String) -> Result<String, Error> {
    let s = match dsl.as_str() {
        "chartjs" => schemars::schema_for!(fulgur_chart::schema::ChartJsSpec),
        "vegalite" => schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec),
        other => {
            return Err(parse_err(
                ruby,
                format!("unsupported DSL '{other}' (supported: chartjs, vegalite)"),
            ))
        }
    };
    serde_json::to_string(&s).map_err(|e| render_err(ruby, format!("schema serialization: {e}")))
}
```

init 登録:
```rust
module.define_module_function("schema", function!(schema, 1))?;
```

**Step 4: PASS 確認** → `bundle exec rake test TEST=test/test_schema.rb`

**Step 5: Commit**

```bash
git add crates/bindings/ruby/ext/fulgur_chart/src/lib.rs crates/bindings/ruby/test/test_schema.rb
git commit -m "feat(ruby): schema(dsl) and finalize public API (ts1.5)"
```

---

## Task 5: テストフィクスチャ集約 + 全 API スモーク + 決定性 + README

> **Task3 レビュー指摘 #1（テスト衛生）対応:** `test_render_svg.rb` と `test_render_image.rb` がともにトップレベル定数 `BAR` を定義し `already initialized constant BAR` 警告が出る。共有 `test/test_helper.rb` にフィクスチャと require を集約し、各テストを refactor して警告を解消する（VL/CJ もここに置く）。

**Files:**
- Create: `crates/bindings/ruby/test/test_helper.rb`（共有 require + フィクスチャ）
- Modify: `crates/bindings/ruby/test/test_render_svg.rb`（local `BAR` 撤去、`require_relative "test_helper"`）
- Modify: `crates/bindings/ruby/test/test_render_image.rb`（local `BAR`/`PNG_MAGIC` 撤去、helper 利用）
- Create: `crates/bindings/ruby/test/test_acceptance.rb`
- Create: `crates/bindings/ruby/README.md`

**Step 0: 共有テストヘルパ**

`crates/bindings/ruby/test/test_helper.rb`:

```ruby
# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

module Fixtures
  BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
  LINE = '{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,3,2]}]}}'
  VEGALITE_BAR = '{"mark":"bar","data":{"values":[{"a":"x","b":1}]},"encoding":{"x":{"field":"a"},"y":{"field":"b"}}}'
  PNG_MAGIC = "\x89PNG".b
end
```

既存テストは先頭を `require_relative "test_helper"` にし、`BAR` → `Fixtures::BAR`、`PNG_MAGIC` → `Fixtures::PNG_MAGIC` に置換、`require "minitest/autorun"`/`require "fulgur_chart"` の重複行を撤去する（helper が担う）。Rakefile の `t.libs << "test"` で `require_relative` は不要だが、相対 require が最も確実。

**Step 1: 受け入れ基準そのもののスモークテスト**

`crates/bindings/ruby/test/test_acceptance.rb`:

```ruby
# frozen_string_literal: true

require_relative "test_helper"

class TestAcceptance < Minitest::Test
  CJ = Fixtures::LINE
  VL = Fixtures::VEGALITE_BAR

  def test_full_api_present
    %i[render_svg render_image render_png schema version].each do |m|
      assert FulgurChart.respond_to?(m), "FulgurChart.#{m} missing"
    end
  end

  def test_chartjs_svg_and_png
    assert FulgurChart.render_svg(CJ).start_with?("<svg")
    assert FulgurChart.render_png(CJ).start_with?(Fixtures::PNG_MAGIC)
  end

  def test_vegalite_autodetected
    assert FulgurChart.render_svg(VL).start_with?("<svg")
  end

  def test_dsl_override
    assert FulgurChart.render_svg(CJ, dsl: "chartjs").start_with?("<svg")
  end

  def test_scale_changes_png
    refute_equal FulgurChart.render_png(CJ, scale: 1.0), FulgurChart.render_png(CJ, scale: 2.0)
  end

  def test_determinism_svg_and_png
    assert_equal FulgurChart.render_svg(CJ), FulgurChart.render_svg(CJ)
    assert_equal FulgurChart.render_png(CJ), FulgurChart.render_png(CJ)
  end
end
```

> 検証: `bundle exec rake` 実行時に `already initialized constant` 警告が**消えている**こと（警告ゼロ）。

**Step 2: 全テスト実行**

Run（`crates/bindings/ruby/`）: `bundle exec rake` （= compile + 全 test）
Expected: smoke / render_svg / render_image / schema / acceptance すべて green。

**Step 3: README**

`crates/bindings/ruby/README.md` に install / 使用例（`FulgurChart.render_svg` / `render_png` / エラー階層）を記載。

**Step 4: Commit**

```bash
git add crates/bindings/ruby/test/test_acceptance.rb crates/bindings/ruby/README.md
git commit -m "test(ruby): acceptance smoke + determinism; add README (ts1.5)"
```

---

## Task 6: CI ジョブ（拡張ビルド + スモーク）

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: ruby ジョブ追加**

`ci.yml` の末尾に独立ジョブを追加（ルート workspace の他ジョブには触れない）:

```yaml
  ruby-binding:
    name: Ruby binding (build + smoke)
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: crates/bindings/ruby
    steps:
      - uses: actions/checkout@v5

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: crates/bindings/ruby/ext/fulgur_chart
          key: ruby-binding

      - uses: ruby/setup-ruby@v1
        with:
          ruby-version: "3.3"
          bundler-cache: false
          working-directory: crates/bindings/ruby

      - name: Install gems
        run: bundle install

      - name: Compile + test
        run: bundle exec rake
```

**Step 2: yaml 検証**

Run: `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml'); puts 'yaml ok'"`
Expected: `yaml ok`。

**Step 3: 既存ジョブが ruby crate を含まないことを再確認**

Run（ルート）: `cargo metadata --format-version 1 --no-deps | grep -c '"name":"fulgur_chart"'` → `0`。

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(ruby): build native ext and run smoke tests (ts1.5)"
```

---

## Task 7: follow-up issue + 仕上げ

**Step 1: cross-gem プリビルド配布の follow-up を beads に作成**

```bash
bd create \
  --title="Ruby gem: cross-gem プリビルド + 配布 (path依存解消)" \
  --description="ts1.5 はソース gem + ローカル/CI ビルドまで。配布には (1) cross-gem (rb-sys-dock) でプラットフォーム別プリビルド gem, (2) コアの path 依存を crates.io 公開版 or vendoring に切替, (3) gem push が必要。" \
  --type=feature --priority=3
bd dep add <new-id> fulgur-chart-ts1.5   # 任意: ts1.5 完了が前提
```

**Step 2: 実装チェックリスト（契約 §5）の最終確認**

`docs/binding-api-contract.md` §5 の 8 項目をテストでカバーしていることを確認:
- RenderOptions マッピング（Task5 dsl/scale/width）
- 未知 format → ParseError（Task3）
- 未知 dsl(schema) → ParseError（Task4）
- 3 エラーコードのマッピング（Task2/3）
- version()（Task1）
- PNG マジックバイト（Task3）
- `<svg` 始まり（Task2）
- 決定性（Task3/5）

**Step 3: 最終全テスト + clippy/fmt（ext crate）**

Run（`crates/bindings/ruby/`）:
```bash
bundle exec rake
( cd ext/fulgur_chart && cargo fmt --check && cargo clippy --all-targets -- -D warnings )
```
Expected: 全 green、clippy/fmt クリーン。

**Step 4: ルート workspace 健全性の最終確認**

Run（ルート）:
```bash
cargo test --workspace --locked --quiet
cargo check --workspace --locked --target wasm32-unknown-unknown 2>&1 | tail -3   # toolchain があれば
```
Expected: 既存 workspace は影響なし（ruby crate 不参加）。

**Step 5: Commit**

```bash
git add -A
git commit -m "chore(ruby): finalize ts1.5; file cross-gem follow-up"
```

---

## 完了条件（受け入れ基準対応）

- [ ] `require 'fulgur_chart'` 後 `FulgurChart.render_svg(spec)` が `String`（`<svg` 始まり）を返す
- [ ] `FulgurChart.render_png(spec)` がバイナリ `String`（`\x89PNG` 始まり, ASCII-8BIT）を返す
- [ ] `render_image(spec, format:)` / `schema(dsl)` / `version()` が契約通り動作
- [ ] 3 種エラー（ParseError/StrictError/RenderError）が call site ベースで正しく送出
- [ ] CI `ruby-binding` ジョブで拡張ビルド + スモークが green
- [ ] ルート workspace の wasm/musl/MSRV/publish CI が無影響（ruby crate 非参加）
- [ ] cross-gem 配布の follow-up issue を作成
