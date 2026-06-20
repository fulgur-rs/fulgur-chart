# Progress Bar Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart.js v4 互換 JSON の `type:"progress"` を QuickChart `progressBar` 忠実なモデルで実装し、軸なしの水平塗りつぶしバー（角丸トラック + 前景 + 中央パーセンテージ）を決定的な SVG/PNG として描画する。

**Architecture:** 新しいフィールドなし `ChartKind::Progress` を追加。データは数値配列なので matrix のような専用パース経路は不要で、`frontend/chartjs.rs` のメインパスにそのまま乗る。レイアウトは軸を持たない `layout/progress.rs`（`matrix.rs` を範とする）を新設し、角丸矩形は `Prim::Path` を `fmt_num` で整形して決定的に生成する。`datasets[0].data` が各バーの値、任意の `datasets[1].data` が per-bar の max 上書き。

**Tech Stack:** Rust（workspace: `crates/fulgur-chart` コア + `crates/fulgur-chart-cli`）、serde / schemars、insta（スナップショット）、resvg/tiny-skia（PNG）。

**ビルド/テスト/lint コマンド（worktree 内で実行）:**
- テスト: `cargo test -p fulgur-chart`
- 全テスト: `cargo test --workspace`
- フォーマット: `cargo fmt --all`
- lint: `cargo clippy --workspace --all-targets -- -D warnings`

**設計上の確定事項（参照: beads issue fulgur-chart-s6j の design）:**
- `datasets[0].data` = 各バーの値（配列長 = バー本数 N）。任意の `datasets[1].data` = バーごとの max（省略時 100、非有限/≤0 は 100 フォールバック）。
- percentageᵢ = clamp(valueᵢ/maxᵢ×100, 0, 100)、塗り比率 = clamp(valueᵢ/maxᵢ, 0, 1)。
- バー名 = `data.labels[i]`。前景色 = `datasets[0].backgroundColor`、未指定はパレット[0]。
- 前景はソリッド: `fill_alpha = if is_pie || is_progress { 1.0 } else { 0.5 }`。
- トラック色 = 淡灰 `#e0e0e0`（定数）。
- パーセンテージはデフォルト表示、`options.plugins.datalabels.display:false` で非表示。
- 軸・グリッド・凡例は描画しない。

---

## Task 1: ChartKind::Progress 追加 + frontend パース + レイアウト雛形（end-to-end で描画が通る）

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`（`ChartKind` に `Progress` 追加）
- Modify: `crates/fulgur-chart/src/layout/mod.rs`（`pub mod progress;` と dispatch arm）
- Create: `crates/fulgur-chart/src/layout/progress.rs`（最小 `build`：タイトルのみ）
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`（`"progress"` → `ChartKind::Progress`）
- Test: `crates/fulgur-chart/tests/render_progress.rs`（新規）

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/render_progress.rs` を新規作成:

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn progress_renders_svg() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[70]}]}}"#);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"), "{svg}");
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_progress progress_renders_svg`
Expected: FAIL（`chartjs::parse` が `Err("未対応の type: progress")` で `.unwrap()` panic）

**Step 3: Write minimal implementation**

(a) `ir.rs` の `ChartKind` enum に variant を追加（`Matrix {...}` の後）:

```rust
    Matrix {
        color_lo: Color, // min 値のセル色（白固定）
        color_hi: Color, // max 値のセル色（backgroundColor 由来）
    },
    /// QuickChart 互換の progress バー。軸なし水平バー。
    /// series[0].values=各バーの値、series.get(1).values=per-bar max(省略時100)。
    Progress,
```

(b) `layout/progress.rs` を新規作成（雛形）:

```rust
//! progress チャートのレイアウト: ChartSpec → Scene。
//! 軸なしの水平塗りつぶしバー。決定的に組み立て、NaN/Inf/panic を出さない。

use super::common::{OUTER_PAD, TITLE_BAND, TITLE_FONT};
use crate::ir::ChartSpec;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let mut items: Vec<Prim> = Vec::new();

    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }
    let _ = TITLE_BAND; // 後続タスクで使用

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
```

(c) `layout/mod.rs` に module 宣言と dispatch を追加:

```rust
pub mod pie;
pub mod progress;   // ← 追加（アルファベット順で line と radar の間など適宜）
pub mod radar;
```

