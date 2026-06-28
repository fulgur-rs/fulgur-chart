# @fulgur-rs/chart-node npm 配布 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `@fulgur-rs/chart-node` をマルチプラットフォーム prebuild + npm publish できるようにする。タグ駆動ワークフロー `node-npm-release.yml` を追加し、6プラットフォームの `.node` バイナリを `optionalDependencies` として配布する。

**Architecture:** napi-rs v3 の prebuild モデル。各プラットフォームで `napi build --platform --release` → `.node` アーティファクト収集 → `napi artifacts` で npm サブパッケージに配置 → `npm publish --provenance` でプラットフォームパッケージ＋メインパッケージを順番に公開。トリガーは `fulgur-chart-v*` GitHub Release（Ruby gem と同じ lockstep モデル）。バージョン検証は chart-cli-npm-release.yml の強い実装を踏襲（semver + branch-shadow refusal + SHA pin）。OIDC Trusted Publishing を使い npm トークン不使用。

**Tech Stack:** @napi-rs/cli v3, GitHub Actions, npm Trusted Publishing (OIDC + provenance), napi-rs v3

---

## 対象プラットフォーム

| Rust target | npm パッケージ suffix | OS / arch |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `linux-x64-gnu` | Linux x64 (glibc) |
| `x86_64-unknown-linux-musl` | `linux-x64-musl` | Linux x64 (musl) |
| `aarch64-unknown-linux-gnu` | `linux-arm64-gnu` | Linux arm64 (glibc) |
| `aarch64-apple-darwin` | `darwin-arm64` | macOS arm64 |
| `x86_64-apple-darwin` | `darwin-x64` | macOS x64 |
| `x86_64-pc-windows-msvc` | `win32-x64-msvc` | Windows x64 |

---

### Task 1: package.json に全ターゲットを追加

**Files:**
- Modify: `crates/bindings/node/package.json`

**Step 1: 現状確認**

```bash
cat crates/bindings/node/package.json
```

`napi.targets` が `["x86_64-unknown-linux-gnu"]` のみであることを確認。

**Step 2: package.json を更新**

`napi.targets` に 6 プラットフォームを追加し、`optionalDependencies` を追加する。

```json
{
  "name": "@fulgur-rs/chart-node",
  "version": "0.7.0",
  "description": "Deterministic chart.js v4 / Vega-Lite JSON to SVG/PNG renderer (Node.js native binding)",
  "main": "index.js",
  "types": "index.d.ts",
  "license": "MIT OR Apache-2.0",
  "engines": {
    "node": ">=18.17"
  },
  "files": [
    "index.js",
    "index.d.ts",
    "binding.js",
    "binding.d.ts"
  ],
  "napi": {
    "binaryName": "chart-node",
    "targets": [
      "x86_64-unknown-linux-gnu",
      "x86_64-unknown-linux-musl",
      "aarch64-unknown-linux-gnu",
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "x86_64-pc-windows-msvc"
    ]
  },
  "scripts": {
    "build": "napi build --platform --release --js binding.js --dts binding.d.ts",
    "build:debug": "napi build --platform --js binding.js --dts binding.d.ts",
    "test": "node --test",
    "typecheck": "tsc --noEmit -p tsconfig.json",
    "prepack": "npm run build"
  },
  "dependencies": {
    "@types/node": "^20.0.0"
  },
  "devDependencies": {
    "@napi-rs/cli": "^3.0.0",
    "typescript": "^5.0.0"
  },
  "optionalDependencies": {
    "@fulgur-rs/chart-node-linux-x64-gnu": "0.7.0",
    "@fulgur-rs/chart-node-linux-x64-musl": "0.7.0",
    "@fulgur-rs/chart-node-linux-arm64-gnu": "0.7.0",
    "@fulgur-rs/chart-node-darwin-arm64": "0.7.0",
    "@fulgur-rs/chart-node-darwin-x64": "0.7.0",
    "@fulgur-rs/chart-node-win32-x64-msvc": "0.7.0"
  }
}
```

**Step 3: dry-run で npm sub-package dirs の shape を確認**

```bash
cd crates/bindings/node
npm install
npx napi create-npm-dirs --dry-run
```

Expected: 6 プラットフォーム分の `INFO ... created` ログが出る。

**Step 4: Commit**

```bash
git add crates/bindings/node/package.json
git commit -m "chore(node): add multi-platform napi targets and optionalDependencies"
```

---

### Task 2: npm/ サブパッケージ stub を生成してコミット

**Files:**
- Create: `crates/bindings/node/npm/{platform}/package.json` (6 個)
- Create: `crates/bindings/node/npm/{platform}/.gitignore` (6 個)

