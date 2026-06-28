# @fulgur-rs/chart-wasm npm 配布 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `node-npm-release.yml` に `publish-wasm` ジョブを追加し、`@fulgur-rs/chart-wasm` を同一ワークフローで publish できるようにする。

**Architecture:** `validate-release-tag` → `build-native`(matrix) と `publish-wasm` が並行。`publish-node` は `build-native` 完了後に実行。`publish-wasm` は `validate-release-tag` のみに依存するため node ビルドを待たない。両 publish ジョブが `environment: npm` を共有し、1回の approve でアンブロック。

**Tech Stack:** wasm-pack, GitHub Actions, npm Trusted Publishing (OIDC + provenance)

---

## Tasks

### Task 1: node-npm-release.yml に publish-wasm ジョブを追加

**Files:**
- Modify: `.github/workflows/node-npm-release.yml`

**Step 1: 現状を確認**
```bash
cat .github/workflows/node-npm-release.yml
```
ファイル末尾（`publish-packages` ジョブの後）に追記するポイントを把握する。

**Step 2: ファイル冒頭のコメントを更新**

1行目のコメントブロックを以下に更新（WASM パッケージの追記）:

```yaml
# @fulgur-rs/chart-node および @fulgur-rs/chart-wasm の npm への自動配布。
# release-plz が fulgur-chart-v* タグを含む GitHub Release を publish したときに起動する。
# OIDC Trusted Publishing を使用するため npm トークン不要。
#
# 手動セットアップ (一度だけ):
#   npm の各パッケージに Trusted Publisher を登録する。
#   Owner: fulgur-rs, Repo: fulgur-chart, Workflow: node-npm-release.yml, Environment: npm
#
# 配布するパッケージ (8つ):
#   @fulgur-rs/chart-node-linux-x64-gnu
#   @fulgur-rs/chart-node-linux-x64-musl
#   @fulgur-rs/chart-node-linux-arm64-gnu
#   @fulgur-rs/chart-node-darwin-arm64
#   @fulgur-rs/chart-node-darwin-x64
#   @fulgur-rs/chart-node-win32-x64-msvc
#   @fulgur-rs/chart-node (loader + JS wrapper)
#   @fulgur-rs/chart-wasm (WebAssembly)
```

**Step 3: `name:` を更新**

```yaml
name: Node.js / WASM NPM Release
```

**Step 4: publish-wasm ジョブを末尾に追加**

`publish-packages` ジョブの後に以下を追加:

```yaml
  publish-wasm:
    name: Build WASM and publish to npm
    needs: validate-release-tag
    runs-on: ubuntu-latest
    environment:
      name: npm
      url: https://www.npmjs.com/package/@fulgur-rs/chart-wasm
    permissions:
      contents: read
      id-token: write
    defaults:
      run:
        working-directory: crates/bindings/wasm
    steps:
      - uses: actions/checkout@v5
        with:
          ref: ${{ needs.validate-release-tag.outputs.sha }}
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: crates/bindings/wasm
          key: wasm-publish

      - name: Install wasm-pack
        uses: taiki-e/install-action@wasm-pack

      # setup-node MUST be >= v6.1.0 for OIDC trusted publishing: older versions
      # write an .npmrc with `always-auth=true` + a placeholder _authToken, which
      # npm uses instead of OIDC (publish 404s). v6.1.0 removed always-auth
      # handling (actions/setup-node#1436). `registry-url` is required so npm
      # engages the OIDC handshake against the registry; do NOT pass a token.
      - uses: actions/setup-node@v6
        with:
          node-version: "24"
          registry-url: "https://registry.npmjs.org"

      - name: Ensure npm supports OIDC trusted publishing (>= 11.5.1)
        run: |
          npm install -g npm@latest
          node --version
          npm --version

      - name: Install npm deps
        run: npm install

      - name: Set package version from tag
        env:
          VERSION: ${{ needs.validate-release-tag.outputs.version }}
        run: |
          node -e '
            const fs = require("fs");
            const v = process.env.VERSION;
            const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
            pkg.version = v;
            fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");
          '

      - name: Build WASM (regenerates pkg/)
        run: npm run build

      - name: Verify pkg/ was generated
        run: |
          test -f pkg/package.json || { echo "ERROR: pkg/package.json not found" >&2; exit 1; }
          test -f pkg/fulgur_chart_wasm_bg.wasm || { echo "ERROR: .wasm not found in pkg/" >&2; exit 1; }
          echo "pkg/ contents:"
          ls -lh pkg/

      - name: Publish @fulgur-rs/chart-wasm
        run: npm publish --provenance --access public
```

**Step 5: YAML 構文確認**
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/node-npm-release.yml'))" && echo "YAML valid"
```

**Step 6: Commit**
```bash
git add .github/workflows/node-npm-release.yml
git commit -m "ci(wasm): add publish-wasm job to node-npm-release.yml"
```

---

### Task 2: WASM README に npm 配布セクションを追加

**Files:**
- Modify: `crates/bindings/wasm/README.md`

**Step 1: 現状確認**
```bash
cat crates/bindings/wasm/README.md
```

**Step 2: npm Package Distribution セクションを末尾に追加**

node/README.md と同スタイルで英語で追記:

```markdown
## npm Package Distribution

Releases are triggered automatically when a `fulgur-chart-v*` GitHub Release is published,
running `node-npm-release.yml` to build and publish `@fulgur-rs/chart-wasm`.

### One-time setup

Register a Trusted Publisher on npm for `@fulgur-rs/chart-wasm`:
- **Owner**: `fulgur-rs`
- **Repo**: `fulgur-chart`
- **Workflow**: `node-npm-release.yml`
- **Environment**: `npm`
```

**Step 3: Commit**
```bash
git add crates/bindings/wasm/README.md
git commit -m "docs(wasm): add npm release setup instructions"
```

---

### Task 3: 整合性確認

**Files:** (変更なし — 確認のみ)

**Step 1: name が更新されているか確認**
```bash
grep "^name:" .github/workflows/node-npm-release.yml
```
Expected: `name: Node.js / WASM NPM Release`

**Step 2: publish-wasm が validate-release-tag のみに依存しているか確認**
```bash
grep -A2 "publish-wasm:" .github/workflows/node-npm-release.yml | head -5
```
Expected: `needs: validate-release-tag`（build-native は不要）

**Step 3: setup-node@v6 が publish-wasm に使われているか確認**
```bash
grep -A50 "publish-wasm:" .github/workflows/node-npm-release.yml | grep "setup-node"
```
Expected: `actions/setup-node@v6`

**Step 4: --provenance が publish コマンドに含まれるか確認**
```bash
grep "npm publish" .github/workflows/node-npm-release.yml
```
Expected: 全行に `--provenance --access public`

---

## Trusted Publishing セットアップ（一度だけ・人間作業）

`@fulgur-rs/chart-wasm` に Trusted Publisher を登録:
- Owner: `fulgur-rs`, Repo: `fulgur-chart`, Workflow: `node-npm-release.yml`, Environment: `npm`
