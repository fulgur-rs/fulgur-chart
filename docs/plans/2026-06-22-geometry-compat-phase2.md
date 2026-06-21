# chart.js 適合参照ツール Phase 2 — geometry 正規化照合 (bar 先行) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** fulgur と chart.js の縦棒チャートの矩形ジオメトリを、プロット領域基準 [0,1] 正規化で数値・構造照合し、HTML レポートに画像オーバーレイで可視化する(beads: fulgur-chart-m80, プラン Task 10-11 を bar に限定)。

**Architecture:** 縦棒の矩形ジオメトリを純粋 pub 関数 `vertical_bar_boxes` に抽出し、レンダラ(`layout/bar.rs::build_vertical`)と意味モデル(`model.rs`)の両方がこれを呼ぶ「単一の真実源」にする。これでモデルが報告する geometry == 実際に描画される矩形を保証する。モデルへ `Geometry{plot_area, elements[{series,index,kind,nx,ny,nw,nh}]}` を追加し、JS 側 `extract.mjs` が chart.js の `getDatasetMeta` 要素を `chartArea` 基準で正規化、`diff.mjs` が geometry 次元(構造 + 数値 |Δ|<=0.02)を照合、`report.mjs` が両 PNG に正規化ボックスを SVG オーバーレイする。

**Tech Stack:** Rust (serde, insta スナップショット), Node.js (chart.js v4, node-canvas, node:test)。

**重要な事前知識:**
- 縦棒の矩形は現状 `layout/bar.rs::build_vertical` 内でインライン計算。定数 `GROUP_RATIO=0.8` / `BAND_PAD_RATIO=0.1` / `BAR_FILL_RATIO=0.9` は `bar.rs` private。
- `Frame{plot_left,plot_right,plot_top,plot_bottom, ticks:NiceTicks, ys:LinearScale}` はすべて `pub`(`layout/common.rs`)。`category_center(frame,i,n)` / `band_width(frame,n)` / `compute(spec,m)` も `pub`。`LinearScale::map(v)` は線形写像で `pub`。
- `ElemN{series,index,...}` が要する series/index は **計算時にしか存在しない**(Scene の `Prim::Rect` には載らない)。ゆえに描画済み Scene からの抽出は不可で、共有関数方式が必然。
- 安全網: `render_bar__bar_snapshot.snap`(SVG), `render_stacked_bar__*`(SVG), `render_datalabels.rs`, `golden_png.rs`(PNG ピクセル), `inspect_model__snapshot_bar_model.snap`(モデル YAML)。リファクタはこれらが守る。
- **正規化規約(両言語で完全一致させる):**
  - `plot_area`: **キャンバス基準** [0,1]。`{x:plot_left/width, y:plot_top/height, w:(plot_right-plot_left)/width, h:(plot_bottom-plot_top)/height}`。chart.js 側は `chartArea` を同様にキャンバス基準で正規化。オーバーレイで要素を画像座標へ写すために必要。
  - `ElemN.n*`: **プロット領域基準** [0,1]。`nx=(x-plot_left)/plotW`, `ny=(y-plot_top)/plotH`, `nw=w/plotW`, `nh=h/plotH`。
- **スコープ:** 縦棒(`bar`, `stacked-bar`)のみ。横棒・line・scatter・bubble は geometry=None(別 issue)。
- **検証すべきリスク(advisor 指摘):** `plot_area`(fulgur)と `chartArea`(chart.js)は両者とも軸ラベル/目盛/タイトル/凡例を除いた内側データ領域 — この**意味的**整合は構造上保証される。一方で 2 つのレイアウトエンジンが余白を**画素一致**させる保証はない(タイトルや長い y ラベルで余白が変わる)。各要素は「自分のプロット領域」で正規化済みのため要素座標は plot_area の画素差に頑健で、要素が tolerance 内で一致すること自体が分母の意味整合の証左になる。よって **`plot_area` 差は pass ゲートに含めず info(診断)として記録**し、HTML の破線ボックスで可視化する。要素 nx/ny/nw/nh + 構造のみで pass 判定する。

---

## Task 10a: 共有ジオメトリ関数 `vertical_bar_boxes` + `BarBox` 型

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs`

**Step 1: 失敗するテストを書く**(`bar.rs` 末尾に `#[cfg(test)] mod geom_tests`)

