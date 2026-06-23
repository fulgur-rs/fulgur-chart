# `@fulgur-rs/chart-cli` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an npm meta-package `@fulgur-rs/chart-cli` (plus platform-specific optionalDependencies) that lets users run the existing `fulgur-chart` Rust CLI via `npx` without building from source.

**Architecture:** Follow the existing `@fulgur-rs/cli` (PDF) distribution pattern: a tiny JS launcher in the meta-package resolves the correct platform package from `optionalDependencies`, then `spawnSync`s the bundled binary. A GitHub Actions release workflow cross-compiles the Rust CLI and publishes all packages to npm with provenance.

**Tech Stack:** Node.js 20 (no runtime deps), Rust/cargo (cross-compilation), GitHub Actions, npm provenance/OIDC.

---

## File Structure

```text
packages/npm/
├── chart-cli/                          # meta package
│   ├── package.json
│   ├── README.md
│   ├── bin/fulgur-chart                # JS launcher (executable)
│   └── __test__/launcher.test.js
├── chart-cli-linux-x64/package.json
├── chart-cli-linux-x64-musl/package.json
├── chart-cli-linux-arm64/package.json
├── chart-cli-darwin-arm64/package.json
├── chart-cli-darwin-x64/package.json
└── chart-cli-win32-x64/package.json

.github/workflows/
├── ci.yml                              # add chart-cli job
└── chart-cli-npm-release.yml           # new release workflow
```

---

## Task 1: Create platform package manifests

**Files:**
- Create: `packages/npm/chart-cli-linux-x64/package.json`
- Create: `packages/npm/chart-cli-linux-x64-musl/package.json`
- Create: `packages/npm/chart-cli-linux-arm64/package.json`
- Create: `packages/npm/chart-cli-darwin-arm64/package.json`
- Create: `packages/npm/chart-cli-darwin-x64/package.json`
- Create: `packages/npm/chart-cli-win32-x64/package.json`

- [ ] **Step 1: Create `packages/npm/chart-cli-linux-x64/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli-linux-x64",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for Linux x64 (glibc)",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli-linux-x64"
  },
  "os": ["linux"],
  "cpu": ["x64"],
  "files": ["bin/"]
}
```

- [ ] **Step 2: Create `packages/npm/chart-cli-linux-x64-musl/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli-linux-x64-musl",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for Linux x64 (musl)",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli-linux-x64-musl"
  },
  "os": ["linux"],
  "cpu": ["x64"],
  "files": ["bin/"]
}
```

- [ ] **Step 3: Create `packages/npm/chart-cli-linux-arm64/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli-linux-arm64",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for Linux arm64",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli-linux-arm64"
  },
  "os": ["linux"],
  "cpu": ["arm64"],
  "files": ["bin/"]
}
```

- [ ] **Step 4: Create `packages/npm/chart-cli-darwin-arm64/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli-darwin-arm64",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for macOS arm64",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli-darwin-arm64"
  },
  "os": ["darwin"],
  "cpu": ["arm64"],
  "files": ["bin/"]
}
```

- [ ] **Step 5: Create `packages/npm/chart-cli-darwin-x64/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli-darwin-x64",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for macOS x64",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli-darwin-x64"
  },
  "os": ["darwin"],
  "cpu": ["x64"],
  "files": ["bin/"]
}
```

- [ ] **Step 6: Create `packages/npm/chart-cli-win32-x64/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli-win32-x64",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for Windows x64",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli-win32-x64"
  },
  "os": ["win32"],
  "cpu": ["x64"],
  "files": ["bin/"]
}
```

---

## Task 2: Create meta package and launcher

**Files:**
- Create: `packages/npm/chart-cli/package.json`
- Create: `packages/npm/chart-cli/bin/fulgur-chart`
- Create: `packages/npm/chart-cli/README.md`

- [ ] **Step 1: Create `packages/npm/chart-cli/package.json`**

