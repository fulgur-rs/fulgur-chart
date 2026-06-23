# `@fulgur-rs/chart-cli`: npx ゼロインストール配布設計

Issue: `fulgur-chart-wya`

## 背景

既存 Rust CLI `fulgur-chart`（`crates/fulgur-chart-cli`）を、`npx @fulgur-rs/chart-cli ...` でソースビルド不要で実行できるようにする。Node.js native binding（`@fulgur-rs/chart-node`）とは独立した別パッケージとし、Fulgur(PDF) の `@fulgur-rs/cli` と同じ「メタパッケージ + platform 別 optionalDependencies」方式を踏襲する。

## ゴール

- `npx @fulgur-rs/chart-cli render spec.json -o out.svg` が対応 platform でビルド不要で動作する
- 未対応 platform では明確なエラーを出す
- リリースは GitHub Actions で自動化し、npm publish は OIDC / provenance 対応のワークフローに委ねる（registry 側の Trusted Publisher 設定は別途手動）

## ディレクトリ構成

```text
packages/npm/
├── chart-cli/                    # @fulgur-rs/chart-cli (メタパッケージ)
│   ├── package.json
│   ├── README.md
│   └── bin/
│       └── fulgur-chart          # JS launcher
├── chart-cli-linux-x64/          # @fulgur-rs/chart-cli-linux-x64
│   ├── package.json
│   └── bin/
│       └── fulgur-chart
├── chart-cli-linux-x64-musl/
├── chart-cli-linux-arm64/
├── chart-cli-darwin-arm64/
├── chart-cli-darwin-x64/
└── chart-cli-win32-x64/          # bin/fulgur-chart.exe
```

## メタパッケージ

`packages/npm/chart-cli/package.json`:

```json
{
  "name": "@fulgur-rs/chart-cli",
  "version": "0.1.9",
  "description": "Zero-install npx distribution of the fulgur-chart Rust CLI",
  "license": "MIT OR Apache-2.0",
  "bin": { "fulgur-chart": "bin/fulgur-chart" },
  "files": ["bin/"],
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

- 初期バージョンは Rust crate `fulgur-chart-cli` の現在のバージョン `0.1.9` と一致させる。
- 今後のリリースでは tag からバージョンを取得し、すべての package.json を同じバージョンで更新する。

## Platform パッケージ

`packages/npm/chart-cli-linux-x64/package.json`（例）:

```json
{
  "name": "@fulgur-rs/chart-cli-linux-x64",
  "version": "0.1.9",
  "description": "fulgur-chart CLI binary for Linux x64",
  "license": "MIT OR Apache-2.0",
  "os": ["linux"],
  "cpu": ["x64"],
  "files": ["bin/"]
}
```

各 platform パッケージは `bin/fulgur-chart`（Windows のみ `bin/fulgur-chart.exe`）にプリビルドバイナリを同梱する。`os` / `cpu` フィールドで npm がサポート外 platform のパッケージをスキップする。

## Launcher スクリプト

`packages/npm/chart-cli/bin/fulgur-chart` は `@fulgur-rs/cli@0.18.0` の launcher を踏襲する。

### 動作

1. `process.platform` / `process.arch` から platform key を判定。
2. Linux x64 の場合、`/proc/self/maps` を読んで musl かどうか判定し、`linux-x64-musl` / `linux-x64` を切り替える。
3. 対応する optionalDependency の `package.json` を `require.resolve` で解決。
4. 解決できなければ stderr にメッセージを出して exit 1。
5. `bin/fulgur-chart`（Windows は `bin/fulgur-chart.exe`）を `spawnSync(..., process.argv.slice(2), { stdio: 'inherit' })` で実行。
6. 子プロセスの exit code をそのまま伝播。シグナル終了時は exit 1。

### 未対応 platform / 未インストール時のエラー

- 未対応 platform: `@fulgur-rs/chart-cli: unsupported platform ${platform}/${arch}`
- optionalDep 未インストール: `@fulgur-rs/chart-cli: platform package ${pkg} not found. This usually means it was not installed (e.g. --ignore-optional was used).`

## リリースワークフロー

`.github/workflows/chart-cli-npm-release.yml`:

### トリガー

- `release` event: `published`（`fulgur-chart-v*` tag の Release）
- `workflow_dispatch`: tag 入力

### 処理概要

1. tag からバージョンを取得（`fulgur-chart-v0.1.9` → `0.1.9`）。
2. Matrix で各 target 向けに `cargo build --release -p fulgur-chart-cli` を実行。
   - `x86_64-unknown-linux-gnu`
   - `x86_64-unknown-linux-musl`
   - `aarch64-unknown-linux-gnu`
   - `aarch64-apple-darwin`
   - `x86_64-apple-darwin`
   - `x86_64-pc-windows-msvc`
3. 各 platform 用 npm パッケージの `package.json` を生成（または更新）し、ビルド済みバイナリを `bin/` にコピー。
4. 各 platform パッケージを `npm publish --provenance --access public`。
5. メタパッケージの `package.json` の optionalDependencies を各 platform パッケージのバージョンに更新し、`npm publish --provenance --access public`。

### 認証

npm provenance / OIDC（Trusted Publishing）を使用。GitHub Actions からの publish に必要な `id-token: write` permission を付与。npm 側での Trusted Publisher 設定は本ワークフロー作成後に手動で実施する。

## CI 統合

`.github/workflows/ci.yml` に `chart-cli` ジョブを追加する（runner は `ubuntu-latest` で、linux-x64 向けバイナリで検証する）。

- `cargo build --release -p fulgur-chart-cli` を実行。
- ビルド済みバイナリを `packages/npm/chart-cli-linux-x64/bin/fulgur-chart` にコピー。
- `packages/npm/chart-cli` から `npx . render examples/specs/bar.json -o /tmp/out.svg` を実行。
- 出力が `<svg` で始まり、既存 Rust CLI `cargo run -p fulgur-chart-cli -- render ...` の結果とバイト一致することを検証する。

## テスト

`packages/npm/chart-cli/__test__/launcher.test.js`（`node:test`、追加依存なし）:

- platform detection 関数のユニットテスト
  - `linux` + `x64` + glibc → `linux-x64`
  - `linux` + `x64` + musl → `linux-x64-musl`
  - `linux` + `arm64` → `linux-arm64`
  - `darwin` + `arm64` / `x64`
  - `win32` + `x64`
  - 未対応 platform → `null`
- 未対応 platform 実行時のエラーテスト（launcher の exit code / stderr）
- optionalDep 未インストール時のエラーテスト（`--ignore-optional` 相当で再現）

## 制約・保留事項

- 本設計では npm publish までのワークフローは作成するが、実際の publish を可能にするには npm registry 側で Trusted Publisher を登録する必要がある。
- 全 platform の prebuild を実際に publish して npx を検証する作業は、registry 設定後のリリース時に実施する。
- launcher は現状の `fulgur-chart` バイナリの引数をそのまま渡す。CLI インターフェースに変更があった場合、テストで検知する。
