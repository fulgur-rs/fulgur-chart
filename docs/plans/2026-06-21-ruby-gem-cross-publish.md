# Ruby Gem Cross-Publish Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ruby gem `fulgur_chart` をソース gem として正しくインストール可能にし、cross-gem プリビルドを release-plz 連動で RubyGems.org に Trusted Publishing で自動配布する。

**Architecture:** `ext/fulgur_chart/Cargo.toml` の path 依存を crates.io 版に切替え、gemspec のバージョンを `ext/fulgur_chart/Cargo.toml` から動的読み取りにする。`ruby-gem-release.yml` ワークフローが `fulgur-chart-v*` タグの GitHub Release をトリガーとして rb-sys-dock でクロスコンパイルし、OIDC Trusted Publishing で全 platform gem を push する。

**Tech Stack:** Ruby/rb-sys/magnus (既存), rb-sys-dock (cross-gem ビルド), rubygems/configure-rubygems-credentials (Trusted Publishing), GitHub Actions matrix

---

### Task 1: ext/fulgur_chart/Cargo.toml の path 依存解消

**Files:**
- Modify: `crates/bindings/ruby/ext/fulgur_chart/Cargo.toml`

**Step 1: 現在の内容を確認**

```bash
cat crates/bindings/ruby/ext/fulgur_chart/Cargo.toml
```

Expected: `fulgur-chart = { path = "../../../../fulgur-chart" }` が見える

**Step 2: path 依存を crates.io 版に変更し、package version を合わせる**

`crates/bindings/ruby/ext/fulgur_chart/Cargo.toml` を以下に変更:

```toml
[package]
name = "fulgur_chart"
version = "0.2.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
magnus = "0.7"
fulgur-chart = "0.2.0"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = { version = "1.2", features = ["preserve_order"] }
```

**Step 3: ビルドが通ることを確認**

```bash
cd crates/bindings/ruby
bundle exec rake compile 2>&1 | tail -5
```

Expected: `fulgur_chart.so` (または `.bundle`) が生成される。crates.io から `fulgur-chart 0.2.0` が解決されること。

**Step 4: Ruby テストが通ることを確認**

```bash
cd crates/bindings/ruby
bundle exec rake test 2>&1 | tail -10
```

Expected: `26 runs, 0 failures`

**Step 5: コミット**

```bash
git add crates/bindings/ruby/ext/fulgur_chart/Cargo.toml
git commit -m "fix(ruby): switch fulgur-chart dep from path to crates.io 0.2.0"
```

---

### Task 2: gemspec 修正（version 動的読み取り・Cargo.lock 追加・changelog_uri）

**Files:**
- Modify: `crates/bindings/ruby/fulgur_chart.gemspec`

**Step 1: 現在の gemspec を確認**

```bash
cat crates/bindings/ruby/fulgur_chart.gemspec
```

**Step 2: gemspec を修正**

`crates/bindings/ruby/fulgur_chart.gemspec` を以下に変更:

```ruby
# frozen_string_literal: true

# Version is read dynamically from ext/fulgur_chart/Cargo.toml so that
# `gem build` in CI always picks up the version set by the release workflow.
ext_toml = File.read(File.join(__dir__, "ext/fulgur_chart/Cargo.toml"))
crate_version = ext_toml.match(/^\[package\].*?^version\s*=\s*"([^"]+)"/m)[1]

Gem::Specification.new do |spec|
  spec.name = "fulgur_chart"
  spec.version = crate_version
  spec.authors = ["Fulgur"]
  spec.summary = "Render chart.js / Vega-Lite specs to deterministic SVG/PNG (Rust core)"
  spec.description = spec.summary
  spec.homepage = "https://github.com/fulgur-rs/fulgur-chart"
  spec.license = "MIT OR Apache-2.0"
  spec.required_ruby_version = ">= 3.0"

  spec.metadata = {
    "homepage_uri"    => spec.homepage,
    "source_code_uri" => spec.homepage,
    "changelog_uri"   => "#{spec.homepage}/blob/main/crates/fulgur-chart/CHANGELOG.md",
  }

  spec.files = Dir["lib/**/*.rb", "ext/**/*.{rs,toml,rb,lock}", "Cargo.lock", "README.md"]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/fulgur_chart/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9"
end
```

