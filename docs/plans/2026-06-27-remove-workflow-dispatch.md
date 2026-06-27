# Remove workflow_dispatch from ruby-gem-release Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `ruby-gem-release.yml` から `workflow_dispatch` トリガーを削除し、`release: [published]` のみをリリーストリガーにする。

**Architecture:** GitHub Actions の `on:` ブロックと、`workflow_dispatch` 専用 fallback 式 (`github.event.inputs.tag || github.ref_name`) を `github.ref_name` 単独に置き換える。コード変更なし、YAMLのみ。

**Tech Stack:** GitHub Actions YAML

---

## Task 1: workflow_dispatch トリガー削除と式の統一

**Files:**
- Modify: `.github/workflows/ruby-gem-release.yml`

**Step 1: `workflow_dispatch` ブロックを削除**

`.github/workflows/ruby-gem-release.yml` の `on:` ブロックを以下に変更する:

変更前:
```yaml
on:
  release:
    types: [published]
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag (e.g. fulgur-chart-v0.3.0)"
        required: true
```

変更後:
```yaml
on:
  release:
    types: [published]
```

**Step 2: `validate-release-tag` の `if` 条件を簡素化**

変更前 (line 22):
```yaml
    if: startsWith(github.event.inputs.tag || github.ref_name, 'fulgur-chart-v')
```

変更後:
```yaml
    if: startsWith(github.ref_name, 'fulgur-chart-v')
```

**Step 3: `RELEASE_TAG` env var を簡素化**

変更前 (line 32):
```yaml
          RELEASE_TAG: ${{ github.event.inputs.tag || github.ref_name }}
```

変更後:
```yaml
          RELEASE_TAG: ${{ github.ref_name }}
```

**Step 4: YAML 構文を確認**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ruby-gem-release.yml'))" && echo "YAML valid"
```

Expected: `YAML valid`

**Step 5: Commit**

```bash
git add .github/workflows/ruby-gem-release.yml
git commit -m "ci(ruby): remove workflow_dispatch from ruby-gem-release"
```
