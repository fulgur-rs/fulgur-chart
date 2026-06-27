# Jsonnet Input Format Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** CLI で `.jsonnet` ファイルおよび stdin の Jsonnet 入力をサポートする（Jsonnet → JSON 評価後、既存の chartjs/vegalite パーサーへ渡す）。

**Architecture:** `jrsonnet-evaluator` を `fulgur-chart-cli` のみの依存として追加。`render` / `inspect` サブコマンドに `--jsonnet` フラグ（stdin 専用）と `.jsonnet` 拡張子自動検出を追加。評価は `State::enter()` + `FileImportResolver::default()` で行い、ファイル入力は `state.import(path)`、stdin は `state.evaluate_snippet()` を使う。

**Tech Stack:** Rust, jrsonnet-evaluator 0.5.0-pre96（実際には pre98 が解決される）, clap, assert_cmd

---

### Task 1: 依存追加

**Files:**
- Modify: `crates/fulgur-chart-cli/Cargo.toml`

**Step 1: Cargo.toml に jrsonnet-evaluator を追加**

`[dependencies]` セクションに追加：

```toml
jrsonnet-evaluator = "0.5.0-pre96"
```

**Step 2: ビルドが通ることを確認**

```bash
cargo build -p fulgur-chart-cli 2>&1 | tail -5
```

Expected: `Finished` が出る（jrsonnet が大量のクレートをコンパイルするので数分かかる）

**Step 3: コミット**

```bash
git add crates/fulgur-chart-cli/Cargo.toml Cargo.lock
git commit -m "chore(cli): add jrsonnet-evaluator dependency"
```

---

### Task 2: evaluate_jsonnet ヘルパーの実装（TDD）

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs`

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart-cli/tests/cli.rs` の末尾に追加（`use assert_cmd::Command;` は既存）：

```rust
// --- Jsonnet サポート ---

const MINIMAL_JSONNET_STDIN: &str = r#"
// コメント付き bar チャート
{
  type: "bar",
  data: {
    labels: ["A", "B"],
    datasets: [{ data: [1, 2] }],
  },
}
"#;

#[test]
fn jsonnet_stdin_renders_svg() {
    let out = bin()
        .args(["render", "-", "-o", "-", "--jsonnet"])
        .write_stdin(MINIMAL_JSONNET_STDIN)
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"), "expected SVG, got: {s}");
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart-cli jsonnet_stdin_renders_svg 2>&1 | tail -10
```

Expected: `error: Found argument '--jsonnet'` のようなエラーで失敗

**Step 3: evaluate_jsonnet 関数と --jsonnet フラグを実装**

`main.rs` に以下を追加：

`use` セクションに追加：
```rust
use jrsonnet_evaluator::{
    manifest::{JsonFormat, ManifestFormat},
    FileImportResolver, State,
};
```

`RenderArgs` struct に追加（`--strict` の後あたり）：
```rust
/// Evaluate input as Jsonnet before parsing. Only valid with stdin ('-').
#[arg(long)]
jsonnet: bool,
```

`main.rs` の関数として追加（`run_render` の前あたり）：

```rust
/// Jsonnet ソース文字列を JSON 文字列に評価する。
/// ファイル入力時は base_dir を指定して import を解決する。
/// stdin（base_dir=None）の場合は import を使用不可。
fn evaluate_jsonnet(src: &str, base_dir: Option<&std::path::Path>) -> Result<String, String> {
    let mut b = State::builder();
    b.import_resolver(FileImportResolver::default());
    let state = b.build();
    let _guard = state.enter();

    let val = match base_dir {
        Some(dir) => {
            let path = dir.canonicalize().map_err(|e| format!("error: {e}"))?;
            // base_dir を一時ファイルとして扱うのではなく、そのファイルを import する
            // この呼び出しは既に Task 3 でファイルパスを渡す設計に変わるが、
            // stdin 用ではソースコードから evaluate_snippet を使う。
            state
                .evaluate_snippet(
                    jrsonnet_evaluator::IStr::from("(stdin)"),
                    jrsonnet_evaluator::IStr::from(src),
                )
                .map_err(|e| format!("{e}"))?
        }
        None => state
            .evaluate_snippet(
                jrsonnet_evaluator::IStr::from("(stdin)"),
                jrsonnet_evaluator::IStr::from(src),
            )
            .map_err(|e| format!("{e}"))?,
    };

    JsonFormat::default()
        .manifest(val)
        .map_err(|e| format!("{e}"))
}
```