```rust
#[cfg(test)]
mod geom_tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::text::TextMeasurer;

    fn boxes_for(json: &str) -> Vec<BarBox> {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let frame = super::super::common::compute(&spec, &m);
        vertical_bar_boxes(&spec, &frame)
    }

    #[test]
    fn one_box_per_category_series_grouped() {
        // 3 カテゴリ × 2 系列 = 6 矩形。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B","C"],
              "datasets":[{"data":[10,20,30]},{"data":[5,15,25]}]}}"#,
        );
        assert_eq!(bs.len(), 6);
        // (series,index) が全組み合わせ網羅。
        for s in 0..2 {
            for i in 0..3 {
                assert!(bs.iter().any(|b| b.series == s && b.index == i));
            }
        }
    }

    #[test]
    fn boxes_left_to_right_by_category() {
        // 単系列: カテゴリ順に x が増加する。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#,
        );
        assert!(bs[0].x < bs[1].x && bs[1].x < bs[2].x);
        // 幅は正。
        assert!(bs.iter().all(|b| b.w > 0.0));
    }

    #[test]
    fn box_height_tracks_value_magnitude() {
        // 値が大きいほど高い矩形(baseline=0)。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,100]}]}}"#,
        );
        assert!(bs[1].h > bs[0].h);
    }

    #[test]
    fn stacked_collapses_to_one_column_per_category() {
        // 積み上げ: 2 カテゴリ × 2 系列、各カテゴリの 2 矩形は同じ x・同じ幅(縦に積む)。
        let bs = boxes_for(
            r#"{"type":"bar","data":{"labels":["A","B"],
              "datasets":[{"data":[10,20]},{"data":[30,40]}]},
              "options":{"scales":{"x":{"stacked":true},"y":{"stacked":true}}}}"#,
        );
        assert_eq!(bs.len(), 4);
        let cat0: Vec<&BarBox> = bs.iter().filter(|b| b.index == 0).collect();
        assert_eq!(cat0.len(), 2);
        assert_eq!(cat0[0].x, cat0[1].x);
        assert_eq!(cat0[0].w, cat0[1].w);
    }
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --lib layout::bar::geom_tests 2>&1 | tail -20`
Expected: `vertical_bar_boxes` / `BarBox` 未定義でコンパイルエラー。

**Step 3: 最小実装**(`bar.rs` の冒頭 const 群の直後、`build` の前に追加)

```rust
/// 縦棒1本のデータ矩形(ピクセル空間)。`series`=dataset index, `index`=category index。
/// `value` はラベル描画用に元値を保持する(geometry には出力しない)。
#[derive(Debug, Clone, PartialEq)]
pub struct BarBox {
    pub series: usize,
    pub index: usize,
    pub value: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 縦棒の全データ矩形を build_vertical と同一の式で算出する単一の真実源。
/// レンダラ(`build_vertical`)とモデル(`model::Geometry`)の両方がこれを呼ぶ。
/// 非積み上げ: category 外側 × series 内側で全 (i,sidx) を生成(欠損/非有限値も
///   `unwrap_or(0.0)` で矩形化、既存 build_vertical 挙動と一致)。
/// 積み上げ: category 外側 × series 内側で有限値のみ値空間に積む。
pub fn vertical_bar_boxes(spec: &ChartSpec, frame: &super::common::Frame) -> Vec<BarBox> {
    let n = spec.categories.len().max(1);
    let band_w = super::common::band_width(frame, n);
    let s = spec.series.len().max(1);
    let group_w = band_w * GROUP_RATIO;
    let bar_w = group_w / s as f64;
    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let baseline_y = frame.ys.map(base_v);
    let stacked = matches!(spec.kind, crate::ir::ChartKind::Bar { stacked: true, .. });

    let mut boxes = Vec::new();
    if stacked {
        let stack_w = (group_w * BAR_FILL_RATIO).max(0.0);
        for i in 0..spec.categories.len() {
            let band_left = super::common::category_center(frame, i, n) - band_w / 2.0;
            let bx = band_left + band_w * BAND_PAD_RATIO;
            let mut pos_acc = 0.0_f64;
            let mut neg_acc = 0.0_f64;
            for (sidx, ser) in spec.series.iter().enumerate() {
                let Some(&v) = ser.values.get(i) else {
                    continue;
                };
                if !v.is_finite() {
                    continue;
                }
                let (v0, v1) = if v >= 0.0 {
                    let lo = pos_acc;
                    pos_acc += v;
                    (lo, pos_acc)
                } else {
                    let hi = neg_acc;
                    neg_acc += v;
                    (neg_acc, hi)
                };
                let y0 = frame.ys.map(v0);
                let y1 = frame.ys.map(v1);
                let y_top = y0.min(y1);
                let h = (y1 - y0).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: stack_w,
                    h,
                });
            }
        }
    } else {
        for i in 0..spec.categories.len() {
            let band_left = super::common::category_center(frame, i, n) - band_w / 2.0;
            for (sidx, ser) in spec.series.iter().enumerate() {
                let bx = band_left + band_w * BAND_PAD_RATIO + sidx as f64 * bar_w;
                let v = ser.values.get(i).copied().unwrap_or(0.0);
                let vy = frame.ys.map(v);
                let y_top = vy.min(baseline_y);
                let h = (vy - baseline_y).abs();
                boxes.push(BarBox {
                    series: sidx,
                    index: i,
                    value: v,
                    x: bx,
                    y: y_top,
                    w: (bar_w * BAR_FILL_RATIO).max(0.0),
                    h,
                });
            }
        }
    }
    boxes
}
```

注: `build` の冒頭で `use` している `ChartSpec` は既存。`super::common::Frame` の参照は `bar.rs` から到達可能。

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib layout::bar::geom_tests 2>&1 | tail -20`
Expected: PASS(4 tests)。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/bar.rs
git commit -m "feat(layout): extract vertical_bar_boxes as shared bar geometry source"
```