`build_scene` の match に arm を追加:

```rust
        ChartKind::Matrix { .. } => matrix::build(spec, m),
        ChartKind::Progress => progress::build(spec, m),
```

(d) `frontend/chartjs.rs` の kind 解決 match（`other => return Err(...)` の直前）に追加:

```rust
            "radar" => ChartKind::Radar,
            "progress" => ChartKind::Progress,
            other => return Err(format!("未対応の type: {other}")),
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_progress progress_renders_svg`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/layout/mod.rs \
        crates/fulgur-chart/src/layout/progress.rs crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/tests/render_progress.rs docs/plans/2026-06-20-progress-chart.md
git commit -m "feat(progress): add ChartKind::Progress scaffold and frontend parsing"
```

---

## Task 2: 角丸矩形ヘルパ `rounded_rect_path`（pure 関数、単体テスト）

**Files:**
- Modify: `crates/fulgur-chart/src/layout/progress.rs`（`rounded_rect_path` + `#[cfg(test)]`）

**Step 1: Write the failing test**

`progress.rs` の末尾に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rect_path_is_closed_and_clean() {
        let d = rounded_rect_path(10.0, 20.0, 100.0, 30.0, 15.0);
        assert!(d.starts_with('M'), "must start with moveto: {d}");
        assert!(d.ends_with('Z'), "must close: {d}");
        assert!(d.matches('A').count() == 4, "4 corner arcs: {d}");
        assert!(!d.contains("NaN") && !d.contains("inf"), "{d}");
    }

    #[test]
    fn rounded_rect_path_clamps_radius() {
        // 半径が w/2, h/2 を超えても破綻しない（幅 4 → r は 2 にクランプ）
        let d = rounded_rect_path(0.0, 0.0, 4.0, 30.0, 15.0);
        assert!(!d.contains("NaN") && d.ends_with('Z'), "{d}");
    }

    #[test]
    fn rounded_rect_path_deterministic() {
        let a = rounded_rect_path(1.0, 2.0, 50.0, 12.0, 6.0);
        let b = rounded_rect_path(1.0, 2.0, 50.0, 12.0, 6.0);
        assert_eq!(a, b);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart rounded_rect_path`
Expected: FAIL（`rounded_rect_path` 未定義のコンパイルエラー）

**Step 3: Write minimal implementation**

`progress.rs` の `build` の下（`#[cfg(test)]` の上）に追加。`use` に `fmt_num` を足す:

```rust
use crate::num::fmt_num;
```

```rust
/// 角丸矩形の SVG path data。半径は w/2, h/2, 0 でクランプして破綻を防ぐ。
/// すべての座標を `fmt_num` で整形し決定的に出力する（Prim::Path の d 規約に準拠）。
fn rounded_rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.max(0.0).min(w / 2.0).min(h / 2.0);
    let x1 = x + w;
    let y1 = y + h;
    format!(
        "M{} {}L{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}Z",
        fmt_num(x + r), fmt_num(y),
        fmt_num(x1 - r), fmt_num(y),
        fmt_num(r), fmt_num(r), fmt_num(x1), fmt_num(y + r),
        fmt_num(x1), fmt_num(y1 - r),
        fmt_num(r), fmt_num(r), fmt_num(x1 - r), fmt_num(y1),
        fmt_num(x + r), fmt_num(y1),
        fmt_num(r), fmt_num(r), fmt_num(x), fmt_num(y1 - r),
        fmt_num(x), fmt_num(y + r),
        fmt_num(r), fmt_num(r), fmt_num(x + r), fmt_num(y),
    )
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart rounded_rect_path`
Expected: PASS（3 tests）

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/progress.rs
git commit -m "feat(progress): add deterministic rounded_rect_path helper"
```

---

## Task 3: バー本体のレイアウト（角丸トラック + 前景 + クランプ + ソリッド前景色）

**Files:**
- Modify: `crates/fulgur-chart/src/layout/progress.rs`（`build` 本体）
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`（progress を `fill_alpha=1.0` に）
- Test: `crates/fulgur-chart/tests/render_progress.rs`

**Step 1: Write the failing test**

`render_progress.rs` に追加:

```rust
fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

#[test]
fn progress_two_bars_two_tracks_two_foregrounds() {
    // 値が 2 つ → トラック 2 + 前景 2 = path 4
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[70,40]}]}}"#);
    assert_eq!(count(&svg, "<path"), 4, "{svg}");
}

#[test]
fn progress_zero_value_is_track_only() {
    // 0% は前景パスを描かない（トラックのみ → path 1）
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[0]}]}}"#);
    assert_eq!(count(&svg, "<path"), 1, "{svg}");
}

#[test]
fn progress_foreground_uses_solid_background_color() {
    // 前景色は backgroundColor、ソリッド（fill-opacity の半透明指定がない）
    let svg = render(
        r##"{"type":"progress","data":{"datasets":[{"data":[60],"backgroundColor":"#ff0000"}]}}"##,
    );
    assert!(svg.contains("#ff0000"), "foreground color missing: {svg}");
    assert!(!svg.contains("fill-opacity=\"0.5\""), "should be solid: {svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_progress`
Expected: FAIL（`progress_two_bars_...` は path 0、色テストも未描画で失敗）

**Step 3: Write minimal implementation**

(a) `frontend/chartjs.rs`：`fill_alpha` の行を progress も 1.0 にする。

```rust
            let is_progress = matches!(kind, ChartKind::Progress);
            let fill_alpha = if is_pie || is_progress { 1.0_f32 } else { 0.5_f32 };
```

> 注: `kind` はこのクロージャの外で確定済み。`is_pie` と同様にクロージャ内で `matches!` を評価してよい（`kind` は move されない）。既存 `is_pie` がクロージャ内で使われていることを確認し、同じスコープに `is_progress` を置く。

(b) `progress.rs` の `build` を本実装に置き換え。定数と本体:

```rust
use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{ChartSpec, Color};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// トラック（背景）の淡灰色。
const TRACK_COLOR: Color = Color { r: 224, g: 224, b: 224, a: 1.0 };
/// バンド高に対するバー高の比。
const BAR_HEIGHT_RATIO: f64 = 0.6;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // series[0] が各バーの値。無ければ空。
    let values: &[f64] = spec
        .series
        .first()
        .map(|s| s.values.as_slice())
        .unwrap_or(&[]);
    let n = values.len();

    let title_band = if spec.title.is_some() { TITLE_BAND } else { 0.0 };

    // 左ラベル帯: バー名(categories)の最大幅。全て空なら 0。
    let mut max_label_w = 0.0_f32;
    for name in &spec.categories {
        if !name.is_empty() {
            let w = m.width(name, label_font as f32);
            if w > max_label_w {
                max_label_w = w;
            }
        }
    }
    let label_band = if max_label_w > 0.0 { max_label_w as f64 + 10.0 } else { 0.0 };

    let plot_left = OUTER_PAD + label_band;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD;
    let plot_w = (plot_right - plot_left).max(0.0);
    let plot_h = (plot_bottom - plot_top).max(0.0);

    let mut items: Vec<Prim> = Vec::new();

    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }

    if n == 0 {
        return Scene { width: spec.width, height: spec.height, items };
    }

    let band_h = plot_h / n as f64;
    let bar_h = (band_h * BAR_HEIGHT_RATIO).max(0.0);

    for i in 0..n {
        let band_top = plot_top + i as f64 * band_h;
        let bar_y = band_top + (band_h - bar_h) / 2.0;
        let center_y = band_top + band_h / 2.0;

        // per-bar max: series.get(1).values[i]。非有限/≤0 は 100。
        let max_i = spec
            .series
            .get(1)
            .and_then(|s| s.values.get(i).copied())
            .filter(|mx| mx.is_finite() && *mx > 0.0)
            .unwrap_or(100.0);

        let v = values[i];
        let frac = if v.is_finite() { (v / max_i).clamp(0.0, 1.0) } else { 0.0 };

        // トラック（角丸・全幅）
        let track_r = (bar_h / 2.0).min(plot_w / 2.0);
        items.push(Prim::Path {
            d: rounded_rect_path(plot_left, bar_y, plot_w, bar_h, track_r),
            fill: Some(TRACK_COLOR),
            stroke: None,
            stroke_width: 0.0,
        });

        // 前景（角丸・幅 = frac × 全幅）。0 幅は描かない。
        let fg_w = plot_w * frac;
        if fg_w > 0.0 {
            let fg_r = (bar_h / 2.0).min(fg_w / 2.0);
            items.push(Prim::Path {
                d: rounded_rect_path(plot_left, bar_y, fg_w, bar_h, fg_r),
                fill: Some(spec.series[0].fill_at(i)),
                stroke: None,
                stroke_width: 0.0,
            });
        }

        let _ = (center_y, fmt_num as fn(f64) -> String); // 次タスクで使用
    }

    Scene { width: spec.width, height: spec.height, items }
}
```

> 注: `let _ = (center_y, ...)` の行は未使用変数警告回避の暫定。Task 4 で `center_y` をラベルに使うので削除する。`fmt_num` は `rounded_rect_path` 内で使うため import 済み。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_progress`
Expected: PASS（`progress_renders_svg`, `progress_two_bars_...`, `progress_zero_value_...`, `progress_foreground_...`）

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/progress.rs crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/tests/render_progress.rs
git commit -m "feat(progress): render rounded track + solid foreground bars with clamping"
```

---

## Task 4: パーセンテージラベル（デフォルト ON、`datalabels.display:false` で OFF）

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`（progress の `data_labels` 既定 ON）
- Modify: `crates/fulgur-chart/src/layout/progress.rs`（中央ラベル描画）
- Test: `crates/fulgur-chart/tests/render_progress.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn progress_shows_percentage_by_default() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[70]}]}}"#);
    assert!(svg.contains(">70%<"), "percentage label missing: {svg}");
}

