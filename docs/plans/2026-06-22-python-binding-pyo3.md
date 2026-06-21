# Python バインディング (PyO3 + maturin) 実装プラン

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** PyO3 でコアをラップし maturin で abi3-py38 wheel をビルド。`fulgur_chart` モジュールとして `render_svg`/`render_image`/`render_png`/`schema`/`version` と 3 種エラークラスを公開する。

**Architecture:** `crates/bindings/python/` に独立 Cargo プロジェクト（main workspace は `exclude = ["crates/bindings"]` 済み）。PyO3 0.29 abi3-py38 cdylib クレート + maturin の `python-source = "python"` レイアウト。`render_image(spec, format, **opts)` が契約準拠のプライマリ API、`render_png` は `__init__.py` 内の薄い Python ラッパー（ergonomics 用）。エラー分類は呼び出しサイトで決定（エラー文字列のパース禁止）。

**Tech Stack:** PyO3 0.29, maturin 1.8.1, Python ≥ 3.8 (abi3), fulgur-chart 0.6.0 (path 依存), serde_json 1, schemars 1

---

## 全タスク一覧

| # | タスク | 目的 |
|---|--------|------|
| 1 | ディレクトリ構造 + Cargo/pyproject 設定 | ビルド基盤 |
| 2 | `version()` / `schema()` 実装 | シンプルな動作確認 |
| 3 | DSL 検出 + IR 構築ヘルパー | render_svg/image の共通基盤 |
| 4 | `render_svg()` 実装 | SVG 出力 |
| 5 | `render_image()` / `render_png()` 実装 | PNG 出力 |

---

### Task 1: ディレクトリ構造 + Cargo/pyproject 設定

**Files:**
- Create: `crates/bindings/python/Cargo.toml`
- Create: `crates/bindings/python/pyproject.toml`
- Create: `crates/bindings/python/src/lib.rs` (スケルトン)
- Create: `crates/bindings/python/python/fulgur_chart/__init__.py`

**Step 1: ディレクトリを作成する**

worktree 内で実行（`cd` して作業すること）:

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3
mkdir -p crates/bindings/python/src
mkdir -p crates/bindings/python/python/fulgur_chart
mkdir -p crates/bindings/python/tests
```

**Step 2: Cargo.toml を作成する**

`crates/bindings/python/Cargo.toml`:

```toml
[package]
name = "fulgur_chart"
version = "0.6.0"
edition = "2024"
rust-version = "1.85.0"
publish = false

[lib]
name = "fulgur_chart"
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.29", features = ["abi3-py38"] }
fulgur-chart = { path = "../../fulgur-chart" }
serde_json = "1"
schemars = "1"
```

**Step 3: pyproject.toml を作成する**

`crates/bindings/python/pyproject.toml`:

```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "fulgur_chart"
version = "0.6.0"
description = "Render chart.js / Vega-Lite specs to deterministic SVG/PNG"
license = {text = "MIT OR Apache-2.0"}
requires-python = ">=3.8"
classifiers = [
    "Programming Language :: Rust",
    "Programming Language :: Python :: Implementation :: CPython",
    "Programming Language :: Python :: Implementation :: PyPy",
]

[tool.maturin]
python-source = "python"
features = ["pyo3/abi3-py38"]
```

**Step 4: src/lib.rs のスケルトンを作成する**

`crates/bindings/python/src/lib.rs` — エラークラス定義とモジュール登録のみ:

```rust
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

pyo3::create_exception!(fulgur_chart, FulgurParseError, PyValueError);
pyo3::create_exception!(fulgur_chart, FulgurStrictError, FulgurParseError);
pyo3::create_exception!(fulgur_chart, FulgurRenderError, PyRuntimeError);

fn parse_error(msg: impl Into<String>) -> PyErr {
    FulgurParseError::new_err(msg.into())
}

fn strict_error(msg: impl Into<String>) -> PyErr {
    FulgurStrictError::new_err(msg.into())
}

fn render_error(msg: impl Into<String>) -> PyErr {
    FulgurRenderError::new_err(msg.into())
}