**Step 1: npm サブパッケージ dirs を生成**

```bash
cd crates/bindings/node
npx napi create-npm-dirs
```

Expected: `npm/linux-x64-gnu/`, `npm/linux-x64-musl/`, `npm/linux-arm64-gnu/`, `npm/darwin-arm64/`, `npm/darwin-x64/`, `npm/win32-x64-msvc/` が作成される。各ディレクトリに `package.json` が生成される。

**Step 2: 生成物を確認**

```bash
find npm/ -name "package.json" | xargs -I{} sh -c 'echo "=== {} ==="; cat {}'
```

各 `package.json` に `"name": "@fulgur-rs/chart-node-{platform}"` と `"main": "chart-node.{platform}.node"` が含まれることを確認。

**Step 3: *.node を gitignore する**

各プラットフォームディレクトリに `.gitignore` を追加:

```bash
for dir in npm/linux-x64-gnu npm/linux-x64-musl npm/linux-arm64-gnu npm/darwin-arm64 npm/darwin-x64 npm/win32-x64-msvc; do
  echo "*.node" > "crates/bindings/node/$dir/.gitignore"
done
```

**Step 4: npm/ を gitignore から除外されているか確認**

```bash
git check-ignore -v crates/bindings/node/npm/ || echo "not ignored (good)"
```

Expected: `not ignored (good)` — npm/ は追跡対象。

**Step 5: Commit**

```bash
git add crates/bindings/node/npm/
git commit -m "chore(node): add napi npm sub-package stubs for 6 platforms"
```

---

### Task 3: ローカルビルドでターゲット拡張後の動作を確認

**Files:**
- (変更なし — 検証のみ)

**Step 1: ホストプラットフォームでビルド**

```bash
cd crates/bindings/node
npm run build
```

Expected: `binding.js` と `binding.d.ts` が生成される。

**Step 2: 生成された binding.js が全プラットフォームを解決するか確認**

```bash
grep -c "chart-node\." binding.js
```

Expected: 6 行以上（各プラットフォームの `.node` ファイル名が含まれる）。

```bash
grep "chart-node\." binding.js
```

Expected: `chart-node.linux-x64-gnu.node`, `chart-node.darwin-arm64.node` 等 6 プラットフォーム分がリストされる。

**Step 3: テスト実行（ホストプラットフォームでスモーク）**

```bash
npm test
```

Expected: all tests pass。

**Step 4: npm pack dry-run で配布物確認**

```bash
# binding.js が files に含まれるかチェック
npm pack --dry-run
```

Expected: `binding.js`, `binding.d.ts`, `index.js`, `index.d.ts` が含まれる。`.node` ファイルは含まれない（メインパッケージには含めない）。

---

### Task 4: node-npm-release.yml ワークフロー作成

**Files:**
- Create: `.github/workflows/node-npm-release.yml`

**Step 1: ワークフローファイルを作成**

chart-cli-npm-release.yml の強い `validate-release-tag` を踏襲し、`fulgur-chart-v*` タグをトリガーにする。