#[test]
fn progress_datalabels_display_false_hides_percentage() {
    let svg = render(
        r#"{"type":"progress","data":{"datasets":[{"data":[70]}],
        "options":{"plugins":{"datalabels":{"display":false}}}}"#,
    );
    assert!(!svg.contains('%'), "percentage should be hidden: {svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_progress progress_shows_percentage_by_default`
Expected: FAIL（ラベル未描画）

**Step 3: Write minimal implementation**

(a) `frontend/chartjs.rs` の `data_labels` 計算を progress 既定 ON に変更:

```rust
    // datalabels: 既存は「キーが存在し display!=false なら有効」。
    // progress のみ既定 ON（QuickChart 準拠）。明示 display:false は尊重する。
    let data_labels = match (&raw.options.plugins.datalabels, &kind) {
        (Some(dl), _) => dl.display != Some(false),
        (None, ChartKind::Progress) => true,
        (None, _) => false,
    };
```

(b) `progress.rs` のループ内、`let _ = (center_y, ...)` の暫定行を削除し、ラベル描画を追加:

```rust
        // パーセンテージ（バー中央・整数%に丸め）。
        if spec.data_labels {
            let pct = frac * 100.0;
            items.push(Prim::Text {
                x: plot_left + plot_w / 2.0,
                y: center_y + label_font * TEXT_BASELINE_RATIO,
                size: label_font,
                anchor: Anchor::Middle,
                fill: ink,
                content: format!("{}%", fmt_num(pct.round())),
            });
        }
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_progress`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/src/layout/progress.rs \
        crates/fulgur-chart/tests/render_progress.rs
git commit -m "feat(progress): center percentage label, on by default, toggleable via datalabels"
```

---

## Task 5: バー名（`data.labels`）+ per-bar max 上書き（2つ目 dataset）

**Files:**
- Modify: `crates/fulgur-chart/src/layout/progress.rs`（左ラベル描画）
- Test: `crates/fulgur-chart/tests/render_progress.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn progress_renders_bar_names_from_labels() {
    let svg = render(
        r#"{"type":"progress","data":{"labels":["CPU","RAM"],"datasets":[{"data":[30,80]}]}}"#,
    );
    assert!(svg.contains(">CPU<"), "bar name CPU missing: {svg}");
    assert!(svg.contains(">RAM<"), "bar name RAM missing: {svg}");
}

#[test]
fn progress_second_dataset_overrides_max() {
    // 15 / 30 = 50%
    let svg = render(
        r#"{"type":"progress","data":{"datasets":[{"data":[15]},{"data":[30]}]}}"#,
    );
    assert!(svg.contains(">50%<"), "expected 50%: {svg}");
}

#[test]
fn progress_clamps_over_max_to_100() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[150]}]}}"#);
    assert!(svg.contains(">100%<"), "expected clamp to 100%: {svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_progress progress_renders_bar_names_from_labels`
Expected: FAIL（左ラベル未描画）。max 上書き・クランプは Task 3 のロジックで既に通る可能性があるが、名前テストは必ず失敗する。

**Step 3: Write minimal implementation**

`progress.rs` のループ内、トラック描画の前に左ラベルを追加:

```rust
        // バー名（左・右寄せ）。categories[i] があり非空のときのみ。
        if let Some(name) = spec.categories.get(i) {
            if !name.is_empty() {
                items.push(Prim::Text {
                    x: plot_left - 6.0,
                    y: center_y + label_font * TEXT_BASELINE_RATIO,
                    size: label_font,
                    anchor: Anchor::End,
                    fill: ink,
                    content: name.clone(),
                });
            }
        }
```

> max 上書きとクランプは Task 3 の `max_i` / `frac` 実装で既にカバー済み。テストで明示的に固定する。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_progress`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/progress.rs crates/fulgur-chart/tests/render_progress.rs
git commit -m "feat(progress): left bar names from labels and per-bar max override"
```

---

## Task 6: エッジケース（空・決定性）+ スナップショット

**Files:**
- Test: `crates/fulgur-chart/tests/render_progress.rs`
- Create（テスト初回実行で生成）: `crates/fulgur-chart/tests/snapshots/render_progress__progress_snapshot.snap`

**Step 1: Write the failing test**

```rust
#[test]
fn progress_empty_data_no_panic() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[{"data":[]}]}}"#);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"), "{svg}");
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}

#[test]
fn progress_no_datasets_no_panic() {
    let svg = render(r#"{"type":"progress","data":{"datasets":[]}}"#);
    assert!(svg.starts_with("<svg"), "{svg}");
}

#[test]
fn progress_deterministic() {
    let j = r##"{"type":"progress","data":{"labels":["A","B"],
        "datasets":[{"data":[25,90],"backgroundColor":["#36a2eb","#ff6384"]}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn progress_snapshot() {
    let svg = render(
        r##"{"type":"progress","data":{"labels":["CPU","Memory","Disk"],
        "datasets":[{"data":[30,72,95],"backgroundColor":"#36a2eb"}]},
        "options":{"plugins":{"title":{"display":true,"text":"System Usage"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_progress`
Expected: 空/決定性は PASS のはず。`progress_snapshot` は新規スナップショットで FAIL（保留中）。

**Step 3: Write minimal implementation / accept snapshot**

実装変更は不要（空データは Task 3 の `n == 0` 早期 return でカバー）。スナップショットを確認して受理:

```bash
cargo insta review   # 内容を目視確認して accept
# もしくは内容に問題なければ:
INSTA_UPDATE=always cargo test -p fulgur-chart --test render_progress progress_snapshot
```

生成された `.snap` に `NaN`/`inf` が無いこと、3 本のトラック+前景+ラベルが含まれることを目視確認する。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_progress`
Expected: PASS（全 progress テスト）

**Step 5: Commit**

```bash
git add crates/fulgur-chart/tests/render_progress.rs \
        crates/fulgur-chart/tests/snapshots/render_progress__progress_snapshot.snap
git commit -m "test(progress): edge cases, determinism, and snapshot"
```

---

## Task 7: JSON Schema に `Progress` バリアント追加

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`（`ChartJsSpec` enum + 型）
- Test: `crates/fulgur-chart-cli/tests/cli.rs`（既存 `schema_chartjs_is_valid_json` が緑のまま）

**Step 1: Write the failing test**

`crates/fulgur-chart-cli/tests/cli.rs` に追加（schema 出力に progress が含まれることを確認）:

```rust
#[test]
fn schema_chartjs_includes_progress() {
    let out = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("progress"), "schema should mention progress variant");
}
```

> 既存テストの `Command::cargo_bin` 等のヘルパ/インポートに合わせること（ファイル冒頭の `use` を踏襲）。

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart-cli --test cli schema_chartjs_includes_progress`
Expected: FAIL（schema に progress 無し）

**Step 3: Write minimal implementation**

(a) `schema/chartjs.rs` の `ChartJsSpec` enum に variant 追加:

```rust
    Matrix(MatrixSpec),
    Progress(ProgressSpec),
}
```

(b) Matrix セクションの後に型を追加:

```rust
// ────────────────────────────────────────────────
// Progress bar chart (QuickChart-compatible)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressSpec {
    pub data: ProgressData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ProgressOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressData {
    /// Per-bar names, one per value in the first dataset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// First dataset = per-bar values; optional second dataset = per-bar max (default 100).
    pub datasets: Vec<ProgressDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ScalarOrArray<ColorString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart-cli --test cli schema_chartjs`
Expected: PASS（`schema_chartjs_is_valid_json` と `schema_chartjs_includes_progress` 両方）

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(progress): add Progress variant to JSON schema"
```

---

## Task 8: example spec + README + CHANGELOG + 最終品質ゲート

**Files:**
- Create: `examples/specs/progress.json`
- Modify: `README.md`（Supported chart types / subset の type 一覧）
- Modify: `CHANGELOG.md`

**Step 1: example spec を作成**

`examples/specs/progress.json`:

```json
{
  "type": "progress",
  "data": {
    "labels": ["CPU", "Memory", "Disk"],
    "datasets": [
      { "data": [30, 72, 95], "backgroundColor": "#36a2eb" }
    ]
  },
  "options": {
    "plugins": { "title": { "display": true, "text": "System Usage" } }
  }
}
```

CLI で描画して目視確認:

```bash
cargo run -p fulgur-chart-cli -- render examples/specs/progress.json -o /tmp/progress.svg
head -c 200 /tmp/progress.svg
```

Expected: `<svg ...>` で始まり、3 本のバー + パーセンテージが含まれる。

**Step 2: README 更新**

`README.md` の「Supported chart types」リストに追加:

```markdown
- Mixed chart (per-dataset `type`, e.g. bar + line)
- Progress bar chart (QuickChart-style; horizontal fill bar with centered percentage)
```

「Supported chart.js subset」の type 行に `progress` を追記:

```markdown
- `type` — `bar` / `line` / `pie` / `doughnut` / `scatter` / `bubble` / `radar` / `progress`
```

（必要なら progress の説明を 1〜2 行追記: `datasets[0].data` が各バーの値、任意の 2 つ目 dataset が per-bar max。）

**Step 3: CHANGELOG 更新**

`CHANGELOG.md` の最新節（Unreleased 等）に追加:

```markdown
- Add `progress` chart type (QuickChart-compatible progress bar): rounded track + solid
  foreground, centered percentage, optional second dataset for per-bar max override.
```

**Step 4: 最終品質ゲート（全部グリーンであること）**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: fmt 差分なし、clippy 警告ゼロ、全テスト PASS。

**Step 5: Commit**

```bash
git add examples/specs/progress.json README.md CHANGELOG.md
git commit -m "docs(progress): add example spec, README and CHANGELOG entries"
```

---

## 完了基準（acceptance、issue fulgur-chart-s6j より）

- `type:"progress"` が SVG/PNG で描画でき、`datasets[0].data` 長 N 本のバーが角丸トラック+前景の2層で描画される。
- 任意の2つ目 dataset で per-bar max を上書きでき、percentage = value/max×100、ラベルは "X%" を中央表示。
- `options.plugins.datalabels.display:false` でパーセンテージ非表示にできる。
- 前景はソリッド（alpha=1.0）、トラックは淡灰。
- 空データ・範囲外（<0, >max）で panic せず、出力に `NaN`/`inf` を含めない。
- 同一入力で byte 一致（決定性）。
- `cargo test` / `fmt` / `clippy` がグリーン、README・CHANGELOG 更新済み。

## 留意点
- `ChartKind` への variant 追加で網羅 match は `layout/mod.rs::build_scene` のみ（他は `_` で吸収）。コンパイルエラーが出たら指示通り arm を追加。
- パーセンテージラベルは `text_color`(#666) を中央に置くため中〜高% では色付き前景に乗りコントラストがやや弱い（v1 許容）。
- 非有限値は JSON 入力からは発生しないが、レイアウトは `frac=0` で防御済み。