```json
{
  "name": "@fulgur-rs/chart-cli",
  "version": "0.1.9",
  "description": "Zero-install npx distribution of the fulgur-chart Rust CLI",
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/fulgur-rs/fulgur-chart.git",
    "directory": "packages/npm/chart-cli"
  },
  "bin": {
    "fulgur-chart": "bin/fulgur-chart"
  },
  "files": [
    "bin/"
  ],
  "optionalDependencies": {
    "@fulgur-rs/chart-cli-linux-x64": "0.1.9",
    "@fulgur-rs/chart-cli-linux-x64-musl": "0.1.9",
    "@fulgur-rs/chart-cli-linux-arm64": "0.1.9",
    "@fulgur-rs/chart-cli-darwin-arm64": "0.1.9",
    "@fulgur-rs/chart-cli-darwin-x64": "0.1.9",
    "@fulgur-rs/chart-cli-win32-x64": "0.1.9"
  }
}
```

- [ ] **Step 2: Create `packages/npm/chart-cli/bin/fulgur-chart`**

```js
#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const PLATFORMS = {
  'linux-x64':      { pkg: '@fulgur-rs/chart-cli-linux-x64',      bin: 'fulgur-chart' },
  'linux-x64-musl': { pkg: '@fulgur-rs/chart-cli-linux-x64-musl', bin: 'fulgur-chart' },
  'linux-arm64':    { pkg: '@fulgur-rs/chart-cli-linux-arm64',    bin: 'fulgur-chart' },
  'darwin-arm64':   { pkg: '@fulgur-rs/chart-cli-darwin-arm64',   bin: 'fulgur-chart' },
  'darwin-x64':     { pkg: '@fulgur-rs/chart-cli-darwin-x64',     bin: 'fulgur-chart' },
  'win32-x64':      { pkg: '@fulgur-rs/chart-cli-win32-x64',      bin: 'fulgur-chart.exe' },
};

function isMusl() {
  try { return fs.readFileSync('/proc/self/maps', 'utf8').includes('musl'); }
  catch { return false; }
}

function detectPlatformKey(platform, arch, isMuslFn = isMusl) {
  if (platform === 'linux' && arch === 'x64') return isMuslFn() ? 'linux-x64-musl' : 'linux-x64';
  if (platform === 'linux' && arch === 'arm64') return 'linux-arm64';
  if (platform === 'darwin' && arch === 'arm64') return 'darwin-arm64';
  if (platform === 'darwin' && arch === 'x64') return 'darwin-x64';
  if (platform === 'win32' && arch === 'x64') return 'win32-x64';
  return null;
}

function main() {
  const key = detectPlatformKey(process.platform, process.arch);
  if (!key) {
    process.stderr.write(`@fulgur-rs/chart-cli: unsupported platform ${process.platform}/${process.arch}\n`);
    process.exit(1);
  }

  const { pkg, bin } = PLATFORMS[key];
  let pkgDir;
  try {
    pkgDir = path.dirname(require.resolve(`${pkg}/package.json`));
  } catch {
    process.stderr.write(
      `@fulgur-rs/chart-cli: platform package ${pkg} not found.\n` +
      `This usually means it was not installed (e.g. --ignore-optional was used).\n`
    );
    process.exit(1);
  }

  const r = spawnSync(path.join(pkgDir, 'bin', bin), process.argv.slice(2), { stdio: 'inherit' });
  process.exit(r.status ?? 1);
}

module.exports = { detectPlatformKey, PLATFORMS };
if (require.main === module) main();
```

- [ ] **Step 3: Make the launcher executable**

Run:

```bash
chmod +x packages/npm/chart-cli/bin/fulgur-chart
```

- [ ] **Step 4: Create `packages/npm/chart-cli/README.md`**

