# Python Binding CI Job Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `.github/workflows/ci.yml` に `python-binding` ジョブを追加し、Python バインディングのリグレッションを CI で自動検出できるようにする。

**Architecture:** Ruby バインディング用の `ruby-binding` ジョブと同構造で、fmt/clippy/maturin develop/pytest を順に実行する。`crates/bindings/python` は root workspace の exclude 対象のため、専用ジョブが必要。

**Tech Stack:** GitHub Actions, maturin (pyo3), pytest, actions/setup-python@v5

---

### Task 1: `python-binding` ジョブを ci.yml に追加する

**Files:**
- Modify: `.github/workflows/ci.yml`（末尾に新ジョブを追加）

**Step 1: ci.yml の末尾に python-binding ジョブを追記する**

`.github/workflows/ci.yml` の `ruby-binding:` ジョブの直後に以下を追加する:

```yaml
  python-binding:
    name: Python binding (build + smoke)
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: crates/bindings/python
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: crates/bindings/python
          key: python-binding

      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"

      - name: Format check
        run: cargo fmt --manifest-path Cargo.toml -- --check

      - name: Clippy
        run: cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings

      - name: Install maturin + pytest
        run: pip install maturin pytest

      - name: Build (maturin develop)
        run: maturin develop

      - name: Test
        run: pytest tests/
```

**Step 2: YAML 構文を確認する**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

期待: `YAML OK`

**Step 3: ジョブが追加されたことを確認する**

```bash
grep -n "python-binding" .github/workflows/ci.yml
```

期待: `python-binding:` と `Python binding` の2行がヒットする

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add python-binding job (maturin develop + pytest)"
```