---

## Task 10b: `build_vertical` を `vertical_bar_boxes` 消費へリファクタ

**目的:** レンダラの矩形・ラベルを共有関数の box から導出し、モデルと描画の単一真実源を確立する。**emit 順(矩形→そのラベル を要素ごと)を保持**して SVG スナップショットを不変に保つ。

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs`(`build_vertical` の bar 本体描画部, 旧 line 36-129 相当)

**Step 1: リファクタ前に安全網が緑であることを確認**

Run: `cargo test -p fulgur-chart --test render_bar --test render_stacked_bar --test render_datalabels --test golden_png 2>&1 | tail -15`
Expected: 全 PASS(現状の基準)。

**Step 2: `build_vertical` の bar 本体描画ループを置換**

`build_vertical` 内、`super::common::draw_frame(&mut items, spec, &frame, m);` の後ろ、`Scene { ... }` の前にある「bar 本体」ブロック(`let n = spec.categories.len().max(1);` から stacked/else の二重ループ全体)を、以下で置換する:

```rust
    // bar 本体: 矩形は共有 vertical_bar_boxes(単一真実源)から、値ラベルは box から導出。
    let base_v = 0.0_f64.clamp(frame.ticks.min, frame.ticks.max);
    let stacked = matches!(spec.kind, crate::ir::ChartKind::Bar { stacked: true, .. });
    for b in vertical_bar_boxes(spec, &frame) {
        let ser = &spec.series[b.series];
        items.push(Prim::Rect {
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
            fill: ser.fill_at(b.index),
        });
        if !spec.data_labels {
            continue;
        }
        let cx = b.x + b.w / 2.0;
        if stacked {
            // セグメント中央(box 中心 = 値中点; ys は線形なので一致)に値ラベル。
            let mid_y = b.y + b.h / 2.0;
            items.push(value_label(
                cx,
                mid_y + label_font * super::common::TEXT_BASELINE_RATIO,
                label_font,
                Anchor::Middle,
                ink,
                b.value,
            ));
        } else if ser.values.get(b.index).is_some() && b.value.is_finite() {
            // 正の棒は上端の少し上、負の棒は下端の下にラベル。
            let label_y = if b.value >= base_v {
                b.y - LABEL_GAP
            } else {
                b.y + b.h + label_font
            };
            items.push(value_label(cx, label_y, label_font, Anchor::Middle, ink, b.value));
        }
    }
```

注: `LABEL_GAP` / `value_label` / `TEXT_BASELINE_RATIO` は既存 `use super::common::{LABEL_GAP, value_label};` と `super::common::TEXT_BASELINE_RATIO` 経由。`ink`/`label_font` は `build_vertical` 冒頭で定義済み。置換で不要になった旧ローカル(`n`/`band_w`/`group_w`/`bar_w`/`baseline_y` 等)は削除する。

**Step 3: スナップショット不変を確認(リファクタの等価性検証)**

Run: `cargo test -p fulgur-chart --test render_bar --test render_stacked_bar --test render_datalabels --test golden_png 2>&1 | tail -20`
Expected: 全 PASS、スナップショット差分なし。

> もし `render_datalabels` / `render_stacked_bar` のラベル座標が末尾桁のみ動いた場合(積み上げラベルの `ys.map((v0+v1)/2)` → `box中心` の浮動小数差由来)、`cargo insta review` で差分を目視し「ラベル y の末尾桁のみ」であることを確認して受理する。矩形座標・golden PNG は不変でなければならない(不変でなければ box 式が build_vertical と非等価 → 修正)。

**Step 4: lib 全体回帰**

Run: `cargo test -p fulgur-chart --lib 2>&1 | tail -8`
Expected: PASS(130+ tests)。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/bar.rs
git commit -m "refactor(layout): draw vertical bars from shared vertical_bar_boxes"
```

---

## Task 10c: モデルに `Geometry` を追加(縦棒のみ)

**Files:**
- Modify: `crates/fulgur-chart/src/model.rs`

**Step 1: 失敗するテストを書く**(`model::tests` に追記)

```rust
    #[test]
    fn bar_has_normalized_geometry() {
        let json = r#"{"type":"bar","data":{"labels":["A","B","C"],
          "datasets":[{"data":[10,20,30]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let g = model.geometry.expect("縦棒には geometry があるべき");
        // plot_area はキャンバス [0,1] 内、要素はプロット領域 [0,1] 内。
        assert!(g.plot_area.x > 0.0 && g.plot_area.x < 1.0);
        assert!(g.plot_area.w > 0.0 && g.plot_area.w <= 1.0);
        assert_eq!(g.elements.len(), 3);
        for e in &g.elements {
            assert_eq!(e.kind, "bar");
            assert!(e.nx >= 0.0 && e.nx <= 1.0, "nx={}", e.nx);
            assert!(e.nw > 0.0 && e.nw <= 1.0, "nw={}", e.nw);
            assert!(e.nh >= 0.0 && e.nh <= 1.0, "nh={}", e.nh);
        }
        // 左→右にカテゴリが並ぶ。
        assert!(g.elements[0].nx < g.elements[1].nx);
        assert!(g.elements[1].nx < g.elements[2].nx);
        // 値が大きいほど高い。
        assert!(g.elements[2].nh > g.elements[0].nh);
    }

    #[test]
    fn pie_has_no_geometry() {
        let json = r#"{"type":"pie","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.geometry.is_none());
    }

    #[test]
    fn horizontal_bar_has_no_geometry_yet() {
        // 横棒は今回スコープ外: geometry=None。
        let json = r#"{"type":"bar","data":{"labels":["a","b"],
          "datasets":[{"data":[10,90]}]},"options":{"indexAxis":"y"}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.geometry.is_none());
    }
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --lib model::tests 2>&1 | tail -20`
Expected: `geometry` フィールド / 型未定義でコンパイルエラー。