```markdown
# @fulgur-rs/chart-cli

Zero-install `npx` distribution of the `fulgur-chart` Rust CLI.

## Usage

```bash
npx @fulgur-rs/chart-cli render spec.json -o out.svg
npx @fulgur-rs/chart-cli render spec.json -o out.png --format png
npx @fulgur-rs/chart-cli schema
```

## Supported platforms

- Linux x64 (glibc)
- Linux x64 (musl)
- Linux arm64
- macOS arm64
- macOS x64
- Windows x64

Unsupported platforms exit with code 1 and a clear error message.
```

---

## Task 3: Test the launcher

**Files:**
- Create: `packages/npm/chart-cli/__test__/launcher.test.js`

- [ ] **Step 1: Write the launcher tests**

```js
'use strict';
const { describe, it } = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('child_process');
const path = require('path');
const { detectPlatformKey } = require('../bin/fulgur-chart');

const LAUNCHER = path.join(__dirname, '..', 'bin', 'fulgur-chart');

describe('detectPlatformKey', () => {
  it('detects linux x64 glibc', () => {
    assert.strictEqual(detectPlatformKey('linux', 'x64', () => false), 'linux-x64');
  });

  it('detects linux x64 musl', () => {
    assert.strictEqual(detectPlatformKey('linux', 'x64', () => true), 'linux-x64-musl');
  });

  it('detects linux arm64', () => {
    assert.strictEqual(detectPlatformKey('linux', 'arm64', () => false), 'linux-arm64');
  });

  it('detects darwin arm64', () => {
    assert.strictEqual(detectPlatformKey('darwin', 'arm64', () => false), 'darwin-arm64');
  });

  it('detects darwin x64', () => {
    assert.strictEqual(detectPlatformKey('darwin', 'x64', () => false), 'darwin-x64');
  });

  it('detects win32 x64', () => {
    assert.strictEqual(detectPlatformKey('win32', 'x64', () => false), 'win32-x64');
  });

  it('returns null for unsupported platform', () => {
    assert.strictEqual(detectPlatformKey('freebsd', 'x64', () => false), null);
  });
});

describe('launcher errors', () => {
  it('exits 1 when platform is unsupported or package is missing', () => {
    // Run from /tmp so the launcher cannot resolve any optional platform package.
    const r = spawnSync(process.execPath, [LAUNCHER, 'render', '-', '-o', '-'], {
      cwd: '/tmp',
      input: '{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}',
      encoding: 'utf8',
    });
    assert.notStrictEqual(r.status, 0, `expected non-zero exit, got ${r.status}`);
    assert.ok(
      /unsupported platform/.test(r.stderr) || /platform package @fulgur-rs\/chart-cli-/.test(r.stderr),
      `expected clear error message, got: ${r.stderr}`
    );
  });
});
```

- [ ] **Step 2: Run the unit tests**

Run:

```bash
cd packages/npm/chart-cli && node --test
```

Expected: all `detectPlatformKey` tests pass.

---

## Task 4: Add CI integration

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Append a `chart-cli` job to `.github/workflows/ci.yml`**

Insert after the `node-binding` job (around line 283) and before the `perf` job:

```yaml
  chart-cli:
    name: Chart CLI launcher (build + smoke)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          key: chart-cli

      - uses: actions/setup-node@v4
        with:
          node-version: "20"

      - name: Build fulgur-chart binary (linux-x64)
        run: cargo build --release -p fulgur-chart-cli

      - name: Stage binary for launcher
        run: |
          mkdir -p packages/npm/chart-cli-linux-x64/bin
          cp target/release/fulgur-chart packages/npm/chart-cli-linux-x64/bin/fulgur-chart

      - name: Link platform package into meta package
        run: |
          mkdir -p packages/npm/chart-cli/node_modules/@fulgur-rs
          ln -s ../../../chart-cli-linux-x64 packages/npm/chart-cli/node_modules/@fulgur-rs/chart-cli-linux-x64

      - name: Run launcher unit tests
        working-directory: packages/npm/chart-cli
        run: node --test

      - name: Smoke test npx execution
        run: |
          npx ./packages/npm/chart-cli render examples/specs/bar.json -o /tmp/chart-cli-smoke.svg
          test "$(head -c 4 /tmp/chart-cli-smoke.svg)" = "<svg"

      - name: Verify output matches native CLI
        run: |
          cargo run --release -p fulgur-chart-cli -- render examples/specs/bar.json -o /tmp/chart-cli-native.svg
          diff /tmp/chart-cli-smoke.svg /tmp/chart-cli-native.svg
```

