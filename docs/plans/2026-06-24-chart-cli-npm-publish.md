# @fulgur-rs/chart-cli npm Publish Setup & Verification

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 2 つのドキュメント変更（releasing.md への npm 認証セットアップ手順追記、README への npx インストール手順追記）をコードコミットし、その後の手動オペレーション（NPM_TOKEN 設定 → release-plz PR マージ → npx 動作確認）を案内する。

**Architecture:** `chart-cli-npm-release.yml`（PR #61 でマージ済み）は release-plz が作成する `fulgur-chart-cli-v*` タグの GitHub Release をトリガーに、6 プラットフォームのバイナリをビルドして 7 つの npm パッケージを publish する。認証は `NPM_TOKEN` (Option A) を使う。OIDC (Option B) は 7 パッケージ分の npm 登録が必要なため初回は NPM_TOKEN が現実的。

**Tech Stack:** GitHub Actions, npm (Node.js 20), Rust/Cargo, release-plz, bash

---

### Task 1: releasing.md に npm リリース手順を追記

**Files:**
- Modify: `docs/releasing.md`

**Step 1: ファイル末尾に npm セクションを追加**

`docs/releasing.md` の末尾（最終行の後）に以下を追加する:

```markdown

---

## npm リリース (@fulgur-rs/chart-cli)

`.github/workflows/chart-cli-npm-release.yml` が GitHub Release イベント
(`fulgur-chart-cli-v*` タグ) で自動的に npm publish を実行する。
認証は `NPM_TOKEN` (クラシック Automation トークン) を使う。

### 一度きりのセットアップ (手動)

#### 1. npm アクセストークンを作成

1. npmjs.com → Account Settings → Access Tokens → Generate New Token
2. **Classic Token** → **Automation** タイプを選択 (CI/CD 向け、2FA 不要)
3. 生成されたトークンをコピーして安全に保管

#### 2. GitHub リポジトリシークレットに登録

リポジトリ Settings → Secrets and variables → Actions → New repository secret:

| 項目 | 値 |
|------|----|
| Name | `NPM_TOKEN` |
| Secret | 上で生成した npm Automation トークン |

#### 3. `@fulgur-rs` npm Organization を確認

`@fulgur-rs` スコープが npm Organization として存在することを確認する。
未作成の場合: npmjs.com → + → Create Organization → `fulgur-rs`

### npm リリースのフロー

通常は **release-plz PR をマージするだけ** で自動化される:

1. release-plz PR (例: PR #60) をマージ → `release-plz-release` ジョブが実行
2. release-plz が `fulgur-chart-cli-v0.1.10` タグと GitHub Release を作成
3. GitHub Release の `published` イベントが `chart-cli-npm-release.yml` をトリガー
4. 全 6 プラットフォームのバイナリがビルドされ、7 パッケージが npm publish される
   - `@fulgur-rs/chart-cli-linux-x64`
   - `@fulgur-rs/chart-cli-linux-x64-musl`
   - `@fulgur-rs/chart-cli-linux-arm64`
   - `@fulgur-rs/chart-cli-darwin-arm64`
   - `@fulgur-rs/chart-cli-darwin-x64`
   - `@fulgur-rs/chart-cli-win32-x64`
   - `@fulgur-rs/chart-cli` (メタパッケージ)

### 手動トリガー (workflow_dispatch)

Actions → **Chart CLI NPM Release** → Run workflow → tag: `fulgur-chart-cli-v0.1.10`

既存の GitHub Release タグに対してのみ動作する (ワークフローが `ref: ${{ env.RELEASE_TAG }}`
でチェックアウトするため)。

### OIDC Trusted Publishing への移行 (将来オプション)

npm の OIDC を使う場合は、7 パッケージすべてに対してそれぞれ
npmjs.com → パッケージ Settings → Trusted Publishing で GitHub Actions を登録し、
`chart-cli-npm-release.yml` の `NODE_AUTH_TOKEN` 行を削除する。
初回 publish 後にのみ設定可能 (npm の仕様)。
```

**Step 2: 動作確認**

```bash
wc -l docs/releasing.md
```

追加後: 行数が増えていることを確認する。

**Step 3: Commit**