#[pymodule]
fn fulgur_chart(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FulgurParseError", m.py().get_type::<FulgurParseError>())?;
    m.add("FulgurStrictError", m.py().get_type::<FulgurStrictError>())?;
    m.add("FulgurRenderError", m.py().get_type::<FulgurRenderError>())?;
    Ok(())
}
```

**Step 5: python/fulgur_chart/__init__.py を作成する**

`crates/bindings/python/python/fulgur_chart/__init__.py`:

```python
from .fulgur_chart import (
    FulgurParseError,
    FulgurRenderError,
    FulgurStrictError,
    render_image,
    render_svg,
    schema,
    version,
)


def render_png(
    spec_json: str,
    *,
    width=None,
    height=None,
    scale: float = 1.0,
    strict: bool = False,
    dsl=None,
    font=None,
) -> bytes:
    """PNG バイト列を返す（render_image(spec, 'png', ...) の短縮形）。"""
    return render_image(
        spec_json,
        "png",
        width=width,
        height=height,
        scale=scale,
        strict=strict,
        dsl=dsl,
        font=font,
    )


__all__ = [
    "FulgurParseError",
    "FulgurRenderError",
    "FulgurStrictError",
    "render_image",
    "render_png",
    "render_svg",
    "schema",
    "version",
]
```

**Step 6: maturin develop でビルドを確認する**

```bash
cd crates/bindings/python
maturin develop
```

期待出力: `Finished dev [unoptimized + debuginfo]` で終わること。エラーなし。

**Step 7: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3
git add crates/bindings/python/
git commit -m "feat(python): scaffold PyO3 binding structure"
```

---

### Task 2: `version()` と `schema()` の実装（TDD）

**Files:**
- Modify: `crates/bindings/python/src/lib.rs`
- Create: `crates/bindings/python/tests/test_fulgur_chart.py`

**Step 1: テストを先に書く**

`crates/bindings/python/tests/test_fulgur_chart.py` を新規作成:

```python
import json

import fulgur_chart


# ── version ──────────────────────────────────────────────────────────

def test_version_returns_string():
    v = fulgur_chart.version()
    assert isinstance(v, str)
    assert len(v) > 0


def test_version_is_semver():
    v = fulgur_chart.version()
    parts = v.split(".")
    assert len(parts) == 3, f"期待: X.Y.Z 形式, 実際: {v}"


# ── schema ───────────────────────────────────────────────────────────

def test_schema_chartjs_is_valid_json():
    s = fulgur_chart.schema("chartjs")
    parsed = json.loads(s)
    assert isinstance(parsed, dict)


def test_schema_vegalite_is_valid_json():
    s = fulgur_chart.schema("vegalite")
    parsed = json.loads(s)
    assert isinstance(parsed, dict)


def test_schema_unknown_dsl_raises_parse_error():
    try:
        fulgur_chart.schema("unknown")
        assert False, "例外が送出されなかった"
    except fulgur_chart.FulgurParseError:
        pass
```

**Step 2: テストが失敗することを確認**

```bash
cd crates/bindings/python
python -m pytest tests/ -k "version or schema" -v 2>&1 | head -20
```

期待: `AttributeError: module 'fulgur_chart' has no attribute 'version'`（未実装）

**Step 3: `version()` と `schema()` を lib.rs に実装する**

`#[pymodule]` の前に追加:

```rust
#[pyfunction]
fn version() -> &'static str {
    fulgur_chart::version()
}

#[pyfunction]
fn schema(dsl: &str) -> PyResult<String> {
    let s = match dsl {
        "chartjs" => serde_json::to_string(
            &schemars::schema_for!(fulgur_chart::schema::ChartJsSpec),
        )
        .unwrap(),
        "vegalite" => serde_json::to_string(
            &schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec),
        )
        .unwrap(),
        other => return Err(parse_error(format!("未知のDSL: {other}"))),
    };
    Ok(s)
}
```

`#[pymodule]` 内に登録を追加:

```rust
m.add_function(wrap_pyfunction!(version, m)?)?;
m.add_function(wrap_pyfunction!(schema, m)?)?;
```