```yaml
# @fulgur-rs/chart-node の マルチプラットフォーム prebuild と npm への自動配布。
# release-plz が fulgur-chart-v* タグを含む GitHub Release を publish したときに起動する。
# OIDC Trusted Publishing を使用するため npm トークン不要。
#
# 手動セットアップ (一度だけ):
#   npm の @fulgur-rs/chart-node* パッケージ群に Trusted Publisher を登録する。
#   Owner: fulgur-rs, Repo: fulgur-chart, Workflow: node-npm-release.yml, Environment: npm
#
# 配布するパッケージ (7つ):
#   @fulgur-rs/chart-node-linux-x64-gnu
#   @fulgur-rs/chart-node-linux-x64-musl
#   @fulgur-rs/chart-node-linux-arm64-gnu
#   @fulgur-rs/chart-node-darwin-arm64
#   @fulgur-rs/chart-node-darwin-x64
#   @fulgur-rs/chart-node-win32-x64-msvc
#   @fulgur-rs/chart-node (loader + JS wrapper)

name: Node.js NPM Release

on:
  release:
    types: [published]

jobs:
  validate-release-tag:
    name: Validate release tag
    if: startsWith(github.ref_name, 'fulgur-chart-v')
    runs-on: ubuntu-latest
    permissions:
      contents: read
    outputs:
      tag: ${{ steps.validate.outputs.tag }}
      sha: ${{ steps.validate.outputs.sha }}
      version: ${{ steps.validate.outputs.version }}
    steps:
      - name: Validate immutable release tag
        id: validate
        env:
          REQUESTED_TAG: ${{ github.ref_name }}
          REF_TYPE: ${{ github.ref_type }}
          REF_NAME: ${{ github.ref_name }}
          REPO_URL: ${{ github.server_url }}/${{ github.repository }}.git
        run: |
          set -euo pipefail

          tag="$REQUESTED_TAG"

          # 1. Strict SemVer under the fulgur-chart-v prefix.
          semver_re='^fulgur-chart-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(\.(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$'
          if [[ ! "$tag" =~ $semver_re ]]; then
            echo "Invalid release tag (expected fulgur-chart-v<strict-semver>): '$tag'" >&2
            exit 1
          fi

          # 2. Workflow must run from the tag itself.
          if [[ "$REF_TYPE" != "tag" || "$REF_NAME" != "$tag" ]]; then
            echo "Workflow must run from the release tag (ref_type=$REF_TYPE ref_name=$REF_NAME tag=$tag)." >&2
            exit 1
          fi

          # 3. Refuse if a branch with the same name exists (branch-shadow attack).
          branch_match="$(git ls-remote --heads "$REPO_URL" "refs/heads/$tag" \
            | awk -v ref="refs/heads/$tag" '$2 == ref { print $1 }')"
          if [[ -n "$branch_match" ]]; then
            echo "Refusing ambiguous release ref: branch '$tag' exists." >&2
            exit 1
          fi

          # 4. Resolve to immutable commit SHA.
          tag_refs="$(git ls-remote "$REPO_URL" "refs/tags/$tag" "refs/tags/$tag^{}")"
          sha="$(awk '$2 ~ /\^\{\}$/ { print $1 }' <<< "$tag_refs")"
          if [[ -z "$sha" ]]; then
            sha="$(awk -v ref="refs/tags/$tag" '$2 == ref { print $1 }' <<< "$tag_refs")"
          fi
          if [[ -z "$sha" ]]; then
            echo "Tag '$tag' does not exist on origin." >&2
            exit 1
          fi

          echo "Resolved $tag -> $sha"
          {
            echo "tag=$tag"
            echo "sha=$sha"
            echo "version=${tag#fulgur-chart-v}"
          } >> "$GITHUB_OUTPUT"

  build-native:
    name: Build ${{ matrix.settings.target }}
    needs: validate-release-tag
    runs-on: ${{ matrix.settings.os }}
    permissions:
      contents: read
    strategy:
      fail-fast: false
      matrix:
        settings:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
            use-cross: true
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
            use-cross: true
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    defaults:
      run:
        working-directory: crates/bindings/node
    steps:
      - uses: actions/checkout@v5
        with:
          ref: ${{ needs.validate-release-tag.outputs.sha }}
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.settings.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: crates/bindings/node
          key: ${{ matrix.settings.target }}

      - uses: actions/setup-node@v4
        with:
          node-version: "20"

      - name: Install npm deps
        run: npm install

      - name: Install cross (musl / arm64)
        if: matrix.settings.use-cross
        uses: taiki-e/install-action@cross

      - name: Build native addon (cross)
        if: matrix.settings.use-cross
        run: npx napi build --platform --release --strip --target ${{ matrix.settings.target }} --use-cross

      - name: Build native addon (native)
        if: '!matrix.settings.use-cross'
        run: npx napi build --platform --release --strip --target ${{ matrix.settings.target }}

      - uses: actions/upload-artifact@v4
        with:
          name: bindings-${{ matrix.settings.target }}
          path: crates/bindings/node/*.node
          if-no-files-found: error

  publish-packages:
    name: Publish npm packages
    needs: [validate-release-tag, build-native]
    runs-on: ubuntu-latest
    environment:
      name: npm
      url: https://www.npmjs.com/package/@fulgur-rs/chart-node
    permissions:
      contents: read
      id-token: write
    defaults:
      run:
        working-directory: crates/bindings/node
    steps:
      - uses: actions/checkout@v5
        with:
          ref: ${{ needs.validate-release-tag.outputs.sha }}
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: crates/bindings/node
          key: publish

      - uses: actions/setup-node@v4
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

      - name: Download all .node artifacts
        uses: actions/download-artifact@v4
        with:
          pattern: bindings-*
          path: crates/bindings/node/artifacts/
          merge-multiple: true

      - name: Move artifacts into npm sub-packages
        run: npx napi artifacts --output-dir artifacts/ --npm-dir npm/

      - name: Build binding.js loader (regenerates loader covering all 6 platforms)
        run: npx napi build --platform --release --js binding.js --dts binding.d.ts

      - name: Verify binding.js covers all 6 platforms
        run: |
          count=$(grep -c "chart-node\." binding.js)
          echo "binding.js references $count platform entries"
          if [ "$count" -lt 6 ]; then
            echo "ERROR: expected >= 6 platform entries in binding.js, got $count" >&2
            exit 1
          fi

      - name: Set package version from tag
        env:
          VERSION: ${{ needs.validate-release-tag.outputs.version }}
        run: |
          node -e '
            const fs = require("fs");
            const v = process.env.VERSION;
            // Update main package.json
            const main = JSON.parse(fs.readFileSync("package.json", "utf8"));
            main.version = v;
            for (const dep of Object.keys(main.optionalDependencies || {})) {
              main.optionalDependencies[dep] = v;
            }
            fs.writeFileSync("package.json", JSON.stringify(main, null, 2) + "\n");
            // Update each npm/ sub-package
            const platforms = ["linux-x64-gnu","linux-x64-musl","linux-arm64-gnu","darwin-arm64","darwin-x64","win32-x64-msvc"];
            for (const p of platforms) {
              const path = `npm/${p}/package.json`;
              const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
              pkg.version = v;
              fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
            }
          '

      - name: Publish platform packages
        run: |
          for platform in linux-x64-gnu linux-x64-musl linux-arm64-gnu darwin-arm64 darwin-x64 win32-x64-msvc; do
            npm publish --provenance --access public npm/$platform/
          done

      - name: Publish main package (@fulgur-rs/chart-node)
        run: npm publish --provenance --access public
```

