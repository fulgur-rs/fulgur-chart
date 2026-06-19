# DSL Auto-Detection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** When `--dsl` is omitted, infer chartjs vs vegalite from the spec's top-level JSON keys.

**Architecture:** Add `detect_dsl(json: &str) -> Result<&'static str, String>` in the CLI (`main.rs`). Change `RenderArgs.dsl` from `String` (default "chartjs") to `Option<String>`. Resolve the DSL at the top of `render_one`: use the explicit value when provided, call `detect_dsl` when not. `parse_spec` and the library are unchanged.

**Tech Stack:** Rust, clap 4, serde_json (already a dependency)

---

### Task 1: Add `detect_dsl` with unit tests

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs` — add function + `#[cfg(test)]` block

**Step 1: Write the failing tests**

Add to the bottom of `main.rs`:

```rust
#[cfg(test)]
mod detect_dsl_tests {
    use super::detect_dsl;

    #[test]
    fn type_key_detects_chartjs() {
        assert_eq!(detect_dsl(r#"{"type":"bar","data":{}}"#).unwrap(), "chartjs");
    }

    #[test]
    fn mark_key_detects_vegalite() {
        assert_eq!(detect_dsl(r#"{"mark":"bar","data":{"values":[]}}"#).unwrap(), "vegalite");
    }

    #[test]
    fn mark_takes_priority_over_type() {
        // a spec with both keys (unusual): mark wins
        assert_eq!(detect_dsl(r#"{"mark":"bar","type":"x"}"#).unwrap(), "vegalite");
    }

    #[test]
    fn no_known_key_is_err() {
        assert!(detect_dsl(r#"{"labels":[]}"#).is_err());
    }

    #[test]
    fn invalid_json_is_err() {
        assert!(detect_dsl("not json").is_err());
    }
}
```

**Step 2: Run test to verify they fail**

```bash
cd crates/fulgur-chart-cli
cargo test detect_dsl 2>&1
```
Expected: FAIL — `detect_dsl` not defined yet.

**Step 3: Implement `detect_dsl`**

Add before `fn run_schema` in `main.rs`:

```rust
fn detect_dsl(json: &str) -> Result<&'static str, String> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("error: invalid JSON: {e}"))?;
    if v.get("mark").is_some() {
        return Ok("vegalite");
    }
    if v.get("type").is_some() {
        return Ok("chartjs");
    }
    Err("error: cannot auto-detect DSL: specify --dsl chartjs or --dsl vegalite".to_string())
}
```

**Step 4: Run tests to verify they pass**

```bash
cargo test detect_dsl 2>&1
```
Expected: 5 tests pass.

**Step 5: Commit**

```bash
git add crates/fulgur-chart-cli/src/main.rs
git commit -m "feat: add detect_dsl for DSL auto-detection"
```

---

### Task 2: Change `RenderArgs.dsl` to `Option<String>` and update validation

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs` lines ~60-95

**Step 1: Change the field type and doc comment**

In `RenderArgs`, change:
```rust
    /// Input DSL. Supported values: chartjs, vegalite.
    #[arg(long, default_value = "chartjs")]
    dsl: String,
```
to:
```rust
    /// Input DSL (chartjs or vegalite). Auto-detected from the spec when omitted.
    #[arg(long)]
    dsl: Option<String>,
```

**Step 2: Update the validation in `run_render`**

Change:
```rust
    // Validate DSL; only chartjs and vegalite are supported.
    if args.dsl != "chartjs" && args.dsl != "vegalite" {
        eprintln!(
            "error: unsupported DSL '{}' (supported: chartjs, vegalite)",
            args.dsl
        );
        std::process::exit(1);
    }
```
to:
```rust
    // Validate explicit DSL; only chartjs and vegalite are supported.
    if let Some(dsl) = &args.dsl {
        if dsl != "chartjs" && dsl != "vegalite" {
            eprintln!("error: unsupported DSL '{dsl}' (supported: chartjs, vegalite)");
            std::process::exit(1);
        }
    }
