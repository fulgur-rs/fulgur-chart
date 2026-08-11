# 横棒 x 軸目盛ラベルはみ出し修正 Implementation Plan

> タスク状態は beads の `fulgur-chart-53k` で管理する。この文書は設計と実施内容を記録する。

**Goal:** 横棒チャートの x 軸端目盛ラベルを canvas 内に収め、目盛・棒・グリッド・軸線の境界を一致させる。

**Architecture:** `build_horizontal` が `nice_ticks` の端点表示文字列を `TextMeasurer` で計測し、右凡例帯と端点余白を反映して `plot_left`/`plot_right` を決める。基準境界は canvas 内の有限な区間へ正規化し、狭すぎる場合も最小1px幅を残す。非有限の計測値は境界計算へ流さない。既存の描画要素は同じ境界を共有するため、公開 API は変更しない。

**Tech Stack:** Rust、Cargo、`fulgur-chart` の `TextMeasurer`/`Prim::Text`、標準ユニットテスト、insta スナップショット。

## Global Constraints

- 指定された canvas 寸法は変更しない。
- 目盛ラベルの `Anchor::Middle` 配置は維持する。
- 右余白は最後の `fmt_num` 済み x 軸目盛文字列、左余白は先頭文字列から独立して算出する。
- `MIN_HORIZONTAL_PLOT_WIDTH`（1px）を全ての有限入力で確保する。
- 最小幅のフォールバックを含め、プロット境界を指定 canvas の `[0, width]` 内に収める。
- カテゴリ・凡例・目盛の幅計測では、フォントサイズを f32 の有限範囲へ正規化する。`NaN` と負の無限大は `0`、正の無限大と f32 上限超過値は `f32::MAX` とし、計測結果が非有限なら幅 `0.0` にフォールバックする。
- 現行ブランチでは対数軸実装がないため、対数軸固有の変更は追加しない。
- 実装コードを書く前に、回帰テストが期待どおり失敗することを確認する。

---

### 実施項目 1: 横棒の右端目盛ラベル回帰テスト

**Files:** `crates/fulgur-chart/src/layout/bar.rs` の `horizontal_axis_style_tests`。

**Interfaces:** 既存の `parse`、`build`、`TextMeasurer`、`Prim::Text` を利用し、`100000` の中央寄せラベルが `Scene::width` 内に収まることを検証する。

`[5,500,95000]` を入力し、`100000` の x 座標と計測幅を検証するテストを追加した。`cargo test -p fulgur-chart horizontal_rightmost_tick_label_fits_inside_canvas --lib` で、修正前の `x=792` と半幅約`19.98`による失敗を確認した。

### 実施項目 2: 端点目盛幅をプロット境界へ反映

**Files:** `crates/fulgur-chart/src/layout/bar.rs:build_horizontal`。

**Interfaces:** `ticks.ticks`、`fmt_num`、`TextMeasurer::width`、`plot_left`、`legend_right` を使い、既存の `LinearScale` と全描画要素へ端点ラベルを収容した境界を渡す。

右端は終端 tick、左端は必要時のみ先頭 tick の幅を使い、基準幅が狭い場合は余白を比例縮小して1px幅を残す共有ヘルパーを追加した。同じ回帰テストが成功することを確認した。

### 実施項目 3: 極端な境界と計測値の検証

**Files:** `crates/fulgur-chart/src/layout/bar.rs` の `horizontal_axis_style_tests`。

長いカテゴリラベル・右凡例・幅30pxの canvas でも、最小プロット幅を canvas 内に確保するテストを追加した。

`fontSize=1e40` でも、測定時のフォントサイズを f32 の有限範囲へクランプし、境界が有限かつ非縮退になることを検証した。カテゴリ幅・凡例幅・目盛幅の非有限計測値は、安全な有限値へフォールバックする。`NaN` のフォントサイズで `TextMeasurer::width` が `NaN` を返すケースと、長い文字列・最大 f32 フォントサイズで `∞` を返すケースを直接テストし、どちらも `0.0` 幅へ変換されることを確認した。

### 実施項目 4: 既存回帰とフォーマットの検証

**Files:** `bar.rs`、`render_bar__horizontal_bar_snapshot.snap`、`render_stacked_bar__horizontal_stacked_snapshot.snap`。

**Interfaces:** 横棒の軸線テストと SVG スナップショットを新しい共有境界に合わせ、crate 全体の品質ゲートで確認する。

`cargo test -p fulgur-chart horizontal --lib` と `cargo test -p fulgur-chart --test render_bar` を実行し、共有境界変更に伴う横棒・横棒積み上げスナップショットを更新した。`cargo fmt --all -- --check`、`git diff --check`、`cargo test -p fulgur-chart` を実行し、実装差分をコミットした。