**Step 2: YAML 構文確認**

```bash
# actionlint が利用可能な場合
which actionlint && actionlint .github/workflows/node-npm-release.yml || echo "actionlint not installed; skip"
# 最低限の YAML 構文チェック
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/node-npm-release.yml'))" && echo "YAML valid"
```

Expected: エラーなし。

**Step 3: Commit**

```bash
git add .github/workflows/node-npm-release.yml
git commit -m "ci: add node-npm-release.yml for @fulgur-rs/chart-node multi-platform publish"
```

---

### Task 5: ドキュメント更新 (README)

**Files:**
- Modify: `crates/bindings/node/README.md`

**Step 1: README の現状確認**

```bash
cat crates/bindings/node/README.md
```

**Step 2: npm publish セットアップ手順を追記**

README に Trusted Publishing の一度だけセットアップ手順を追加する。

> ### npm パッケージの配布
>
> リリースは `fulgur-chart-v*` GitHub Release が publish されると自動的に起動します。
> 初回セットアップ: npm で `@fulgur-rs/chart-node` および各プラットフォームパッケージ
> (`@fulgur-rs/chart-node-*`) に Trusted Publisher を登録してください。
> - Owner: `fulgur-rs`, Repo: `fulgur-chart`, Workflow: `node-npm-release.yml`, Environment: `npm`

**Step 3: Commit**

```bash
git add crates/bindings/node/README.md
git commit -m "docs(node): add npm release setup instructions"
```

---

### Task 6: ローカル検証まとめ

**ローカルで確認できる項目 (今すぐ実行):**

```bash
cd crates/bindings/node

# 1. npm sub-packages の形状確認
find npm/ -name "package.json" | sort | xargs -I{} sh -c 'echo "=== {} ==="; cat {}'

# 2. ビルド確認
npm run build

# 3. テスト
npm test

# 4. binding.js の全プラットフォームカバレッジ
grep "chart-node\." binding.js | grep -c ".node"

# 5. npm pack dry-run
npm pack --dry-run
```

**リリース時のみ確認できる項目:**
- マトリクスビルドの全プラットフォーム成功
- `napi artifacts` による .node ファイル配置
- `npm publish --provenance` の OIDC 認証成功

---

## Trusted Publishing セットアップ手順 (一度だけ・人間作業)

以下 7 パッケージそれぞれに npm Trusted Publisher を登録する:
- `@fulgur-rs/chart-node`
- `@fulgur-rs/chart-node-linux-x64-gnu`
- `@fulgur-rs/chart-node-linux-x64-musl`
- `@fulgur-rs/chart-node-linux-arm64-gnu`
- `@fulgur-rs/chart-node-darwin-arm64`
- `@fulgur-rs/chart-node-darwin-x64`
- `@fulgur-rs/chart-node-win32-x64-msvc`

設定値: Owner=`fulgur-rs`, Repo=`fulgur-chart`, Workflow=`node-npm-release.yml`, Environment=`npm`
