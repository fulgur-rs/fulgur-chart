# リリース手順

crates.io への publish は [release-plz](https://release-plz.dev) と GitHub Actions
(`.github/workflows/release-plz.yml`) で自動化されている。認証は crates.io の
[Trusted Publishing](https://crates.io/docs/trusted-publishing) (OIDC) を使い、
長期トークンはリポジトリに保持しない。

## 通常のリリースフロー

日常の操作は **リリース PR をマージするだけ**。

1. [Conventional Commits](https://www.conventionalcommits.org/)
   (`feat:`, `fix:`, `feat(progress):` など) で `main` に変更をマージする。
2. `release-plz-pr` ジョブが **リリース PR** を自動で作成/更新する。内容:
   - 各クレートのバージョン更新 (lib のバージョンと cli 側の `version = "x.y.z"`
     依存ピンを自動同期)
   - ルート `CHANGELOG.md` への追記
3. リリース PR の内容を確認してマージする。
4. `release-plz-release` ジョブが、マージされた版を **crates.io へ publish**
   (依存順に lib → cli)し、**git タグ**と **GitHub Release** を作成する。

> 現在 `fulgur-chart` / `fulgur-chart-cli` は `0.1.0` が公開済みのため、次のリリースは
> `0.1.1` または `0.2.0` になる。

<!-- -->

> **初回リリース PR で確認すること**:
> - ルート `CHANGELOG.md` の `[Unreleased]`（progress チャートの手動エントリ）が、
>   release-plz の自動生成と重複/競合していないか。重複する場合は手動エントリを削除する。
> - 両クレートを 1 つの CHANGELOG に集約しているため、デフォルトテンプレートでは
>   どちらのクレートの変更か区別しづらいことがある。気になる場合は `release-plz.toml`
>   の `[changelog]` セクションで `{{ package }}` を含む body テンプレートを設定する。

## 一度きりのセットアップ (手動)

ワークフローでは賄えない、最初に一度だけ必要な作業。

### 1. crates.io で Trusted Publisher を登録

**両方**のクレートに対して登録する (publisher はクレート単位)。
crates.io の各クレートページ → Settings → Trusted Publishing → GitHub から追加:

| 項目 | 値 |
|------|----|
| Repository owner | `fulgur-rs` |
| Repository name | `fulgur-chart` |
| Workflow filename | `release-plz.yml` |
| Environment | `crates-io` |

- https://crates.io/crates/fulgur-chart/settings
- https://crates.io/crates/fulgur-chart-cli/settings

> 初回 publish は不要 (既に `0.1.0` を手動公開済み)。Trusted Publishing は初回 publish
> 後にのみ設定できる仕様だが、本プロジェクトは既に公開済みなので今すぐ登録できる。

### 2. GitHub Environment `crates-io` を作成

`release-plz-release` ジョブは `environment: crates-io` を参照するため、
リポジトリに同名の Environment が必要。

リポジトリ Settings → Environments → **New environment**:

1. 名前: `crates-io`
2. **Required reviewers** にリリース承認者を追加
3. Save protection rules

これにより、`release-plz-release` は Required reviewers の承認後にのみ実行される。

### 3. GitHub App を作成して Secrets を登録

`GITHUB_TOKEN` で作成した PR は CI / release イベントをトリガーしないため、App トークンを使う。

1. GitHub App を作成 (Organization or 個人)。権限:
   - Repository → **Contents**: Read and write
   - Repository → **Pull requests**: Read and write
2. App を `fulgur-rs/fulgur-chart` にインストールする。
3. App の **App ID** と **Private key** を取得する。
4. リポジトリの Secrets に登録する:
   - `RELEASE_PLZ_APP_ID` — App ID
   - `RELEASE_PLZ_APP_PRIVATE_KEY` — Private key (PEM 全体)

## ローカルでの確認 (任意)

リリース PR の中身を事前に確認したい場合:

```bash
cargo install release-plz   # 未インストールなら
release-plz update          # バージョン更新と CHANGELOG をローカルに反映
git diff                    # 差分を確認
git restore .               # 確認後、変更を元に戻す
```

> `release-plz update` に `--dry-run` は無い。上記のように一度ローカルへ反映してから
> `git restore .` で戻すか、`release-plz release --dry-run` で publish 前チェックを行う。

---

## npm リリース (@fulgur-rs/chart-cli)

`.github/workflows/chart-cli-npm-release.yml` が GitHub Release イベント
(`fulgur-chart-cli-v*` タグ) で自動的に npm publish を実行する。
認証は `NPM_TOKEN` (Granular access token) を使う。

### 一度きりのセットアップ (手動)

#### 1. npm アクセストークンを作成

1. npmjs.com → Account Settings → Access Tokens → Generate New Token
2. **Granular access token** を選択 → 有効期限を設定 → `@fulgur-rs` スコープに **Read and write** 権限を付与
3. 生成されたトークンをコピーして安全に保管

#### 2. GitHub リポジトリシークレットに登録

リポジトリ Settings → Secrets and variables → Actions → New repository secret:

| 項目 | 値 |
|------|----|
| Name | `NPM_TOKEN` |
| Secret | 上で生成した npm Granular access token |

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

Actions → **Chart CLI NPM Release** → Run workflow → tag: `fulgur-chart-cli-v<VERSION>` (例: `fulgur-chart-cli-v0.1.10`)

既存の git タグに対してのみ動作する (ワークフローが `ref: ${{ env.RELEASE_TAG }}`
でチェックアウトするため)。

### OIDC Trusted Publishing への移行 (将来オプション)

npm の OIDC を使う場合は、7 パッケージすべてに対してそれぞれ
npmjs.com → パッケージ Settings → Trusted Publishing で GitHub Actions を登録し、
`chart-cli-npm-release.yml` の `NODE_AUTH_TOKEN` 行を削除する。
初回 publish 後にのみ設定可能 (npm の仕様)。

> **注意:** npm Trusted Publishing (OIDC) は Node.js 22.14.0+ と npm 11.5.1+ が必要。
> 移行時はワークフローの `node-version: "20"` を `"22"` に更新すること。
