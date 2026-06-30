# Pin GitHub Actions to SHAs (repo-wide hardening) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development) to implement this plan task-by-task.

**Goal:** Pin every GitHub Action `uses:` reference (third-party AND first-party `actions/*`) across the 6 remaining workflows to an immutable commit SHA + `# version` comment, and add Dependabot to keep them current.

**Architecture:** Mechanical-but-careful. Straightforward `@ref` → `@<sha> # <ver>` swaps are done in bulk; the *ref-as-selector* actions (`dtolnay/rust-toolchain`, `taiki-e/install-action`) additionally need a `with:` selector (`toolchain:` / `tool:`) because the channel/tool information carried by the ref is lost when the ref becomes a SHA. `release-plz.yml` is already fully pinned → out of scope.

**Tech Stack:** GitHub Actions YAML, Dependabot, actionlint (verification).

**Beads issue:** fulgur-chart-kwn

---

## SHA Reference Table (resolved 2026-07-01 via `git ls-remote`, annotated tags dereferenced with `^{}`)

| Action | Old ref | Pin to | Comment |
|---|---|---|---|
| actions/checkout | @v5 | `93cb6efe18208431cddfb8368fd83d5badbf9bfd` | `# v5.0.1` |
| actions/checkout | @v4 | `34e114876b0b11c390a56381ad16ebd13914f8d5` | `# v4.3.1` |
| actions/setup-node | @v6 | `48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e` | `# v6.4.0` |
| actions/setup-node | @v4 | `49933ea5288caeca8642d1e84afbd3f7d6820020` | `# v4.4.0` |
| actions/upload-artifact | @v4 | `ea165f8d65b6e75b540449e92b4886f43607fa02` | `# v4.6.2` |
| actions/download-artifact | @v4 | `d3f86a106a0bac45b974a628896c90dbdf5c8093` | `# v4.3.0` |
| actions/cache | @v4 | `0057852bfaa89a56745cba8c7296529d2fc39830` | `# v4.3.0` |
| actions/setup-python | @v5 | `a26af69be951a213d495a4c3e4e4022e16d87065` | `# v5.6.0` |
| dtolnay/rust-toolchain | @stable | `29eef336d9b2848a0b548edc03f92a220660cdb8` | `# stable` |
| dtolnay/rust-toolchain | @1.89 | `193d6aa1dbbc28bd2c0a6b0e327cfdce68baaf6e` | `# 1.89.0` |
| Swatinem/rust-cache | @v2 | `c19371144df3bb44fab255c43d04cbc2ab54d1c4` | `# v2.9.1` |
| taiki-e/install-action | @cross / @cargo-llvm-cov / @wasm-pack | `9bcaee1dcae34154180f412e2fa69355a7cda9f6` | `# v2.82.6` |
| codecov/codecov-action | @v5 | `0fb7174895f61a3b6b78fc075e0cd60383518dac` | `# v5.5.5` |
| docker/login-action | @v3 | `c94ce9fb468520275223c153574b00df6fe4bcc9` | `# v3.7.0` |
| docker/metadata-action | @v5 | `c299e40c65443455700f0fdfc63efafe5b349051` | `# v5.10.0` |
| docker/build-push-action | @v5 | `ca052bb54ab0790a636c9b5f226502c73d547a25` | `# v5.4.0` |
| oxidize-rb/actions/setup-ruby-and-rust | @v1 | `e5f9a49a7812a078584072f6e3f657ad247c8771` | `# v1.4.4` |
| oxidize-rb/actions/cross-gem | @v1 | `e5f9a49a7812a078584072f6e3f657ad247c8771` | `# v1.4.4` |
| ruby/setup-ruby | @v1 | `0dafeac902942906541bc140009cdbf32665b601` | `# v1.315.0` |
| rubygems/configure-rubygems-credentials | @v1.0.0 | `bc6dd217f8a4f919d6835fcfefd470ef821f5c44` | `# v1.0.0` |

