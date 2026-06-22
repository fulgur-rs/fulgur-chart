# Line x-axis edge placement Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development) to implement this plan task-by-task.

**Goal:** line/area チャートの x 座標(マーカー点・x カテゴリラベル)を chart.js の `offset:false`(edge-to-edge, `i/(n-1)`)配置に合わせ、compat の geometry 次元を PASS させる。

**Architecture:** `common.rs` に line 専用の `line_x(frame, i, n)` を追加し、`line.rs` の点計算 2 箇所(`line_points` / `build`)と `common.rs::draw_frame` の x ラベル配置(`ChartKind::Line` のときのみ)を `category_center` → `line_x` に切り替える。bar の band 中心配置と mixed(bar を含むため offset:true)は変更しない。

**Tech Stack:** Rust (fulgur-chart core), insta スナップショット, Node の chartjs-compat ツール。

**前提:** ベースは origin/main (PR#50 マージ済み, line geometry 出力あり)。`fulgur-chart-omr` の design フィールドは「beginAtZero=false」も挙げるが、これは既に main で対応済み(`chartjs.rs` の `is_line` 除外)。本プランの実作業は x 座標(差異 1)のみ。

**現状(baseline, 再現済み):** `npm run compat -- line area` で両者 `FAIL [geometry]`。fulgur.nx=(i+0.5)/n vs chartjs.nx=i/(n-1)、最大差 0.0714(tol 0.02 超過)。

---

### Task 1: `line_x` を common.rs に追加(TDD)

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`(`category_center` 直後, 228 付近)
- Modify: import 行 `use crate::ir::{AxisSpec, ChartSpec, Color, LegendPos};` に `ChartKind` を追加
- Test: `crates/fulgur-chart/src/layout/line.rs`(tests モジュール)

**Step 1: 失敗するテストを書く**(line.rs tests に追加)

```rust
#[test]
fn line_points_x_is_edge_to_edge() {
    // n=3: 最初の点=plot_left, 最後の点=plot_right, 中央=中点。
    let spec = chartjs::parse(
        r#"{"type":"line","data":{"labels":["a","b","c"],
           "datasets":[{"data":[10,20,30]}]}}"#,
        false,
    )
    .unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = common::compute(&spec, &m);
    let ps = line_points(&spec, &frame);
    let s0: Vec<_> = ps.iter().filter(|p| p.series == 0).collect();
    assert!((s0[0].cx - frame.plot_left).abs() < 1e-9);
    assert!((s0[2].cx - frame.plot_right).abs() < 1e-9);
    assert!((s0[1].cx - (frame.plot_left + frame.plot_right) / 2.0).abs() < 1e-9);
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --lib line_points_x_is_edge_to_edge`
Expected: FAIL(現状 category_center なので cx[0] ≠ plot_left）。
※ Task 2 で line_points を line_x に切り替えるまで赤のまま。先に Task 1 で関数を追加する。

**Step 3: `line_x` を実装**(common.rs, `category_center` の直後)

```rust
/// line/area の x 座標。chart.js の category スケール offset:false(edge-to-edge)に合わせ、
/// n 個のカテゴリを [plot_left, plot_right] へ i/(n-1) で等間隔配置する。
/// bar の band 中心(category_center)とは異なる。n<=1 は (n-1)=0 で NaN になるため
/// プロット中央へフォールバックする(縮退ケース; 単一カテゴリの line fixture は無し)。
pub fn line_x(frame: &Frame, i: usize, n: usize) -> f64 {
    if n <= 1 {
        return frame.plot_left + (frame.plot_right - frame.plot_left) / 2.0;
    }
    frame.plot_left + i as f64 * (frame.plot_right - frame.plot_left) / (n - 1) as f64
}
```

**Step 4:** ここではまだ赤(line_points が未切替）。Task 2 で緑化する。

---

### Task 2: line.rs の点計算を line_x に切替

**Files:**
- Modify: `crates/fulgur-chart/src/layout/line.rs:23`(`line_points` 内)
- Modify: `crates/fulgur-chart/src/layout/line.rs:55`(`build` 内 valid 構築)

**Step 1: 置換**(2 箇所とも `common::category_center(frame, i, n)` → `common::line_x(frame, i, n)`、build 側は `&frame`)

**Step 2: テスト緑化を確認**

Run: `cargo test -p fulgur-chart --lib line_points`
Expected: `line_points_x_is_edge_to_edge` PASS、既存の count/monotone/cy も PASS。

---

### Task 3: x カテゴリラベルを Line のとき edge 配置に

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs:306-316`(`draw_frame` の x ラベルループ)

**Step 1: 置換** — Text prim の `x: category_center(frame, i, n)` を kind 分岐に:

```rust
let lx = if matches!(spec.kind, ChartKind::Line) {
    line_x(frame, i, n)
} else {
    category_center(frame, i, n)
};
```
を `if !cat.is_empty() && i % step == 0 {` の直後に置き、`x: lx,` を使う。auto-skip の `step`/`slot_w`(band 幅基準)はそのまま。

**Step 2: ビルド確認**

Run: `cargo build -p fulgur-chart`
Expected: 成功。

---

### Task 4: スナップショット再生成 + 視覚確認

**Files:**
- `crates/fulgur-chart/tests/snapshots/render_line__line_snapshot.snap`
- `crates/fulgur-chart/tests/snapshots/render_line__area_snapshot.snap`
- `crates/fulgur-chart/tests/snapshots/render_line__tension_snapshot.snap`
- `crates/fulgur-chart/tests/snapshots/inspect_model__snapshot_line_model.snap`(geometry nx)

**Step 1:** `cargo test -p fulgur-chart` を実行し、どのスナップショットが差分になるか確認。
**Step 2:** `cargo insta review`(または `INSTA_UPDATE=always`)で差分を**目視レビュー**。点と x ラベルの x が edge-to-edge(先頭=左端, 末尾=右端)に動き、ラベルが点の真下に揃うことを確認してから accept。盲目的 accept はしない。
**Step 3:** 黄金パス視覚確認 — `examples/out/line.svg` 等を再生成し、ラベルが点の下に並ぶか確認。

---

### Task 5: compat geometry PASS と全テスト検証 + コミット

**Step 1:** `cd tools && npm run compat -- line area`
Expected: `PASS line` / `PASS area`(geometry 次元 PASS、axes も PASS のまま）。
**Step 2:** `cd tools && node --test chartjs-compat/*.test.mjs`(compat 自体のテスト回帰なし)。
**Step 3:** `cargo test -p fulgur-chart`(全 PASS)、`cargo clippy -p fulgur-chart`、`cargo fmt --check`。
**Step 4:** bar/scatter の compat 回帰がないこと: `npm run compat -- bar scatter`(PASS のまま)。
**Step 5: コミット**

```bash
git add -A
git commit -m "fix(line): place x at i/(n-1) edge-to-edge to match chart.js offset:false"
```

---

## 受け入れ基準(fulgur-chart-omr)
- [ ] `npm run compat -- line` で geometry 次元 PASS
- [ ] `npm run compat -- line` で axes 次元 PASS(回帰なし)
- [ ] render_line スナップショット更新済み(視覚確認: ラベルが点の下に整列)
- [ ] `cargo test` 全 PASS
- [ ] bar/scatter の compat 回帰なし(mixed の line は band 中心のまま)
