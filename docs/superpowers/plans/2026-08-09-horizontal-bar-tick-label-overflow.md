# 横棒 x 軸目盛ラベルはみ出し修正 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 横棒チャートの x 軸端目盛ラベルを canvas 内に収め、目盛・棒・グリッド・軸線の境界を一致させる。

**Architecture:** `build_horizontal` が `nice_ticks` の端点表示文字列を `TextMeasurer` で計測し、右凡例帯と端点余白を反映して `plot_left`/`plot_right` を決める。基準境界が狭すぎる場合は最小1px幅を残し、非有限の計測値は境界計算へ流さない。既存の描画要素は同じ境界を共有するため、公開 API は変更しない。

**Tech Stack:** Rust、Cargo、`fulgur-chart` の `TextMeasurer`/`Prim::Text`、標準ユニットテスト、insta スナップショット。

## Global Constraints

- 指定された canvas 寸法は変更しない。
- 目盛ラベルの `Anchor::Middle` 配置は維持する。
- 右余白は最後の `fmt_num` 済み x 軸目盛文字列、左余白は先頭文字列から独立して算出する。
- `MIN_HORIZONTAL_PLOT_WIDTH`（1px）を全ての有限入力で確保する。
- f32 の有限範囲を超えるフォントサイズ・計測幅は境界計算へ伝播させない。
- 現行ブランチでは対数軸実装がないため、対数軸固有の変更は追加しない。
- 実装コードを書く前に、回帰テストが期待どおり失敗することを確認する。

---

### Task 1: 横棒の右端目盛ラベル回帰テストを追加する

**Files:** `crates/fulgur-chart/src/layout/bar.rs` の `horizontal_axis_style_tests`。

**Interfaces:** 既存の `parse`、`build`、`TextMeasurer`、`Prim::Text` を利用し、`100000` の中央寄せラベルが `Scene::width` 内に収まることを検証する。

- [x] **Step 1: Write the failing test** — `[5,500,95000]` を入力し、`100000` の x 座標と計測幅を検証するテストを追加した。
- [x] **Step 2: Run the test to verify it fails** — `cargo test -p fulgur-chart horizontal_rightmost_tick_label_fits_inside_canvas --lib` で、修正前の `x=792` と半幅約`19.98`による失敗を確認した。

### Task 2: 端点目盛幅をプロット境界へ反映する

**Files:** `crates/fulgur-chart/src/layout/bar.rs:build_horizontal`。

**Interfaces:** `ticks.ticks`、`fmt_num`、`TextMeasurer::width`、`plot_left`、`legend_right` を使い、既存の `LinearScale` と全描画要素へ端点ラベルを収容した境界を渡す。

- [x] **Step 1: Write the minimal implementation** — 右端は終端 tick、左端は必要時のみ先頭 tick の幅を使い、基準幅が狭い場合は余白を比例縮小して1px幅を残す共有ヘルパーを追加した。
- [x] **Step 2: Run the focused test to verify it passes** — 同じ回帰テストが成功することを確認した。

### Task 3: 極端な境界と計測値を検証する

**Files:** `crates/fulgur-chart/src/layout/bar.rs` の `horizontal_axis_style_tests`。

- [x] 長いカテゴリラベル・右凡例・幅30pxの canvas でも最小プロット幅を確保するテストを追加した。
- [x] `fontSize=1e40` でも境界が有限かつ非縮退であることを検証するテストを追加した。
- [x] カテゴリ幅・凡例幅・目盛幅の非有限計測値を安全な有限値へフォールバックした。

### Task 4: 既存回帰とフォーマットを検証する

**Files:** `bar.rs`、`render_bar__horizontal_bar_snapshot.snap`、`render_stacked_bar__horizontal_stacked_snapshot.snap`。

**Interfaces:** 横棒の軸線テストと SVG スナップショットを新しい共有境界に合わせ、crate 全体の品質ゲートで確認する。

- [x] `cargo test -p fulgur-chart horizontal --lib` と `cargo test -p fulgur-chart --test render_bar` を実行した。
- [x] 共有境界変更に伴う横棒・横棒積み上げスナップショットを更新した。
- [x] `cargo fmt --all -- --check`、`git diff --check`、`cargo test -p fulgur-chart` を実行した。
- [x] 実装差分をコミットする。