**Decisions:**
- `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache`, `taiki-e/install-action` reuse the exact SHAs already pinned in `node-npm-release.yml` (PR #98) for repo-wide uniformity.
- Majors are NOT unified (checkout v5/v4, setup-node v4/v6 stay as-is); version bumps are left to Dependabot.
- `oxidize-rb/actions/*` subdir actions resolve from the `oxidize-rb/actions` repo's v1 tag; the path is preserved.

---

## Task 1: Bulk ref→SHA swaps (non-selector actions)

These swaps do NOT need a `with:` change — they are pure `@ref` → `@<sha> # <ver>` replacements. Applies to: `actions/*`, `Swatinem/rust-cache`, `codecov/*`, `docker/*`, `oxidize-rb/*`, `ruby/setup-ruby`, `rubygems/*`, and `dtolnay/rust-toolchain` lines that already have a `with:` block (the `toolchain:` key is added in Task 2).

**Files:** all 6 workflows under `.github/workflows/` (chart-cli-npm-release.yml, ruby-gem-release.yml, chart-server-docker.yml, ci.yml, chart-server-ci.yml, node-npm-release.yml).

**Step 1:** Apply the swaps with a sed script keyed on the exact `owner/repo@ref` string. Each pattern is anchored so already-pinned lines (which contain a SHA) and `release-plz.yml` (fully pinned) are NOT matched.

```bash
cd .github/workflows
declare -A MAP=(
  ["actions/checkout@v5"]="actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd # v5.0.1"
  ["actions/checkout@v4"]="actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4.3.1"
  ["actions/setup-node@v6"]="actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e # v6.4.0"
  ["actions/setup-node@v4"]="actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4.4.0"
  ["actions/upload-artifact@v4"]="actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2"
  ["actions/download-artifact@v4"]="actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4.3.0"
  ["actions/cache@v4"]="actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0"
  ["actions/setup-python@v5"]="actions/setup-python@a26af69be951a213d495a4c3e4e4022e16d87065 # v5.6.0"
  ["Swatinem/rust-cache@v2"]="Swatinem/rust-cache@c19371144df3bb44fab255c43d04cbc2ab54d1c4 # v2.9.1"
  ["codecov/codecov-action@v5"]="codecov/codecov-action@0fb7174895f61a3b6b78fc075e0cd60383518dac # v5.5.5"
  ["docker/login-action@v3"]="docker/login-action@c94ce9fb468520275223c153574b00df6fe4bcc9 # v3.7.0"
  ["docker/metadata-action@v5"]="docker/metadata-action@c299e40c65443455700f0fdfc63efafe5b349051 # v5.10.0"
  ["docker/build-push-action@v5"]="docker/build-push-action@ca052bb54ab0790a636c9b5f226502c73d547a25 # v5.4.0"
  ["oxidize-rb/actions/setup-ruby-and-rust@v1"]="oxidize-rb/actions/setup-ruby-and-rust@e5f9a49a7812a078584072f6e3f657ad247c8771 # v1.4.4"
  ["oxidize-rb/actions/cross-gem@v1"]="oxidize-rb/actions/cross-gem@e5f9a49a7812a078584072f6e3f657ad247c8771 # v1.4.4"
  ["ruby/setup-ruby@v1"]="ruby/setup-ruby@0dafeac902942906541bc140009cdbf32665b601 # v1.315.0"
  ["rubygems/configure-rubygems-credentials@v1.0.0"]="rubygems/configure-rubygems-credentials@bc6dd217f8a4f919d6835fcfefd470ef821f5c44 # v1.0.0"
  ["dtolnay/rust-toolchain@stable"]="dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable"
  ["dtolnay/rust-toolchain@1.89"]="dtolnay/rust-toolchain@193d6aa1dbbc28bd2c0a6b0e327cfdce68baaf6e # 1.89.0"
)
for f in chart-cli-npm-release.yml ruby-gem-release.yml chart-server-docker.yml ci.yml chart-server-ci.yml node-npm-release.yml; do
  for old in "${!MAP[@]}"; do
    new="${MAP[$old]}"
    # match "uses: <old>" only at end-of-token (followed by EOL) so SHAs/comments aren't touched
    sed -i -E "s#(uses: )${old//\//\\/}\$#\1${new//\//\\/}#" "$f"
  done
done
```

**Step 2 (verify):** every targeted ref is gone; expect 0 matches.

```bash
grep -rnE "uses: (actions/(checkout|setup-node|upload-artifact|download-artifact|cache|setup-python)|Swatinem/rust-cache|codecov/codecov-action|docker/(login|metadata|build-push)-action|oxidize-rb/actions/[a-z-]+|ruby/setup-ruby|rubygems/configure-rubygems-credentials|dtolnay/rust-toolchain)@(v[0-9.]+|stable|1\.89)\s*\$" .github/workflows/
```
Expected: no output.

**Note:** the `taiki-e/install-action@<tool>` refs are intentionally NOT in this map — they are handled in Task 2 because the ref IS the tool selector.

---

## Task 2: ref-as-selector conversions (the critical part)

For each occurrence below, the ref was/will be replaced by a SHA, so a `with:` selector MUST be present. Do these as targeted edits (NOT blind sed) because indentation differs per step.

### 2a. `dtolnay/rust-toolchain` lines that ALREADY have a `with:` block → ADD a `toolchain:` key as the FIRST key under the existing `with:`

(After Task 1 these lines read `dtolnay/rust-toolchain@29eef33… # stable`.) Add `toolchain: stable` (matching the existing key indentation):

- `ci.yml`: under `components: rustfmt, clippy` blocks (3×), `components: llvm-tools-preview` (1×), `targets: ${{ matrix.target }}` (1×), `targets: wasm32-unknown-unknown` (1×) — every `@stable` step that has a `with:`.
- `chart-server-docker.yml`: the `with: targets: x86_64-unknown-linux-musl` step.
- `chart-server-ci.yml`: the `with: components: clippy` step.
- `chart-cli-npm-release.yml`: the `with: targets: ${{ matrix.target }}` step.

Result shape:
```yaml
      - uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable
        with:
          toolchain: stable
          components: rustfmt, clippy
```

### 2b. `dtolnay/rust-toolchain@stable` with NO `with:` block → ADD a new `with:` block

`ci.yml` — 2 occurrences (the two `@stable` steps immediately followed by a blank line then `Swatinem/rust-cache`). Insert:
```yaml
      - uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable
        with:
          toolchain: stable
```

### 2c. `dtolnay/rust-toolchain@1.89` with NO `with:` block → ADD `with: toolchain: "1.89"`

`ci.yml` — 1 occurrence:
```yaml
      - uses: dtolnay/rust-toolchain@193d6aa1dbbc28bd2c0a6b0e327cfdce68baaf6e # 1.89.0
        with:
          toolchain: "1.89"
```

### 2d. `taiki-e/install-action@<tool>` → replace ref with SHA AND add `with: tool: <tool>`

Pin the ref to `9bcaee1dcae34154180f412e2fa69355a7cda9f6 # v2.82.6` and add the tool selector (mind the step indentation — some are `      - uses:`, some are `        uses:` under a `- name:`):

- `ci.yml` L62 → `tool: cargo-llvm-cov`
- `ci.yml` L108 → `tool: cross`
- `ci.yml` L137 → `tool: wasm-pack`
- `ci.yml` L356 → `tool: wasm-pack`
- `chart-cli-npm-release.yml` L145 → `tool: cross`

Result shape (8-space variant under `- name:`):
```yaml
        uses: taiki-e/install-action@9bcaee1dcae34154180f412e2fa69355a7cda9f6 # v2.82.6
        with:
          tool: cross
```

**Step (verify Task 2):** every `dtolnay/rust-toolchain` and `taiki-e/install-action` occurrence is followed by a matching `with:` with `toolchain:` or `tool:`. See Task 4 grep.

---

## Task 3: Add Dependabot for github-actions

**Files:** Create `.github/dependabot.yml` (none exists today).

```yaml
version: 2
updates:
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      github-actions:
        patterns:
          - "*"
    commit-message:
      prefix: "ci"
```

Dependabot keeps SHA pins and updates the `# version` comment on bump; `groups` batches all action bumps into one PR.

---

## Task 4: Verification

**Step 1:** Install actionlint (not in PATH).
```bash
go install github.com/rhysd/actionlint/cmd/actionlint@latest
```
Run on all workflows:
```bash
~/go/bin/actionlint .github/workflows/*.yml
```
Expected: no errors. (If actionlint flags pre-existing unrelated issues, note them but do not fix in this PR.)

**Step 2:** Confirm NO floating refs remain (every `uses:` to a pinned action has a 40-hex SHA). Allow first-party/third-party reusable refs only if intentionally external.
```bash
grep -rnE "uses: [^@]+@(v[0-9]|stable|main|master|[0-9]+\.[0-9])" .github/workflows/ || echo "OK: no floating refs"
```
Expected: `OK: no floating refs`.

**Step 3 (the necessary-but-not-sufficient check):** every selector action has its `with:` selector. For each `dtolnay/rust-toolchain` line confirm a `toolchain:` appears within the next few lines; for each `taiki-e/install-action` confirm a `tool:`.
```bash
python3 - <<'PY'
import re,glob,sys
bad=[]
for f in glob.glob('.github/workflows/*.yml'):
    lines=open(f).read().splitlines()
    for i,l in enumerate(lines):
        if re.search(r'uses:\s*dtolnay/rust-toolchain@',l):
            if not any('toolchain:' in x for x in lines[i+1:i+5]): bad.append(f'{f}:{i+1} dtolnay missing toolchain:')
        if re.search(r'uses:\s*taiki-e/install-action@',l):
            if not any('tool:' in x for x in lines[i+1:i+5]): bad.append(f'{f}:{i+1} taiki-e missing tool:')
print('\n'.join(bad) if bad else 'OK: all selectors present')
sys.exit(1 if bad else 0)
PY
```
Expected: `OK: all selectors present`.

**Step 4:** Sanity — all workflows still parse as YAML.
```bash
for f in .github/workflows/*.yml; do python3 -c "import yaml;yaml.safe_load(open('$f'))" && echo "OK $f"; done
```

---

## Task 5: Commit

```bash
git add .github/workflows/ .github/dependabot.yml docs/plans/2026-07-01-pin-actions-sha.md
git commit -m "$(cat <<'EOF'
ci(security): pin all GitHub Actions to immutable SHAs repo-wide + add Dependabot

Pin third-party AND first-party actions/* to commit SHAs with # version
comments across the 6 remaining workflows (release-plz.yml was already
pinned). ref-as-selector actions (dtolnay/rust-toolchain, taiki-e/install-action)
get explicit with: toolchain:/tool: since the ref no longer carries it.
Add .github/dependabot.yml (github-actions, grouped) to keep pins current.

Closes fulgur-chart-kwn
EOF
)"
```

---

## Acceptance Criteria (from beads fulgur-chart-kwn)

1. All `uses:` in the 6 workflows are pinned to a full SHA + `# version` comment (third-party + `actions/*`); `release-plz.yml` already compliant.
2. Every ref-as-selector action (`dtolnay/rust-toolchain`, `taiki-e/install-action`) has its `with:` selector (`toolchain:` / `tool:`).
3. `.github/dependabot.yml` includes the github-actions ecosystem.
4. `actionlint` passes on all workflows.