```

**Step 3: Update `render_one` to resolve the DSL**

At the top of `render_one`, before `parse_spec`, add DSL resolution.

Change:
```rust
    // Parse non-strictly; JSON/structure/type errors exit 1.
    let mut spec_ir =
        parse_spec(json, &args.dsl, false).map_err(|e| (1, format!("error: parse failed: {e}")))?;

    // When --strict is set, re-parse with strict mode to catch unknown keys (exit 2).
    // Rendering still uses the non-strict IR parsed above.
    if args.strict {
        parse_spec(json, &args.dsl, true)
            .map_err(|e| (2, format!("error: strict violation: {e}")))?;
    }
```
to:
```rust
    // Resolve DSL: use explicit --dsl, or auto-detect from the spec's top-level keys.
    let dsl: &str = match &args.dsl {
        Some(d) => d.as_str(),
        None => detect_dsl(json).map_err(|e| (1, e))?,
    };

    // Parse non-strictly; JSON/structure/type errors exit 1.
    let mut spec_ir =
        parse_spec(json, dsl, false).map_err(|e| (1, format!("error: parse failed: {e}")))?;

    // When --strict is set, re-parse with strict mode to catch unknown keys (exit 2).
    // Rendering still uses the non-strict IR parsed above.
    if args.strict {
        parse_spec(json, dsl, true)
            .map_err(|e| (2, format!("error: strict violation: {e}")))?;
    }
```

**Step 4: Verify it compiles and all tests pass**

```bash
cargo test --workspace 2>&1
```
Expected: all tests pass. Some existing tests that relied on the default "chartjs" behavior should still pass because they either pass a chartjs spec (auto-detected) or use `--dsl chartjs` explicitly.

**Step 5: Commit**

```bash
git add crates/fulgur-chart-cli/src/main.rs
git commit -m "feat: auto-detect DSL when --dsl is omitted"
```

---

### Task 3: Add CLI integration tests for auto-detection

**Files:**
- Modify: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: Write the failing tests**

Add these tests to `cli.rs`:

```rust
#[test]
fn auto_detect_chartjs_without_dsl_flag() {
    // chartjs spec (has "type" key) renders correctly without --dsl
    let spec = r#"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#;
    Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success()
        .stdout(predicates::str::starts_with("<svg"));
}

#[test]
fn auto_detect_vegalite_without_dsl_flag() {
    // vegalite spec (has "mark" key) renders correctly without --dsl
    let spec = r#"{"mark":"bar","data":{"values":[{"x":"A","y":1}]},"encoding":{"x":{"field":"x"},"y":{"field":"y"}}}"#;
    Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .success()
        .stdout(predicates::str::starts_with("<svg"));
}

#[test]
fn auto_detect_unknown_spec_exits_1() {
    // spec with neither "type" nor "mark" gives a clear error
    let spec = r#"{"labels":["A"],"values":[1]}"#;
    Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["render", "-", "-o", "-"])
        .write_stdin(spec)
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("auto-detect"));
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p fulgur-chart-cli auto_detect 2>&1
```
Expected: FAIL — feature not yet active (if running before Task 2 is merged, or compile error).

**Step 3: Run all tests to confirm full suite passes**

```bash
cargo test --workspace 2>&1
```
Expected: all tests pass including the 3 new ones.

**Step 4: Commit**

```bash
git add crates/fulgur-chart-cli/tests/cli.rs
git commit -m "test: CLI integration tests for DSL auto-detection"
```

---

### Task 4: Cargo fmt + final check

**Step 1: Format**

```bash
cargo fmt --all
```

**Step 2: Clippy**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

**Step 3: Full test suite**

```bash
cargo test --workspace
```

**Step 4: Commit if fmt changed anything**

```bash
git add -A
git diff --cached --quiet || git commit -m "style: cargo fmt"
```