**注意:** この段階では stdin のみ対応。ファイル対応は Task 3 で行う。

`run_single` 関数内の `let json = match read_spec(spec_path)...` の後に追加：

```rust
// --jsonnet フラグが立っていたら Jsonnet として評価
let json = if args.jsonnet {
    match evaluate_jsonnet(&json, None) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
} else {
    json
};
```

**Step 4: テストが通ることを確認**

```bash
cargo test -p fulgur-chart-cli jsonnet_stdin_renders_svg 2>&1 | tail -10
```

Expected: `test jsonnet_stdin_renders_svg ... ok`

**Step 5: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): add --jsonnet flag and evaluate_jsonnet helper for stdin"
```

---

### Task 3: --jsonnet + ファイルパスはエラー（TDD）

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`
- Modify: `crates/fulgur-chart-cli/src/main.rs`

**Step 1: 失敗するテストを書く**

`tests/cli.rs` に追加：

```rust
#[test]
fn jsonnet_flag_with_file_path_exits_1() {
    // ファイルと --jsonnet の組み合わせは不正（拡張子を使うべき）
    let dir = tempfile_dir();
    let spec = dir.join("spec.json");
    std::fs::write(&spec, MINIMAL_BAR_A).unwrap();
    bin()
        .args(["render", spec.to_str().unwrap(), "-o", "-", "--jsonnet"])
        .assert()
        .failure()
        .code(1);
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart-cli jsonnet_flag_with_file_path_exits_1 2>&1 | tail -10
```

Expected: テストが失敗（現在は --jsonnet + ファイルでも成功してしまう）

**Step 3: バリデーションを run_single に追加**

`run_single` の先頭（args.spec.is_empty() チェックの後）に追加：

```rust
// --jsonnet はファイル入力と組み合わせ不可（.jsonnet 拡張子を使うこと）
if args.jsonnet && spec_path != "-" {
    eprintln!("error: --jsonnet is only valid with stdin ('-'). For .jsonnet files, use the .jsonnet extension.");
    std::process::exit(1);
}
```

**Step 4: テストが通ることを確認**

```bash
cargo test -p fulgur-chart-cli jsonnet_flag_with_file_path_exits_1 2>&1 | tail -10
```

Expected: `test jsonnet_flag_with_file_path_exits_1 ... ok`

**Step 5: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): reject --jsonnet with file path (use .jsonnet extension instead)"
```

---

### Task 4: .jsonnet ファイルの自動検出（TDD）

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`
- Modify: `crates/fulgur-chart-cli/src/main.rs`

**Step 1: 失敗するテストを書く**

`tests/cli.rs` に追加：

```rust
const MINIMAL_JSONNET_FILE: &str = r#"
// Jsonnet で書いた bar チャート
{
  type: "bar",
  data: {
    labels: ["X", "Y"],
    datasets: [{ data: [3, 7] }],
  },
}
"#;

#[test]
fn jsonnet_file_renders_svg() {
    let dir = tempfile_dir();
    let spec = dir.join("spec.jsonnet");
    std::fs::write(&spec, MINIMAL_JSONNET_FILE).unwrap();
    let out = bin()
        .args(["render", spec.to_str().unwrap(), "-o", "-"])
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"), "expected SVG, got: {s}");
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart-cli jsonnet_file_renders_svg 2>&1 | tail -10
```

Expected: exit 1 でエラー（`.jsonnet` を通常 JSON として扱うと DSL 検出失敗）

**Step 3: ファイル入力の Jsonnet 評価を実装**

まず `evaluate_jsonnet` 関数を整理する。stdin 向け（`evaluate_snippet`）とファイル向け（`state.import(path)`）を分けて実装：

