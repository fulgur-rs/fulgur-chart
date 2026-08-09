# 横棒 x 軸目盛ラベルはみ出し修正 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 横棒チャートの最大 x 軸目盛ラベルを canvas 内に収め、目盛・棒・グリッド・軸線の右境界を一致させる。

**Architecture:** `build_horizontal` が `nice_ticks` の表示文字列を `TextMeasurer` で計測し、右凡例帯の内側から最大幅の半分を差し引いて `plot_right` を決める。既存の描画要素は同じ `plot_right` を共有するため、公開 API は変更しない。

**Tech Stack:** Rust、Cargo、`fulgur-chart` の `TextMeasurer`/`Prim::Text`、標準ユニットテスト、insta スナップショット。

## Global Constraints

- 指定された canvas 寸法は変更しない。
- 目盛ラベルの `Anchor::Middle` 配置は維持する。
- 余白は実際に `fmt_num` で描画する x 軸目盛文字列の最大幅から算出する。
- 現行ブランチでは対数軸実装がないため、対数軸固有の変更は追加しない。
- 実装コードを書く前に、回帰テストが期待どおり失敗することを確認する。

---

### Task 1: 横棒の右端目盛ラベル回帰テストを追加する

**Files:** `crates/fulgur-chart/src/layout/bar.rs` の `horizontal_axis_style_tests`。

**Interfaces:** 既存の `parse`、`build`、`TextMeasurer`、`Prim::Text` を利用し、`100000` の中央寄せラベルが `Scene::width` 内に収まることを検証する。

- [x] **Step 1: Write the failing test** — `[5,500,95000]` を入力し、`100000` の x 座標と計測幅を検証するテストを追加した。
- [x] **Step 2: Run the test to verify it fails** — `cargo test -p fulgur-chart horizontal_rightmost_tick_label_fits_inside_canvas --lib` で、修正前の `x=792` と半幅約`19.98`による失敗を確認した。

### Task 2: 最大目盛幅を `plot_right` の計算へ反映する

**Files:** `crates/fulgur-chart/src/layout/bar.rs:build_horizontal`。

**Interfaces:** `ticks.ticks`、`fmt_num`、`TextMeasurer::width`、`plot_left`、`legend_right` を使い、既存の `LinearScale` と全描画要素へラベルを収容した `plot_right` を渡す。

- [x] **Step 1: Write the minimal implementation** — 表示 tick 文字列の最大幅を計測し、`(spec.width - OUTER_PAD - legend_right - max_width / 2.0).max(plot_left)` を `plot_right` とした。
- [x] **Step 2: Run the focused test to verify it passes** — 同じ回帰テストが成功することを確認した。

### Task 3: 既存回帰とフォーマットを検証する

**Files:** `bar.rs`、`render_bar__horizontal_bar_snapshot.snap`、`render_stacked_bar__horizontal_stacked_snapshot.snap`。

**Interfaces:** 横棒の軸線テストと SVG スナップショットを新しい共有境界に合わせ、crate 全体の品質ゲートで確認する。

- [x] `cargo test -p fulgur-chart horizontal --lib` と `cargo test -p fulgur-chart --test render_bar` を実行した。
- [x] 共有境界変更に伴う横棒・横棒積み上げスナップショットを更新した。
- [x] `cargo fmt --all -- --check`、`git diff --check`、`cargo test -p fulgur-chart` を実行した。
- [x] 実装差分をコミットする。