**Step 3: gem build でバージョンが正しいことを確認**

```bash
cd crates/bindings/ruby
gem build fulgur_chart.gemspec 2>&1
```

Expected: `Successfully built RubyGem: fulgur_chart-0.2.0.gem`

**Step 4: gem に Cargo.lock が含まれることを確認**

```bash
cd crates/bindings/ruby
gem contents fulgur_chart-0.2.0.gem | grep Cargo
```

Expected: `Cargo.lock` が出力に含まれる

**Step 5: 生成した gem ファイルを削除**

```bash
rm crates/bindings/ruby/fulgur_chart-*.gem
```

**Step 6: コミット**

```bash
git add crates/bindings/ruby/fulgur_chart.gemspec
git commit -m "fix(ruby): dynamic version from Cargo.toml, add Cargo.lock to gem files, add changelog_uri"
```

---

### Task 3: Rakefile に cross-gem 設定を追加

**Files:**
- Modify: `crates/bindings/ruby/Rakefile`

**Step 1: 現在の Rakefile を確認**

```bash
cat crates/bindings/ruby/Rakefile
```

**Step 2: cross_compile 設定を追加**

`crates/bindings/ruby/Rakefile` を以下に変更:

```ruby
# frozen_string_literal: true

require "rake/testtask"
require "rb_sys/extensiontask"

GEMSPEC = Gem::Specification.load("fulgur_chart.gemspec")

RbSys::ExtensionTask.new("fulgur_chart", GEMSPEC) do |ext|
  ext.lib_dir = "lib/fulgur_chart"
  ext.cross_compile = true
  ext.cross_platform = %w[
    x86_64-linux
    aarch64-linux
    x86_64-darwin
    arm64-darwin
    x86_64-mingw-ucrt
  ]
end

Rake::TestTask.new(test: :compile) do |t|
  t.libs << "test" << "lib"
  t.test_files = FileList["test/test_*.rb"]
  t.warning = false
end

task default: %i[compile test]
```

**Step 3: rake -T でタスク一覧が増えていることを確認**

```bash
cd crates/bindings/ruby
bundle exec rake -T 2>&1 | grep cross
```

Expected: `rake native:x86_64-linux` などのクロスコンパイルタスクが表示される

**Step 4: コミット**

```bash
git add crates/bindings/ruby/Rakefile
git commit -m "build(ruby): add cross_compile platforms to RbSys::ExtensionTask"
```

---

### Task 4: ruby-gem-release.yml ワークフロー作成

**Files:**
- Create: `.github/workflows/ruby-gem-release.yml`

**Step 1: ワークフローファイルを作成**

`.github/workflows/ruby-gem-release.yml` を以下の内容で作成:

