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
| Environment | (空欄) |

- https://crates.io/crates/fulgur-chart/settings
- https://crates.io/crates/fulgur-chart-cli/settings

> 初回 publish は不要 (既に `0.1.0` を手動公開済み)。Trusted Publishing は初回 publish
> 後にのみ設定できる仕様だが、本プロジェクトは既に公開済みなので今すぐ登録できる。

### 2. GitHub App を作成して Secrets を登録

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
