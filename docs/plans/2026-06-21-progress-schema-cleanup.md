# progress schema cleanup Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `ProgressDataset` の no-op な `borderColor`/`borderWidth` をスキーマから削除し、`legend` を受け付けない `ProgressPlugins` 構造体を導入して strict チェックを追加する。

**Architecture:** gauge/radialGauge（PR #22）の先例を踏襲。スキーマ型で no-op フィールドを排除し、専用 strict 検証関数 `check_unknown_keys_progress()` で実行時にも同じ境界を強制する。

**Tech Stack:** Rust, serde/schemars, `crates/fulgur-chart/src/schema/chartjs.rs`, `crates/fulgur-chart/src/frontend/chartjs.rs`, `crates/fulgur-chart/tests/render_progress.rs`

---

### Task 1: schema — ProgressDataset から border フィールドを削除

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs:464-476`

**Step 1: 削除前に既存テストが全パスすることを確認**

```bash
cargo test -p fulgur-chart --test render_progress
```
Expected: 15 passed, 0 failed

**Step 2: ProgressDataset から border_color/border_width を削除**

`crates/fulgur-chart/src/schema/chartjs.rs` の `ProgressDataset` を以下に変更:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
}
```

**Step 3: ビルドが通ることを確認**

```bash
cargo build -p fulgur-chart 2>&1 | head -30
```
Expected: エラーなし（ProgressDataset.border_color/border_width を参照しているコードはないはず）

**Step 4: 既存テストが引き続き全パスすることを確認**

```bash
cargo test -p fulgur-chart --test render_progress
```
Expected: 15 passed, 0 failed

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "fix(progress): remove no-op borderColor/borderWidth from ProgressDataset schema"
```

---

### Task 2: schema — ProgressPlugins 構造体を新設して legend を除外

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs:202-209` (GaugePlugins の直後あたり)

**Step 1: ProgressPlugins 構造体を追加**

`GaugePlugins` の定義（202行目付近）の直後に追加:

```rust
/// progress バーには凡例が描けないため legend は非公開。datalabels は % 表示制御に使用。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ProgressPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datalabels: Option<DataLabelsPlugin>,
}
```

**Step 2: ProgressOptions.plugins の型を変更**

`ProgressOptions`（480行目付近）を以下に変更:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<ProgressPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}
```

**Step 3: ビルドとテストが通ることを確認**

```bash
cargo test -p fulgur-chart --test render_progress
```
Expected: 15 passed, 0 failed

**Step 4: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "fix(progress): introduce ProgressPlugins without legend field"
```

---

### Task 3: schema 変更の検証テストを追加

**Files:**
- Modify: `crates/fulgur-chart/tests/render_progress.rs`

**Step 1: テストを追加（ファイル末尾に追記）**

```rust
#[test]
fn progress_strict_rejects_border_color() {
    let err = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70],"borderColor":"#ff0000"}]}}"##,
        true,
    );
    assert!(err.is_err(), "borderColor should be rejected in strict mode");
}

#[test]
fn progress_strict_rejects_border_width() {
    let err = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70],"borderWidth":2}]}}"##,
        true,
    );
    assert!(err.is_err(), "borderWidth should be rejected in strict mode");
}

#[test]
fn progress_strict_rejects_legend() {
    let err = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70]}]},"options":{"plugins":{"legend":{"display":true}}}}"##,
        true,
    );
    assert!(err.is_err(), "legend should be rejected in strict mode");
}

#[test]
fn progress_schema_rejects_border_color() {
    // deny_unknown_fields により strict=false でも serde パースエラー
    let err = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70],"borderColor":"#ff0000"}]}}"##,
        false,
    );
    assert!(err.is_err(), "borderColor should fail schema parse");
}

#[test]
fn progress_schema_rejects_legend() {
    // deny_unknown_fields により strict=false でも serde パースエラー
    let err = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70]}]},"options":{"plugins":{"legend":{"display":true}}}}"##,
        false,
    );
    assert!(err.is_err(), "legend should fail schema parse");
}

#[test]
fn progress_strict_accepts_datalabels() {
    // datalabels は ProgressPlugins に残っているため strict でも通る（回帰確認）
    let ok = chartjs::parse(
        r##"{"type":"progress","data":{"datasets":[{"data":[70]}]},"options":{"plugins":{"datalabels":{"display":false}}}}"##,
        true,
    );
    assert!(ok.is_ok(), "datalabels should be accepted: {:?}", ok);
}
```

**Step 2: 新テストを実行して全パスを確認**

```bash
cargo test -p fulgur-chart --test render_progress
```
Expected: 21 passed, 0 failed（既存 15 + 新規 6）

**Step 3: Commit**

```bash
git add crates/fulgur-chart/tests/render_progress.rs
git commit -m "test(progress): verify borderColor/borderWidth/legend are rejected"
```

---

### Task 4: frontend — check_unknown_keys_progress() を追加

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`

**Step 1: `check_unknown_keys_gauge` 関数（844行目付近）の直後に新関数を追加**

```rust
/// progress / progressBar の許可キーに対して検証する。
/// stroke を描かないため borderColor/borderWidth は受け付けない。
/// legend は描画しないため受け付けない（datalabels は % 表示制御に使用するため許可）。
fn check_unknown_keys_progress(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["labels", "datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    // progress はストロークを描かないため borderColor/borderWidth は拒否
                    check_object(
                        ds,
                        &["label", "data", "backgroundColor"],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            // legend は progress では描画しないため拒否
            check_object(plugins, &["title", "datalabels"], "options.plugins")?;
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &[
                    "palette",
                    "gridColor",
                    "textColor",
                    "backgroundColor",
                    "fontSize",
                ],
                "options.theme",
            )?;
        }
    }
    Ok(())
}
```

**Step 2: strict ディスパッチに progress/progressBar 分岐を追加**

`frontend/chartjs.rs` の 237行目付近（gauge 分岐の直後）に追加:

```rust
        if matches!(chart_type.as_deref(), Some("progress") | Some("progressBar")) {
            if strict {
                check_unknown_keys_progress(json)?;
            }
        }
```

注意: この分岐は `return` しない（progress は汎用 `parse_raw` パスで処理される）。gauge/matrix と異なり、専用パーサは不要。

**Step 3: ビルドを確認**

```bash
cargo build -p fulgur-chart 2>&1 | head -30
```
Expected: エラーなし

**Step 4: 全テストを実行**

```bash
cargo test -p fulgur-chart --test render_progress
```
Expected: 21 passed, 0 failed

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs
git commit -m "fix(progress): add check_unknown_keys_progress for strict mode"
```

---

### Task 5: 最終検証

**Step 1: fulgur-chart 全テストを実行**

```bash
cargo test -p fulgur-chart
```
Expected: 全パス、0 failures

**Step 2: CLI テストを実行**

```bash
cargo test -p fulgur-chart-cli
```
Expected: 全パス

**Step 3: スナップショットの差分確認**

```bash
cargo insta review 2>/dev/null || echo "no snapshots to review"
```
Expected: スナップショット変更なし（progress の見た目は変わらない）

**Step 4: Commit（変更がなければスキップ）**

変更があった場合のみ:
```bash
git add -u
git commit -m "chore: update snapshots for progress cleanup"
```