**Step 3: 最小実装**(`model.rs`)

`ChartModel` に geometry フィールドを追加(`counts` の後ろ):

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
```

型定義を追加(`Counts` 定義の後ろ付近):

```rust
/// 矩形/プロット領域の正規化座標(チャート間ジオメトリ照合用)。
#[derive(Debug, Serialize, PartialEq)]
pub struct RectN {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 単一データ要素の正規化ジオメトリ。n* はプロット領域基準 [0,1]。
#[derive(Debug, Serialize, PartialEq)]
pub struct ElemN {
    pub series: usize,
    pub index: usize,
    pub kind: String,
    pub nx: f64,
    pub ny: f64,
    pub nw: f64,
    pub nh: f64,
}

/// チャートのジオメトリ。plot_area はキャンバス基準 [0,1]、elements はプロット領域基準。
#[derive(Debug, Serialize, PartialEq)]
pub struct Geometry {
    pub plot_area: RectN,
    pub elements: Vec<ElemN>,
}

/// 縦棒のジオメトリを共有 `vertical_bar_boxes` から構築する(描画と単一真実源)。
/// 縦棒以外、または退化プロット領域(幅/高さ<=0)は None。
fn compute_geometry(spec: &ChartSpec, m: &TextMeasurer) -> Option<Geometry> {
    match &spec.kind {
        ChartKind::Bar {
            horizontal: false, ..
        } => {
            let frame = crate::layout::common::compute(spec, m);
            let pw = frame.plot_right - frame.plot_left;
            let ph = frame.plot_bottom - frame.plot_top;
            if pw <= 0.0 || ph <= 0.0 {
                return None;
            }
            let plot_area = RectN {
                x: frame.plot_left / spec.width,
                y: frame.plot_top / spec.height,
                w: pw / spec.width,
                h: ph / spec.height,
            };
            let elements = crate::layout::bar::vertical_bar_boxes(spec, &frame)
                .iter()
                .map(|b| ElemN {
                    series: b.series,
                    index: b.index,
                    kind: "bar".to_string(),
                    nx: (b.x - frame.plot_left) / pw,
                    ny: (b.y - frame.plot_top) / ph,
                    nw: b.w / pw,
                    nh: b.h / ph,
                })
                .collect();
            Some(Geometry {
                plot_area,
                elements,
            })
        }
        _ => None,
    }
}
```

`build_model_core` が返す `ChartModel` リテラルに `geometry: None,` を追加(コア段階では未計算)。
`build_model` を更新して geometry を載せる:

```rust
pub fn build_model(spec: &ChartSpec, m: &TextMeasurer) -> ChartModel {
    let mut model = build_model_core(spec);
    if let Some((x, y, y_ticks)) = compute_axes(spec, m) {
        model.counts.y_ticks = y_ticks;
        model.axes = Some(Axes { x, y });
    }
    model.geometry = compute_geometry(spec, m);
    model
}
```

注: `crate::layout::bar::vertical_bar_boxes` を呼ぶため `bar` モジュールが crate 内から可視であること(`scatter::axis_domain` を既に呼べているので同様に可視)を確認。

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib model 2>&1 | tail -10`
Expected: PASS。

**Step 5: inspect スナップショット更新**

bar モデルに geometry が増えるためスナップショットを更新する(他の pie/line/scatter/bar-horizontal は geometry=None で不変)。

Run: `cargo insta test --review -p fulgur-chart --test inspect_model 2>&1 | tail -20`
(または `INSTA_UPDATE=always cargo test -p fulgur-chart --test inspect_model`)
Expected: `snapshot_bar_model` のみ geometry セクション追加で差分。目視で plot_area が妥当(x≈0.05-0.1, w≈0.9 前後)、要素 5 本(bar.json は 5 カテゴリ)が nx 昇順・nh が値に比例していることを確認して受理。

Run(固定確認): `cargo test -p fulgur-chart --test inspect_model 2>&1 | tail -8`
Expected: PASS(差分なし)。

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/model.rs crates/fulgur-chart/tests/snapshots/
git commit -m "feat(model): add normalized bar geometry from shared boxes"
```

---

## Task 11a: chart.js 側 geometry 抽出(`extract.mjs`)

**Files:**
- Modify: `tools/chartjs-compat/extract.mjs`
- Test: `tools/chartjs-compat/extract.test.mjs`(末尾に追記)

**前提:** `cd tools && npm install`(chart.js/canvas は導入済み)。

**Step 1: 失敗するテストを書く**(`extract.test.mjs` に追記)

```js
test('bar: chartArea 基準の正規化 geometry を出力', async () => {
  const spec = { type: 'bar', data: { labels: ['A','B','C'],
    datasets: [{ data: [10,20,30] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.ok(model.geometry, 'geometry を持つべき');
  assert.equal(model.geometry.elements.length, 3);
  const { plot_area, elements } = model.geometry;
  // plot_area はキャンバス [0,1]。
  assert.ok(plot_area.x > 0 && plot_area.x < 1);
  assert.ok(plot_area.w > 0 && plot_area.w <= 1);
  for (const e of elements) {
    assert.equal(e.kind, 'bar');
    assert.ok(e.nx >= 0 && e.nx <= 1, `nx=${e.nx}`);
    assert.ok(e.nw > 0 && e.nw <= 1, `nw=${e.nw}`);
  }
  // 左→右にカテゴリ、値が大きいほど高い。
  assert.ok(elements[0].nx < elements[1].nx);
  assert.ok(elements[2].nh > elements[0].nh);
});

test('horizontal bar は geometry を出さない(スコープ外)', async () => {
  const spec = { type: 'bar', data: { labels: ['A','B'], datasets: [{ data: [10,90] }] },
    options: { indexAxis: 'y' } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.equal(model.geometry, undefined);
});
```

**Step 2: 失敗を確認**

Run: `cd tools && node --test chartjs-compat/extract.test.mjs 2>&1 | tail -20`
Expected: `model.geometry` undefined で失敗。

**Step 3: 実装**(`extract.mjs`)

`extractChartjsModel` 内、`const png = canvas.toBuffer('image/png');` の前に geometry を組み立て、戻り値に追加する。ヘルパ関数をファイル内に追加:

```js
/// 縦棒の BarElement を chartArea 基準 [0,1] へ正規化。横棒(indexAxis:'y')と
/// 非 bar は undefined(fulgur 側スコープに揃える)。
function barGeometry(chart, spec, width, height) {
  const indexAxis = (spec.options && spec.options.indexAxis) || 'x';
  if (spec.type !== 'bar' || indexAxis === 'y') return undefined;
  const a = chart.chartArea;
  const caw = a.right - a.left;
  const cah = a.bottom - a.top;
  if (!(caw > 0) || !(cah > 0)) return undefined;
  const elements = [];
  for (let s = 0; s < spec.data.datasets.length; s++) {
    const meta = chart.getDatasetMeta(s);
    for (let i = 0; i < meta.data.length; i++) {
      const { x, y, base, width: bw } = meta.data[i].getProps(
        ['x', 'y', 'base', 'width'],
        true,
      );
      const left = x - bw / 2;
      const top = Math.min(y, base);
      const h = Math.abs(base - y);
      elements.push({
        series: s,
        index: i,
        kind: 'bar',
        nx: (left - a.left) / caw,
        ny: (top - a.top) / cah,
        nw: bw / caw,
        nh: h / cah,
      });
    }
  }
  return {
    plot_area: { x: a.left / width, y: a.top / height, w: caw / width, h: cah / height },
    elements,
  };
}
```

戻り値オブジェクトに `geometry` を追加(`png` の隣):

```js
  const geometry = barGeometry(chart, spec, width, height);
  const png = canvas.toBuffer('image/png');
  chart.destroy();

  return {
    meta: { type: spec.type, width, height },
    axes,
    series,
    counts: { /* 既存のまま */ },
    geometry,
    png,
  };
```

注: `getProps([...], true)` の第2引数 `true` はアニメーション完了後の最終値を返す(`animation:false` だが安全側)。`base` は縦棒のベースライン y。**実装時に確認**: chart.js v4 の `BarElement` が `base` を返すこと。万一 `base` が undefined のときは `height` を直接使ってフォールバック(`const h = bw_base !== undefined ? Math.abs(base - y) : height;`)。`getProps` に `'height'` も足して保険にしてよい。

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test chartjs-compat/extract.test.mjs 2>&1 | tail -20`
Expected: PASS。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/extract.mjs tools/chartjs-compat/extract.test.mjs
git commit -m "feat(compat): extract chart.js bar geometry normalized to chartArea"
```

---

## Task 11b: geometry 差分次元(`diff.mjs`)

**Files:**
- Modify: `tools/chartjs-compat/diff.mjs`
- Test: `tools/chartjs-compat/diff.test.mjs`(末尾に追記)

**Step 1: 失敗するテストを書く**(`diff.test.mjs`)

```js
const geomBase = () => ({
  plot_area: { x: 0.08, y: 0.05, w: 0.9, h: 0.85 },
  elements: [
    { series: 0, index: 0, kind: 'bar', nx: 0.10, ny: 0.70, nw: 0.20, nh: 0.30 },
    { series: 0, index: 1, kind: 'bar', nx: 0.50, ny: 0.40, nw: 0.20, nh: 0.60 },
  ],
});

test('geometry 一致は PASS', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, true);
});

test('geometry 座標ズレ(>0.02)は FAIL', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  c.geometry.elements[1].nx = 0.55; // 0.05 ズレ
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, false);
});

test('plot_area ズレは pass に影響せず info に記録される', () => {
  // 2 エンジンの余白差は要素正規化で吸収されるため pass を落とさない(診断のみ)。
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  c.geometry.plot_area.w = 0.80; // 内側領域の取り方が違う
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, true);
  assert.ok(r.dimensions.geometry.info.some((d) => d.field === 'plot_area.w'));
});

test('bar 高さ単調性の崩れは FAIL', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  // fulgur: nh 0.30→0.60 (増加)。chartjs を 0.60→0.30 (減少) に。
  c.geometry.elements[0].nh = 0.60;
  c.geometry.elements[1].nh = 0.30;
  c.geometry.elements[1].ny = 0.70;
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, false);
});