---

## Task 5: Create the release workflow

**Files:**
- Create: `.github/workflows/chart-cli-npm-release.yml`

- [ ] **Step 1: Create `.github/workflows/chart-cli-npm-release.yml`**

```yaml
name: Chart CLI NPM Release

on:
  release:
    types: [published]
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag (e.g. fulgur-chart-v0.1.9)"
        required: true

env:
  RELEASE_TAG: ${{ github.event.inputs.tag || github.ref_name }}

jobs:
  build-binary:
    name: Build ${{ matrix.platform }}
    if: startsWith(github.event.inputs.tag || github.ref_name, 'fulgur-chart-v')
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: linux-x64
            target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - platform: linux-x64-musl
            target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - platform: linux-arm64
            target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          - platform: darwin-arm64
            target: aarch64-apple-darwin
            os: macos-latest
          - platform: darwin-x64
            target: x86_64-apple-darwin
            os: macos-latest
          - platform: win32-x64
            target: x86_64-pc-windows-msvc
            os: windows-latest
    steps:
      - uses: actions/checkout@v5

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Install musl tools
        if: matrix.platform == 'linux-x64-musl'
        run: sudo apt-get install -y musl-tools

      - name: Install cross
        if: matrix.platform == 'linux-arm64'
        uses: taiki-e/install-action@cross

      - name: Build binary (native)
        if: matrix.platform != 'linux-arm64'
        run: cargo build --release -p fulgur-chart-cli --target ${{ matrix.target }}

      - name: Build binary (cross)
        if: matrix.platform == 'linux-arm64'
        run: cross build --release -p fulgur-chart-cli --target ${{ matrix.target }}

      - name: Upload binary artifact
        uses: actions/upload-artifact@v4
        with:
          name: chart-cli-${{ matrix.platform }}
          path: target/${{ matrix.target }}/release/fulgur-chart*

  publish-packages:
    name: Publish npm packages
    needs: build-binary
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v5

      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          registry-url: "https://registry.npmjs.org"

      - name: Compute version from tag
        run: echo "VERSION=${RELEASE_TAG#fulgur-chart-v}" >> "$GITHUB_ENV"

      - name: Download all binary artifacts
        uses: actions/download-artifact@v4
        with:
          pattern: chart-cli-*
          path: artifacts/

      - name: Stage platform packages
        run: |
          set -e
          VERSION="$VERSION"
          declare -A BINARY_NAMES=(
            [linux-x64]=fulgur-chart
            [linux-x64-musl]=fulgur-chart
            [linux-arm64]=fulgur-chart
            [darwin-arm64]=fulgur-chart
            [darwin-x64]=fulgur-chart
            [win32-x64]=fulgur-chart.exe
          )
          for platform in linux-x64 linux-x64-musl linux-arm64 darwin-arm64 darwin-x64 win32-x64; do
            pkg_dir="packages/npm/chart-cli-${platform}"
            bin_name="${BINARY_NAMES[$platform]}"
            artifact="artifacts/chart-cli-${platform}/target/*/release/${bin_name}"
            mkdir -p "${pkg_dir}/bin"
            cp ${artifact} "${pkg_dir}/bin/${bin_name}"
            chmod +x "${pkg_dir}/bin/${bin_name}" || true
            node -e "
              const fs = require('fs');
              const pkg = JSON.parse(fs.readFileSync('${pkg_dir}/package.json', 'utf8'));
              pkg.version = '${VERSION}';
              fs.writeFileSync('${pkg_dir}/package.json', JSON.stringify(pkg, null, 2) + '\n');
            "
          done

      - name: Publish platform packages
        run: |
          set -e
          for platform in linux-x64 linux-x64-musl linux-arm64 darwin-arm64 darwin-x64 win32-x64; do
            npm publish --provenance --access public packages/npm/chart-cli-${platform}
          done
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}

      - name: Update meta package optionalDependencies
        working-directory: packages/npm/chart-cli
        run: |
          node -e "
            const fs = require('fs');
            const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
            pkg.version = '${VERSION}';
            for (const dep of Object.keys(pkg.optionalDependencies)) {
              pkg.optionalDependencies[dep] = '${VERSION}';
            }
            fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
          "

      - name: Publish meta package
        working-directory: packages/npm/chart-cli
        run: npm publish --provenance --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

- [ ] **Step 2: Add a comment about npm authentication**

Add the following comment at the top of `.github/workflows/chart-cli-npm-release.yml`, right after the `name:` line:

```yaml
# NOTE: This workflow uses npm provenance. To actually publish, either:
#   1. Configure an NPM_TOKEN repository secret and keep NODE_AUTH_TOKEN as-is, OR
#   2. Set up Trusted Publishing (OIDC) for @fulgur-rs/chart-cli* on npm and remove NODE_AUTH_TOKEN.
# Until one of these is done, the publish steps will fail auth.
```

---

## Task 6: Local verification

- [ ] **Step 1: Build the Rust binary**

Run:

```bash
cargo build --release -p fulgur-chart-cli
```

Expected: binary at `target/release/fulgur-chart`.

- [ ] **Step 2: Stage the binary in the linux-x64 platform package**

Run:

```bash
mkdir -p packages/npm/chart-cli-linux-x64/bin
cp target/release/fulgur-chart packages/npm/chart-cli-linux-x64/bin/fulgur-chart
```

- [ ] **Step 3: Link the platform package into the meta package**

Run:

```bash
mkdir -p packages/npm/chart-cli/node_modules/@fulgur-rs
ln -s ../../../chart-cli-linux-x64 packages/npm/chart-cli/node_modules/@fulgur-rs/chart-cli-linux-x64
```

- [ ] **Step 4: Run the launcher unit tests**

Run:

```bash
cd packages/npm/chart-cli && node --test
```

Expected: all tests pass.

- [ ] **Step 5: Smoke test `npx` rendering**

Run:

```bash
npx ./packages/npm/chart-cli render examples/specs/bar.json -o /tmp/chart-cli-smoke.svg
head -c 4 /tmp/chart-cli-smoke.svg
```

Expected output: `<svg`

- [ ] **Step 6: Verify byte match with native CLI**

Run:

```bash
cargo run --release -p fulgur-chart-cli -- render examples/specs/bar.json -o /tmp/chart-cli-native.svg
diff /tmp/chart-cli-smoke.svg /tmp/chart-cli-native.svg
```

Expected: no diff output.

---

## Self-Review Checklist

- [ ] **Spec coverage:** Does every requirement in `docs/plans/2026-06-23-chart-cli-npm-design.md` map to a task?
  - Directory structure → Task 1 + Task 2
  - Meta package → Task 2
  - Platform packages → Task 1
  - Launcher behavior → Task 2 + Task 3
  - Release workflow → Task 5
  - CI integration → Task 4
  - Testing → Task 3 + Task 4 + Task 6
- [ ] **Placeholder scan:** No "TBD", "TODO", "implement later", or vague "handle edge cases" steps remain.
- [ ] **Type consistency:** `detectPlatformKey` signature, `PLATFORMS` shape, and binary names are consistent across launcher, tests, CI, and release workflow.
- [ ] **Authentication note:** Release workflow explicitly documents that npm auth setup (token or OIDC) is required before publish will succeed.