`main.rs` の `evaluate_jsonnet` 関数を以下に置き換え：

```rust
/// Jsonnet ソース文字列を JSON に評価（stdin 用）。import は CWD 相対で解決。
fn evaluate_jsonnet_snippet(src: &str) -> Result<String, String> {
    let mut b = State::builder();
    b.import_resolver(FileImportResolver::default());
    let state = b.build();
    let _guard = state.enter();
    let val = state
        .evaluate_snippet(
            jrsonnet_evaluator::IStr::from("(stdin)"),
            jrsonnet_evaluator::IStr::from(src),
        )
        .map_err(|e| format!("{e}"))?;
    JsonFormat::default()
        .manifest(val)
        .map_err(|e| format!("{e}"))
}

/// .jsonnet ファイルを JSON に評価。import はファイルのディレクトリから解決。
fn evaluate_jsonnet_file(path: &std::path::Path) -> Result<String, String> {
    let mut b = State::builder();
    b.import_resolver(FileImportResolver::default());
    let state = b.build();
    let _guard = state.enter();
    let val = state
        .import(path)
        .map_err(|e| format!("{e}"))?;
    JsonFormat::default()
        .manifest(val)
        .map_err(|e| format!("{e}"))
}
```

`.jsonnet` 拡張子検出ヘルパーを追加：

```rust
fn is_jsonnet_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jsonnet"))
}
```

`run_single` 内の Jsonnet 評価部分を以下に更新（既存の `--jsonnet` フラグ処理と拡張子検出を統合）：

```rust
// Jsonnet の評価: --jsonnet フラグ（stdin 専用）または .jsonnet 拡張子
let json = if args.jsonnet {
    // --jsonnet は stdin のみ（ファイルとの組み合わせは前段でブロック済み）
    match evaluate_jsonnet_snippet(&json) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: jsonnet evaluation failed: {e}");
            std::process::exit(1);
        }
    }
} else if is_jsonnet_path(spec_path) {
    match evaluate_jsonnet_file(std::path::Path::new(spec_path)) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: jsonnet evaluation failed: {e}");
            std::process::exit(1);
        }
    }
} else {
    json
};
```

**Step 4: テストが通ることを確認**

```bash
cargo test -p fulgur-chart-cli jsonnet_file_renders_svg 2>&1 | tail -10
```

Expected: `test jsonnet_file_renders_svg ... ok`

**Step 5: 既存テストも通ることを確認**

```bash
cargo test -p fulgur-chart-cli 2>&1 | tail -15
```

Expected: 全テスト通過