**Step 4: ビルド → テストを実行**

```bash
maturin develop && python -m pytest tests/ -k "version or schema" -v
```

期待: 5 テスト全 PASS

**Step 5: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3
git add crates/bindings/python/src/lib.rs crates/bindings/python/tests/
git commit -m "feat(python): implement version() and schema()"
```

---

### Task 3: DSL 検出 + IR 構築ヘルパーの実装

**Files:**
- Modify: `crates/bindings/python/src/lib.rs`

`render_svg`/`render_image` 共通の内部ヘルパー。公開 API ではないのでテストは Task 4/5 で間接的に行う。

**Step 1: `detect_dsl` / `parse_spec` / `build_ir` を lib.rs に追加する**

既存コードの `fn parse_error` の直後（`#[pyfunction]` の前）に配置:

```rust
fn detect_dsl(json: &str) -> Option<&'static str> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("mark").is_some() {
        Some("vegalite")
    } else if v.get("type").is_some() {
        Some("chartjs")
    } else {
        None
    }
}

fn parse_spec(
    json: &str,
    strict: bool,
    dsl: &str,
) -> PyResult<fulgur_chart::ir::ChartSpec> {
    // 非 strict でパース（JSON / DSL 構文エラー → ParseError）
    let spec = match dsl {
        "chartjs" => fulgur_chart::frontend::chartjs::parse(json, false),
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, false),
        other => return Err(parse_error(format!("未知のDSL: {other}"))),
    }
    .map_err(parse_error)?;

    // strict モードなら strict=true で再パース（未知キー → StrictError）
    if strict {
        let _ = match dsl {
            "chartjs" => fulgur_chart::frontend::chartjs::parse(json, true),
            "vegalite" => fulgur_chart::frontend::vegalite::parse(json, true),
            _ => unreachable!(),
        }
        .map_err(strict_error)?;
    }

    Ok(spec)
}

fn build_ir(
    spec_json: &str,
    width: Option<f64>,
    height: Option<f64>,
    strict: bool,
    dsl: Option<&str>,
) -> PyResult<fulgur_chart::ir::ChartSpec> {
    let dsl_name = match dsl {
        Some(d) => d,
        None => detect_dsl(spec_json)
            .ok_or_else(|| parse_error("DSL自動判定失敗: 'mark'または'type'キーが必要"))?,
    };
    let mut spec = parse_spec(spec_json, strict, dsl_name)?;
    // width/height override（ChartSpec フィールドは f64）
    if let Some(w) = width {
        spec.width = w;
    }
    if let Some(h) = height {
        spec.height = h;
    }
    // 寸法制限チェック（1–32768 px）
    fulgur_chart::guard::validate_spec(&spec, &fulgur_chart::guard::InputLimits::default())
        .map_err(parse_error)?;
    Ok(spec)
}
```

**Step 2: ビルドが通ることを確認**

```bash
cd crates/bindings/python
maturin develop
```

期待: エラーなし

**Step 3: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3
git add crates/bindings/python/src/lib.rs
git commit -m "feat(python): add detect_dsl / parse_spec / build_ir helpers"
```

---

### Task 4: `render_svg()` の実装（TDD）

**Files:**
- Modify: `crates/bindings/python/src/lib.rs`
- Modify: `crates/bindings/python/tests/test_fulgur_chart.py`

**Step 1: テストを追加する**

`test_fulgur_chart.py` の末尾に追加:

```python
# ── テスト用フィクスチャ ───────────────────────────────────────────

CHARTJS_BAR = json.dumps({
    "type": "bar",
    "data": {
        "labels": ["A", "B"],
        "datasets": [{"data": [1, 2]}],
    },
})

VEGALITE_POINT = json.dumps({
    "mark": "point",
    "data": {"values": [{"x": 1, "y": 2}]},
    "encoding": {
        "x": {"field": "x", "type": "quantitative"},
        "y": {"field": "y", "type": "quantitative"},
    },
})


# ── render_svg ───────────────────────────────────────────────────────