```bash
git add docs/releasing.md
git commit -m "docs: add npm release setup guide to releasing.md"
```

---

### Task 2: README に npx インストール手順を追記

**Files:**
- Modify: `README.md`

**Step 1: Installation セクションを更新**

現在の Installation セクション（`cargo install --path` の 1 行のみ）を以下に置き換える:

```markdown
## Installation

### npx (ゼロインストール)

```sh
npx @fulgur-rs/chart-cli render chart.json -o chart.svg
```

Node.js 18+ が必要。初回実行時にプラットフォーム固有のバイナリが自動ダウンロードされる。

### Cargo

```sh
cargo install fulgur-chart-cli
```

インストール後は `fulgur-chart` コマンドが使えるようになる。

### ソースからビルド (開発向け)

```sh
cargo install --path crates/fulgur-chart-cli
```
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add npx zero-install section to README"
```

---

### Task 3: PR を作成し human へ NPM_TOKEN 設定を依頼 (human action)

**Step 1: ブランチを push して PR を作成**

```bash
git push origin feat/chart-cli-npm-publish
```

その後 GitHub で PR を作成: `feat/chart-cli-npm-publish` → `main`

**Step 2: Human action — NPM_TOKEN シークレットを登録**

リポジトリオーナーが以下を実施:
1. https://www.npmjs.com/settings/<user>/tokens でトークン生成 (Classic / Automation)
2. リポジトリ Settings → Secrets and variables → Actions → `NPM_TOKEN` として登録
3. https://www.npmjs.com/org/fulgur-rs で `@fulgur-rs` Organization の存在を確認

---

### Task 4: リリースをトリガーして npm に publish (human action)

**Step 1: この PR をマージ** (docs変更のみ)

**Step 2: release-plz PR #60 をマージ**

PR #60 (`fulgur-chart-cli: 0.1.9 → 0.1.10`) をマージすることで:
- `release-plz-release` ジョブが `fulgur-chart-cli-v0.1.10` タグ + GitHub Release を作成
- `chart-cli-npm-release.yml` が自動トリガーされる

**Step 3: ワークフローを監視**

Actions → **Chart CLI NPM Release** の最新実行を確認:
- 全 6 つの `build-binary` ジョブが ✓ になること
- `publish-packages` ジョブが ✓ になること
- https://www.npmjs.com/package/@fulgur-rs/chart-cli でバージョン `0.1.10` が表示されること

**ワークフローが失敗した場合の対処:**

| 失敗箇所 | 原因 | 対処 |
|---------|------|------|
| `publish-packages` の auth エラー | NPM_TOKEN 未登録または期限切れ | シークレットを再登録して workflow_dispatch で再実行 |
| `publish-packages` の 403 | `@fulgur-rs` org が存在しない | Organization を作成して再実行 |
| `build-binary` の クロスコンパイルエラー | ツールチェーン問題 | ログを確認してイシューを起票 |

---

### Task 5: npx 動作確認 (linux-x64)

**前提:** Task 4 で npm publish が成功していること。

**Step 1: テスト用 spec ファイルを作成**

```bash
cat > /tmp/npx-test-spec.json << 'EOF'
{
  "type": "bar",
  "data": {
    "labels": ["Jan", "Feb", "Mar"],
    "datasets": [{"label": "Sales", "data": [10, 20, 15]}]
  }
}
EOF
```

**Step 2: npx でレンダリングを実行**

```bash
npx @fulgur-rs/chart-cli render /tmp/npx-test-spec.json -o /tmp/npx-test.svg
```

**Step 3: 出力を確認**

```bash
head -3 /tmp/npx-test.svg
```

期待出力: `<svg` で始まる SVG テキスト。

**Step 4: イシューを close**

```bash
bd update fulgur-chart-wya.1 --notes="linux-x64 で npx @fulgur-rs/chart-cli 動作確認済み ✓"
bd close fulgur-chart-wya.1
```

---

## 補足: ワークフローと package.json のバージョン管理

`packages/npm/*/package.json` の `version` フィールドはプレースホルダー（現在 `0.1.9`）。
ワークフローが publish 時にタグから抽出したバージョン（例: `0.1.10`）で上書きするため、
静的なファイルのバージョンを手動で合わせる必要はない。