**Step 6: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): auto-detect .jsonnet extension and evaluate via jrsonnet"
```

---

### Task 5: import のサポートテスト（TDD）

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: import を使った .jsonnet テストを書く**

```rust
#[test]
fn jsonnet_file_with_import_renders_svg() {
    let dir = tempfile_dir();

    // ライブラリファイル
    std::fs::write(
        dir.join("colors.libsonnet"),
        r#"{ red: "rgb(255,0,0)", blue: "rgb(0,0,255)" }"#,
    )
    .unwrap();

    // メインスペック（import あり）
    let spec = dir.join("spec.jsonnet");
    std::fs::write(
        &spec,
        r#"
local colors = import 'colors.libsonnet';
{
  type: "bar",
  data: {
    labels: ["A"],
    datasets: [{ backgroundColor: colors.red, data: [1] }],
  },
}
"#,
    )
    .unwrap();

    let out = bin()
        .args(["render", spec.to_str().unwrap(), "-o", "-"])
        .assert()
        .success();
    let s = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(s.starts_with("<svg"), "expected SVG, got: {s}");
}
```

**Step 2: テストを実行**

```bash
cargo test -p fulgur-chart-cli jsonnet_file_with_import_renders_svg 2>&1 | tail -10
```

Expected: `test jsonnet_file_with_import_renders_svg ... ok`（Task 4 の実装で既に動くはず）

もし失敗したら: `evaluate_jsonnet_file` の実装を見直す。

**Step 3: コミット**

```bash
git add crates/fulgur-chart-cli/tests/cli.rs
git commit -m "test(cli): verify .jsonnet import resolution"
```

---

### Task 6: .libsonnet 直接入力はエラー（TDD）

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: テストを書く**

```rust
#[test]
fn libsonnet_direct_input_exits_1() {
    // .libsonnet は直接入力に使えない（DSL 検出失敗として扱う）
    let dir = tempfile_dir();
    let lib = dir.join("lib.libsonnet");
    std::fs::write(&lib, r#"{ x: 1 }"#).unwrap();
    let out = bin()
        .args(["render", lib.to_str().unwrap(), "-o", "-"])
        .assert()
        .failure()
        .code(1);
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("auto-detect") || stderr.contains("DSL"),
        "stderr should mention DSL detection, got: {stderr}"
    );
}
```

**Step 2: テストを実行**

```bash
cargo test -p fulgur-chart-cli libsonnet_direct_input_exits_1 2>&1 | tail -10
```

Expected: `test libsonnet_direct_input_exits_1 ... ok`

`.libsonnet` は Jsonnet として評価されるが、JSON オブジェクト `{ x: 1 }` は chartjs/vegalite どちらにもマッチしないため DSL 検出失敗 → exit 1 となる（追加実装不要）。

もし通らない（0 で成功してしまう）場合のみ、`is_jsonnet_path` を以下に変更：

```rust
fn is_jsonnet_path(path: &str) -> bool {
    // .libsonnet は直接入力として使用不可
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    ext.eq_ignore_ascii_case("jsonnet")
}
```

（`.libsonnet` は `jsonnet` にマッチしないのでデフォルトで対象外）

**Step 3: コミット**

```bash
git add crates/fulgur-chart-cli/tests/cli.rs
git commit -m "test(cli): verify .libsonnet direct input fails with DSL detection error"
```

---

### Task 7: inspect サブコマンドの Jsonnet 対応

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs`
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: テストを書く**

```rust
#[test]
fn jsonnet_inspect_emits_model() {
    let dir = tempfile_dir();
    let spec = dir.join("spec.jsonnet");
    std::fs::write(&spec, MINIMAL_JSONNET_FILE).unwrap();
    let out = bin()
        .args(["inspect", spec.to_str().unwrap(), "-o", "-"])
        .assert()
        .success();
    let bytes = out.get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(v["meta"]["type"], "bar");
}

#[test]
fn jsonnet_stdin_inspect_emits_model() {
    let out = bin()
        .args(["inspect", "-", "-o", "-", "--jsonnet"])
        .write_stdin(MINIMAL_JSONNET_STDIN)
        .assert()
        .success();
    let bytes = out.get_output().stdout.clone();
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    assert_eq!(v["meta"]["type"], "bar");
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart-cli "jsonnet_inspect" 2>&1 | tail -10
```

Expected: 失敗

**Step 3: InspectArgs に --jsonnet フラグを追加し、run_inspect に評価ロジックを追加**

`InspectArgs` struct に追加（`--font` の前）：
```rust
/// Evaluate input as Jsonnet before parsing. Only valid with stdin ('-').
#[arg(long)]
jsonnet: bool,
```

`run_inspect` 内の `let json = match read_spec(&args.spec)` の後に追加：

```rust
// --jsonnet は stdin 専用。.jsonnet 拡張子は自動検出。
if args.jsonnet && args.spec != "-" {
    eprintln!("error: --jsonnet is only valid with stdin ('-'). For .jsonnet files, use the .jsonnet extension.");
    std::process::exit(1);
}
let json = if args.jsonnet {
    match evaluate_jsonnet_snippet(&json) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: jsonnet evaluation failed: {e}");
            std::process::exit(1);
        }
    }
} else if is_jsonnet_path(&args.spec) {
    match evaluate_jsonnet_file(std::path::Path::new(&args.spec)) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: jsonnet evaluation failed: {e}");
            std::process::exit(1);
        }
    }
} else {
    json
};
```

**Step 4: テストが通ることを確認**