def test_render_svg_chartjs_starts_with_svg_tag():
    svg = fulgur_chart.render_svg(CHARTJS_BAR)
    assert svg.startswith("<svg"), svg[:80]


def test_render_svg_vegalite_starts_with_svg_tag():
    svg = fulgur_chart.render_svg(VEGALITE_POINT)
    assert svg.startswith("<svg"), svg[:80]


def test_render_svg_is_deterministic():
    a = fulgur_chart.render_svg(CHARTJS_BAR)
    b = fulgur_chart.render_svg(CHARTJS_BAR)
    assert a == b


def test_render_svg_with_width_height_overrides():
    # width/height を指定してもクラッシュしない
    svg = fulgur_chart.render_svg(CHARTJS_BAR, width=400.0, height=300.0)
    assert svg.startswith("<svg")


def test_render_svg_invalid_json_raises_parse_error():
    try:
        fulgur_chart.render_svg("not json")
        assert False, "例外が送出されなかった"
    except fulgur_chart.FulgurParseError:
        pass


def test_render_svg_strict_unknown_key_raises_strict_error():
    spec = json.dumps({
        "type": "bar",
        "data": {"labels": ["A"], "datasets": [{"data": [1]}]},
        "unknownKey": "value",
    })
    try:
        fulgur_chart.render_svg(spec, strict=True)
        assert False, "strict モードで例外が送出されなかった"
    except fulgur_chart.FulgurStrictError:
        pass


def test_strict_error_is_subclass_of_parse_error():
    assert issubclass(fulgur_chart.FulgurStrictError, fulgur_chart.FulgurParseError)


def test_parse_error_is_subclass_of_value_error():
    assert issubclass(fulgur_chart.FulgurParseError, ValueError)
```

**Step 2: テストが失敗することを確認**

```bash
cd crates/bindings/python
python -m pytest tests/ -k "render_svg" -v 2>&1 | head -20
```

期待: `AttributeError: module 'fulgur_chart' has no attribute 'render_svg'`

**Step 3: `render_svg()` を lib.rs に実装する**

`build_ir` の後、`#[pymodule]` の前に追加:

```rust
#[pyfunction]
#[pyo3(signature = (spec_json, *, width=None, height=None, scale=1.0, strict=false, dsl=None, font=None))]
fn render_svg(
    spec_json: &str,
    width: Option<f64>,
    height: Option<f64>,
    scale: f64,
    strict: bool,
    dsl: Option<&str>,
    font: Option<&[u8]>,
) -> PyResult<String> {
    let _ = scale; // render_svg では scale を使用しない（仕様通り）
    let spec = build_ir(spec_json, width, height, strict, dsl)?;
    if let Some(font_bytes) = font {
        // SVG パスのフォントエラー → ParseError（binding-api-contract の非対称規約）
        fulgur_chart::render::render_chart_with_font(&spec, font_bytes).map_err(parse_error)
    } else {
        Ok(fulgur_chart::render::render_chart(&spec))
    }
}
```

`#[pymodule]` 内に登録:

```rust
m.add_function(wrap_pyfunction!(render_svg, m)?)?;
```

**Step 4: ビルド → テストを実行**

```bash
maturin develop && python -m pytest tests/ -k "render_svg or strict_error or parse_error" -v
```

期待: 8 テスト全 PASS

**Step 5: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3
git add crates/bindings/python/src/lib.rs crates/bindings/python/tests/test_fulgur_chart.py
git commit -m "feat(python): implement render_svg()"
```

---

### Task 5: `render_image()` / `render_png()` の実装（TDD）

**Files:**
- Modify: `crates/bindings/python/src/lib.rs`
- Modify: `crates/bindings/python/tests/test_fulgur_chart.py`

**Step 1: テストを追加する**

`test_fulgur_chart.py` の末尾に追加:

```python
# ── render_image / render_png ─────────────────────────────────────

PNG_MAGIC = b"\x89PNG"


def test_render_image_png_returns_bytes_starting_with_magic():
    data = fulgur_chart.render_image(CHARTJS_BAR, "png")
    assert isinstance(data, bytes)
    assert data[:4] == PNG_MAGIC, f"PNGマジックが期待値と異なる: {data[:4]!r}"