test('片方に geometry が無ければ skip', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = base(); // geometry なし
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.skipped, true);
  assert.equal(r.dimensions.geometry.pass, true);
});
```

(注: `base()` は既存ファイル先頭のフィクスチャ。`{ ...base(), geometry }` で geometry を足す。)

**Step 2: 失敗を確認**

Run: `cd tools && node --test chartjs-compat/diff.test.mjs 2>&1 | tail -20`
Expected: `dimensions.geometry` undefined で失敗。

**Step 3: 実装**(`diff.mjs`)

`diffGeometry` を追加(`diffModels` の前):

```js
const gsgn = (d) => (Math.abs(d) < TOLERANCES.geometryNorm ? 0 : Math.sign(d));

// 両モデルの geometry を構造+数値照合する。pass は要素座標(プロット領域基準)と
// 構造(要素数・左→右順序・系列ごと bar 高さ単調性)のみで判定する。
// plot_area(キャンバス基準)の差は info として記録するが pass には含めない:
// 2 つのレイアウトエンジンが余白(OUTER_PAD/軸幅/タイトル帯)を画素一致させる
// 保証はなく、各要素は「自分のプロット領域」で正規化済みのため plot_area 差に
// 頑健だから。要素が tolerance 内で一致する = 分母が意味的に整合していた、の証左。
function diffGeometry(fg, cg) {
  const tol = TOLERANCES.geometryNorm;
  const diffs = [];
  const info = [];

  // plot_area: 診断情報のみ(pass 不参加)。内側領域の取り方の目安。
  for (const k of ['x', 'y', 'w', 'h']) {
    if (Math.abs(fg.plot_area[k] - cg.plot_area[k]) > tol)
      info.push({ field: `plot_area.${k}`, fulgur: fg.plot_area[k], chartjs: cg.plot_area[k] });
  }

  // 構造: 要素数。
  if (fg.elements.length !== cg.elements.length) {
    diffs.push({ field: 'element_count', fulgur: fg.elements.length, chartjs: cg.elements.length });
    return { pass: false, diffs, info }; // ペアリング不能なので以降は省略。
  }

  const key = (e) => `${e.series}:${e.index}`;
  const cmap = new Map(cg.elements.map((e) => [key(e), e]));

  // 数値: (series,index) で対応付けて nx/ny/nw/nh を比較。
  for (const fe of fg.elements) {
    const ce = cmap.get(key(fe));
    if (!ce) {
      diffs.push({ field: `elem[${key(fe)}]`, fulgur: 'present', chartjs: 'missing' });
      continue;
    }
    for (const k of ['nx', 'ny', 'nw', 'nh']) {
      if (Math.abs(fe[k] - ce[k]) > tol)
        diffs.push({ field: `elem[${key(fe)}].${k}`, fulgur: fe[k], chartjs: ce[k] });
    }
  }

  // 構造: 左→右順序(nx 昇順の (series,index) 列が一致)。
  const order = (els) => [...els].sort((a, b) => a.nx - b.nx).map(key).join(',');
  if (order(fg.elements) !== order(cg.elements))
    diffs.push({ field: 'order', fulgur: order(fg.elements), chartjs: order(cg.elements) });

  // 構造: 系列ごとの bar 高さ(nh)単調性。連続する index の nh 増減符号が一致。
  const bySeries = (els) => {
    const m = new Map();
    for (const e of [...els].sort((a, b) => a.index - b.index)) {
      if (!m.has(e.series)) m.set(e.series, []);
      m.get(e.series).push(e.nh);
    }
    return m;
  };
  const fh = bySeries(fg.elements);
  const ch = bySeries(cg.elements);
  for (const [s, hs] of fh) {
    const cs = ch.get(s);
    if (!cs || cs.length !== hs.length) continue;
    for (let i = 1; i < hs.length; i++) {
      if (gsgn(hs[i] - hs[i - 1]) !== gsgn(cs[i] - cs[i - 1])) {
        diffs.push({ field: `monotonicity.series[${s}]`, fulgur: hs, chartjs: cs });
        break;
      }
    }
  }

  return { pass: diffs.length === 0, diffs, info };
}
```

`diffModels` の `counts` 次元の後ろ、`const pass = ...` の前に追加:

```js
  // geometry(両方にある場合のみ照合)。
  if (fulgur.geometry && chartjs.geometry) {
    dims.geometry = diffGeometry(fulgur.geometry, chartjs.geometry);
  } else {
    dims.geometry = { pass: true, skipped: true };
  }