```bash
cargo test -p fulgur-chart-cli "jsonnet_inspect" 2>&1 | tail -10
```

Expected: 全テスト通過

**Step 5: 全テストが通ることを確認**

```bash
cargo test -p fulgur-chart-cli 2>&1 | tail -15
```

**Step 6: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): add Jsonnet support to inspect subcommand"
```

---

### Task 8: Jsonnet 評価失敗のエラーメッセージ確認（TDD）

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: テストを書く**

```rust
#[test]
fn jsonnet_syntax_error_exits_1() {
    bin()
        .args(["render", "-", "-o", "-", "--jsonnet"])
        .write_stdin("{ invalid jsonnet ::::")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn jsonnet_file_syntax_error_exits_1() {
    let dir = tempfile_dir();
    let spec = dir.join("bad.jsonnet");
    std::fs::write(&spec, "{ not valid jsonnet ::::").unwrap();
    bin()
        .args(["render", spec.to_str().unwrap(), "-o", "-"])
        .assert()
        .failure()
        .code(1);
}
```

**Step 2: テストを実行**

```bash
cargo test -p fulgur-chart-cli "jsonnet_syntax_error\|jsonnet_file_syntax_error" 2>&1 | tail -10
```

Expected: 両テストとも通過（既存の実装で exit 1 になるはず）

**Step 3: コミット（テストのみ）**

```bash
git add crates/fulgur-chart-cli/tests/cli.rs
git commit -m "test(cli): verify Jsonnet syntax errors exit 1"
```

---

### Task 9: バッチモードの Jsonnet 対応

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs`
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: テストを書く**

```rust
#[test]
fn batch_renders_jsonnet_files() {
    let dir = batch_dir("batch_renders_jsonnet_files");
    let in_dir = dir.join("in");
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&in_dir).unwrap();

    std::fs::write(in_dir.join("a.jsonnet"), MINIMAL_JSONNET_FILE).unwrap();
    std::fs::write(in_dir.join("b.jsonnet"), MINIMAL_JSONNET_FILE).unwrap();

    bin()
        .args([
            "render",
            in_dir.join("a.jsonnet").to_str().unwrap(),
            in_dir.join("b.jsonnet").to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let sa = std::fs::read_to_string(out_dir.join("a.svg")).unwrap();
    assert!(sa.starts_with("<svg"));
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test -p fulgur-chart-cli batch_renders_jsonnet_files 2>&1 | tail -10
```

**Step 3: run_batch に Jsonnet 評価を追加**

`run_batch` 内の `let json = match std::fs::read_to_string(spec_path)...` の後に追加：

```rust
let json = if is_jsonnet_path(spec_path) {
    match evaluate_jsonnet_file(std::path::Path::new(spec_path)) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("{spec_path}: error: jsonnet evaluation failed: {e}");
            std::process::exit(1);
        }
    }
} else {
    json
};
```

**Step 4: テストが通ることを確認**

```bash
cargo test -p fulgur-chart-cli batch_renders_jsonnet_files 2>&1 | tail -10
cargo test -p fulgur-chart-cli 2>&1 | tail -15
```

Expected: 全テスト通過

**Step 5: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): support .jsonnet files in batch mode"
```

---

### Task 10: 最終確認

**Step 1: ワークスペース全体のテストが通ることを確認**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: 全テスト通過

**Step 2: ヘルプテキストに Jsonnet が記載されているか確認**

```bash
cargo run -p fulgur-chart-cli -- render --help 2>&1 | grep -i jsonnet
cargo run -p fulgur-chart-cli -- inspect --help 2>&1 | grep -i jsonnet
```

**Step 3: 手動スモークテスト**

```bash
echo '{ type: "bar", data: { labels: ["A", "B"], datasets: [{ data: [1, 2] }] } }' \
  | cargo run -p fulgur-chart-cli -- render - -o - --jsonnet 2>&1 | head -3
```

Expected: `<svg` から始まる出力

**Step 4: コミット（変更があれば）**

```bash
git add -p
git commit -m "docs: update render/inspect help for jsonnet flag"
```