```yaml
# Ruby gem の cross-gem プリビルドと RubyGems.org への自動配布。
# release-plz が fulgur-chart-v* タグを含む GitHub Release を publish したときに起動する。
# 認証は Trusted Publishing (OIDC) を使用するため API key は不要。
#
# 手動セットアップ (一度だけ):
#   RubyGems.org で fulgur_chart gem に Trusted Publisher を登録する。
#   Owner: fulgur-rs, Repo: fulgur-chart, Workflow: ruby-gem-release.yml

name: Ruby Gem Release

on:
  release:
    types: [published]

jobs:
  cross-gem:
    name: Build ${{ matrix.platform }}
    if: startsWith(github.ref_name, 'fulgur-chart-v')
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        platform:
          - x86_64-linux
          - aarch64-linux
          - x86_64-darwin
          - arm64-darwin
          - x86_64-mingw-ucrt

    steps:
      - uses: actions/checkout@v4

      - name: Set gem version from tag
        run: |
          VERSION="${GITHUB_REF_NAME#fulgur-chart-v}"
          sed -i "s/^version = .*/version = \"$VERSION\"/" \
            crates/bindings/ruby/ext/fulgur_chart/Cargo.toml
          sed -i "s/^fulgur-chart = .*/fulgur-chart = \"$VERSION\"/" \
            crates/bindings/ruby/ext/fulgur_chart/Cargo.toml

      - uses: oxidize-rb/actions/setup-ruby-and-rust@v1
        with:
          ruby-version: "3.3"
          bundler-cache: true
          cargo-cache: true
          working-directory: crates/bindings/ruby

      - uses: oxidize-rb/actions/cross-gem@v1
        id: cross-gem
        with:
          platform: ${{ matrix.platform }}
          ruby-versions: "3.1,3.2,3.3,3.4"
          working-directory: crates/bindings/ruby

      - uses: actions/upload-artifact@v4
        with:
          name: cross-gem-${{ matrix.platform }}
          path: ${{ steps.cross-gem.outputs.gem-path }}

  source-gem:
    name: Build source gem
    if: startsWith(github.ref_name, 'fulgur-chart-v')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set gem version from tag
        run: |
          VERSION="${GITHUB_REF_NAME#fulgur-chart-v}"
          sed -i "s/^version = .*/version = \"$VERSION\"/" \
            crates/bindings/ruby/ext/fulgur_chart/Cargo.toml
          sed -i "s/^fulgur-chart = .*/fulgur-chart = \"$VERSION\"/" \
            crates/bindings/ruby/ext/fulgur_chart/Cargo.toml

      - uses: ruby/setup-ruby@v1
        with:
          ruby-version: "3.3"
          bundler-cache: true
          working-directory: crates/bindings/ruby

      - name: Build source gem
        working-directory: crates/bindings/ruby
        run: gem build fulgur_chart.gemspec

      - uses: actions/upload-artifact@v4
        with:
          name: source-gem
          path: crates/bindings/ruby/fulgur_chart-*.gem

  push-gem:
    name: Push to RubyGems.org
    needs: [cross-gem, source-gem]
    runs-on: ubuntu-latest
    permissions:
      id-token: write
    environment:
      name: rubygems.org
      url: https://rubygems.org/gems/fulgur_chart

    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: "*-gem*"
          merge-multiple: true
          path: gems/

      - uses: ruby/setup-ruby@v1
        with:
          ruby-version: "3.3"

      - name: Configure RubyGems credentials (Trusted Publishing)
        uses: rubygems/configure-rubygems-credentials@v1

      - name: Push all gems
        run: gem push gems/*.gem
```

**Step 2: YAML 構文を確認**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ruby-gem-release.yml'))" && echo "YAML OK"
```

Expected: `YAML OK`

**Step 3: コミット**

```bash
git add .github/workflows/ruby-gem-release.yml
git commit -m "ci(ruby): add cross-gem release workflow with Trusted Publishing"
```

---

### Task 5: 最終確認

**Step 1: Rust テストが全て通ることを確認**

```bash
cargo test --workspace 2>&1 | tail -5
```

Expected: `test result: ok. 33 passed; 0 failed`

**Step 2: Ruby テストが通ることを確認**

```bash
cd crates/bindings/ruby
bundle exec rake test 2>&1 | tail -5
```

Expected: `26 runs, 0 failures`

**Step 3: gem build が正常に動作することを確認**

```bash
cd crates/bindings/ruby
gem build fulgur_chart.gemspec && gem contents fulgur_chart-0.2.0.gem | sort
rm fulgur_chart-*.gem
```

Expected: `Cargo.lock`、`lib/`、`ext/` が含まれる

**Step 4: 変更ファイルの最終確認**

```bash
git diff main --name-only
```

Expected:
```
.github/workflows/ruby-gem-release.yml
crates/bindings/ruby/Rakefile
crates/bindings/ruby/ext/fulgur_chart/Cargo.toml
crates/bindings/ruby/fulgur_chart.gemspec
```

---

## 手動セットアップ（実装後に必要）

実装完了・PR マージ後、以下を一度だけ手動で行う:

- RubyGems.org で `fulgur_chart` gem に Trusted Publisher を登録
  - Owner: `fulgur-rs`、Repo: `fulgur-chart`、Workflow: `ruby-gem-release.yml`
  - Branch: `main`（任意）