```

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test chartjs-compat/diff.test.mjs 2>&1 | tail -20`
Expected: PASS(既存 + 新規 geometry テスト)。

**Step 5: 実 chart.js との早期経験的照合(プラン前提の検証)**

このプランは「fulgur の bar レイアウトが chart.js と 2% 以内で一致する」を前提にしている。これは合成テストでは検証できない経験的主張なので、抽出(11a)+差分(11b)が揃った今すぐ実測する(レポート整備 11c の前)。`inspect` は Task 10c で geometry を出すため、`compat.mjs` は既に geometry 次元を計算できる。

```bash
cd tools && npm run compat -- bar 2>&1 | tail -8
python3 -c "import json; d=json.load(open('report/bar.json')); g=d['diff']['dimensions']['geometry']; print('geometry pass:', g.get('pass')); print('element diffs:', [x['field'] for x in g.get('diffs',[])]); print('plot_area info:', g.get('info'))"
```
Expected: `geometry pass: True`(要素 nx/ny/nw/nh が tolerance 内)。`plot_area info` に差が出ても pass は True のまま(=#1 の設計が正しいことの確認)。

> もし要素差分が出たら: nw が一致せず nx/ny だけズレる → 余白起点の系統ズレ(plot_area 正規化の起点ズレ。各要素は自領域基準なので通常は吸収されるが、要確認)。nh だけズレる → baseline/y スケールの不一致(Phase 1 の axes は一致しているはずなので稀)。tolerance を緩める前に、まず実差分値を見て原因(chart.js の categoryPercentage/barPercentage 既定との差)を特定する。fulgur は GROUP_RATIO=0.8≈categoryPercentage、BAR_FILL_RATIO=0.9≈barPercentage。

**Step 6: コミット**

```bash
git add tools/chartjs-compat/diff.mjs tools/chartjs-compat/diff.test.mjs
git commit -m "feat(compat): geometry diff dimension (structural + normalized numeric)"
```

---

## Task 11c: HTML 画像オーバーレイ(`report.mjs` + `compat.mjs`)

**Files:**
- Modify: `tools/chartjs-compat/compat.mjs`(result に geometry を載せる)
- Modify: `tools/chartjs-compat/report.mjs`(geometry セクション + badge)

**Step 1: `compat.mjs` で geometry を result に載せる**

`result` オブジェクト(`const result = { name, pass, diff, cross };`)に geometry を追加:

```js
  const result = {
    name,
    pass: diff.pass && cross.pass,
    diff,
    cross,
    geometry: {
      fulgur: fulgurModel.geometry ?? null,
      chartjs: chartjs.geometry ?? null,
    },
  };
```

**Step 2: `report.mjs` に geometry 次元 badge + オーバーレイセクション**

badges 配列に geometry を追加(`dims.geometry` は diff 次元として存在):

```js
    badge('geometry', dims.geometry.pass, dims.geometry.skipped),
```
(`badge('counts', ...)` の後、`badge('crosscheck', ...)` の前に挿入。)

geometry の要素差分(FAIL 時)も既存 diff テーブルへ載せる。`diffTableBody` 配列に追加:

```js
    diffRows('geometry', dims.geometry),
```
(`diffRows('counts', dims.counts),` の後ろ。`diffRows` は `dim.diffs` を読むので、`info`(plot_area 診断)は表に出ず pass にも影響しない — 意図通り。)

オーバーレイ SVG 生成関数を `report.mjs` に追加(`esc` などの近く):

```js
/// 正規化 geometry を画像上の SVG オーバーレイ(viewBox 0 0 1 1)に変換する。
/// plot_area(キャンバス基準)で要素ボックス(プロット領域基準)を画像座標へ写す。
function overlaySvg(geometry, stroke) {
  if (!geometry || !geometry.plot_area) return '';
  const pa = geometry.plot_area;
  const boxes = (geometry.elements || [])
    .map((e) => {
      const x = pa.x + e.nx * pa.w;
      const y = pa.y + e.ny * pa.h;
      const w = e.nw * pa.w;
      const h = e.nh * pa.h;
      return `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="none" stroke="${stroke}" stroke-width="1.5" vector-effect="non-scaling-stroke"/>`;
    })
    .join('');
  const area = `<rect x="${pa.x}" y="${pa.y}" width="${pa.w}" height="${pa.h}" fill="none" stroke="${stroke}" stroke-width="1" stroke-dasharray="4 3" vector-effect="non-scaling-stroke" opacity="0.6"/>`;
  return `<svg class="ov" viewBox="0 0 1 1" preserveAspectRatio="none">${area}${boxes}</svg>`;
}
```

`writeHtmlReport` 内で geometry セクションを組み立てる。`crossSection` を作っている箇所の後ろに追加:

```js
  const geo = result.geometry || {};
  const geomSection =
    geo.fulgur || geo.chartjs
      ? `<h2>Geometry overlay (normalized boxes on render)</h2>
<div class="images">
  <figure>
    <figcaption>fulgur</figcaption>
    <div class="imgwrap">
      <img alt="fulgur ${esc(name)}" src="data:image/png;base64,${fulgurB64}">
      ${overlaySvg(geo.fulgur, '#1565c0')}
    </div>
  </figure>
  <figure>
    <figcaption>chart.js</figcaption>
    <div class="imgwrap">
      <img alt="chart.js ${esc(name)}" src="data:image/png;base64,${chartjsB64}">
      ${overlaySvg(geo.chartjs, '#c62828')}
    </div>
  </figure>
</div>`
      : '';
```

CSS に imgwrap/ov を追加(`<style>` 内、`.images img { ... }` の後ろ):

```css
  .imgwrap { position: relative; display: inline-block; line-height: 0; }
  .imgwrap img { max-width: 480px; height: auto; }
  .imgwrap svg.ov { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; }
```

`html` テンプレートの本文に `${geomSection}` を挿入(`${diffSection}` の前、または `${crossSection}` の後ろ — 既存 images の下が見やすい。`<div class="images">...</div>` 直後に置く):

```js
${geomSection}
${diffSection}
```

**Step 3: 動作確認(レポート生成 + オーバーレイ目視)**

```bash
cd tools && npm install
npm run compat -- bar
```
Expected: コンソール `PASS bar`(または geometry 差分)。`tools/report/bar.html` をブラウザで開き、fulgur と chart.js の PNG 上に正規化ボックスが棒に重なって表示されることを目視。badge に `geometry: PASS/FAIL` が出る。

**Step 4: JS テスト回帰**

Run: `cd tools && node --test chartjs-compat/*.test.mjs 2>&1 | tail -15`
Expected: 全 PASS。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/compat.mjs tools/chartjs-compat/report.mjs
git commit -m "feat(compat): HTML geometry overlay of normalized boxes on renders"
```

---

## Task 11d: エンドツーエンド検証 + フォローアップ issue

**Step 1: 全 spec で compat 実行**

```bash
cd tools && npm run compat 2>&1 | tail -25
```
Expected: 縦棒系(`bar`, `stacked-bar`)で geometry 次元が PASS(または既知の許容内差分)。横棒/line/scatter/bubble は geometry skip(両側 None)で従来通り。`pie`/`doughnut` も従来通り。

**Step 2: geometry 照合の有効性を手動確認(記録のみ・コミットしない)**

`vertical_bar_boxes` の `BAR_FILL_RATIO` を一時的に 0.5 に変える等で棒幅を変え、`npm run compat -- bar` が geometry の `elem[*].nw` 差分で FAIL することを確認 → 確認後 revert。これはツールの有効性検証でありコミットしない。

**Step 3: workspace 全体回帰**

```bash
cd <repoRoot> && cargo test 2>&1 | tail -15
```
Expected: 全 PASS。

**Step 4: scatter/line/bubble geometry のフォローアップ issue を作成**

```bash
bd create --title="compat tool: scatter/line/bubble の geometry 照合(Phase 2 続き)" \
  --type=task --priority=3 \
  --description="m80 で bar 先行実装した geometry 次元を散布図(点中心)・線(折れ点)・バブル(半径)へ拡張する。scatter は xs/ys.map による点正規化、bubble は半径も。chart.js 側は getDatasetMeta の PointElement {x,y} を chartArea 基準で正規化。diff.mjs の geometry 次元は要素 kind ごとに構造チェックを分岐(点は順序のみ、bar は高さ単調性)。fulgur-chart-m80 の design/プラン参照。"
```

**Step 5: コミット(プラン文書)**

```bash
git add docs/plans/2026-06-22-geometry-compat-phase2.md
git commit -m "docs: add geometry compat Phase 2 (bar-first) plan"
```

---

## 完了基準

- `cargo test`(workspace 全体)緑。`render_bar`/`render_stacked_bar`/`golden_png` スナップショット不変(矩形・PNG)。`inspect_model` bar スナップショットに妥当な geometry を固定。
- `cd tools && node --test chartjs-compat/*.test.mjs` 全 PASS。
- `npm run compat -- bar` / `stacked-bar` で geometry 次元 PASS(要素座標 + 構造で判定)、HTML に正規化ボックスのオーバーレイ表示。
- `plot_area` 差は info として記録・破線ボックスで可視化(pass には不参加)。
- 棒幅/高さを意図的に壊すと geometry が FAIL する(手動確認)。
- scatter/line/bubble は follow-up issue 化。

## 既知の制約 / フォローアップ

- 横棒(indexAxis:'y')・line・scatter・bubble の geometry は本タスク対象外(別 issue)。
- 積み上げラベルの y 座標は box 中心由来へ変更(値中点と数学的に等価、浮動小数末尾桁のみ差異の可能性)。