def test_render_png_returns_bytes_starting_with_magic():
    data = fulgur_chart.render_png(CHARTJS_BAR)
    assert isinstance(data, bytes)
    assert data[:4] == PNG_MAGIC


def test_render_image_is_deterministic():
    a = fulgur_chart.render_image(CHARTJS_BAR, "png")
    b = fulgur_chart.render_image(CHARTJS_BAR, "png")
    assert a == b


def test_render_png_equals_render_image_png():
    """render_png は render_image(spec, 'png') の薄いラッパーであること。"""
    a = fulgur_chart.render_image(CHARTJS_BAR, "png")
    b = fulgur_chart.render_png(CHARTJS_BAR)
    assert a == b


def test_render_image_unknown_format_raises_parse_error():
    try:
        fulgur_chart.render_image(CHARTJS_BAR, "jpeg")
        assert False, "例外が送出されなかった"
    except fulgur_chart.FulgurParseError:
        pass


def test_render_error_is_subclass_of_runtime_error():
    assert issubclass(fulgur_chart.FulgurRenderError, RuntimeError)
```

**Step 2: テストが失敗することを確認**

```bash
cd crates/bindings/python
python -m pytest tests/ -k "render_image or render_png or render_error" -v 2>&1 | head -20
```

期待: `AttributeError: module 'fulgur_chart' has no attribute 'render_image'`

**Step 3: `render_image()` を lib.rs に実装する**

`render_svg` の後、`#[pymodule]` の前に追加:

```rust
#[pyfunction]
#[pyo3(signature = (spec_json, format, *, width=None, height=None, scale=1.0, strict=false, dsl=None, font=None))]
fn render_image<'py>(
    py: Python<'py>,
    spec_json: &str,
    format: &str,
    width: Option<f64>,
    height: Option<f64>,
    scale: f64,
    strict: bool,
    dsl: Option<&str>,
    font: Option<&[u8]>,
) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    if format != "png" {
        return Err(parse_error(format!(
            "サポートされていないフォーマット: '{format}'"
        )));
    }
    let spec = build_ir(spec_json, width, height, strict, dsl)?;
    let font_bytes = font.unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    // PNG パスの全エラー（フォントエラー含む）→ RenderError（binding-api-contract の非対称規約）
    let png_data =
        fulgur_chart::raster_direct::render_chart_to_png(&spec, scale as f32, font_bytes)
            .map_err(render_error)?;
    Ok(pyo3::types::PyBytes::new(py, &png_data))
}
```

`#[pymodule]` 内に登録:

```rust
m.add_function(wrap_pyfunction!(render_image, m)?)?;
```

**Step 4: ビルド → 全テストを実行**

```bash
maturin develop && python -m pytest tests/ -v
```

期待: 全テスト PASS

**Step 5: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3
git add crates/bindings/python/src/lib.rs crates/bindings/python/tests/test_fulgur_chart.py
git commit -m "feat(python): implement render_image() → PNG bytes"
```

---

## 完了確認

全タスク完了後、以下を実行して最終確認:

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feature/python-binding-pyo3/crates/bindings/python
maturin develop
python -m pytest tests/ -v
python -c "import fulgur_chart; print(fulgur_chart.version())"
```

期待:
- 全テスト PASS
- バージョン文字列が表示される（例: `0.6.0`）

---

## 補足: `render_image` vs `render_png` 設計判断

- `render_image(spec, format, **opts)` — `docs/binding-api-contract.md` 準拠（将来の `'jpeg'`/`'webp'` 拡張に対応）
- `render_png(spec, **opts)` — issue acceptance の `render_png` 要件を満たす Python ラッパー
- `__init__.py` 内の純 Python 実装なので Rust 変更不要で両立できる

## 補足: 依存関係について

- `fulgur-chart = { path = "../../fulgur-chart" }` — ローカル開発用。PyPI 公開時は crates.io 公開バージョンに切り替える（Ruby binding の先例と同様）。
- `publish = false` — CI で意図せず crates.io に push されることを防ぐ。
