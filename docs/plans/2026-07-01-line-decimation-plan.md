# Line Decimation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Chart.js 互換の `options.plugins.decimation`（min-max / lttb）を実装し、巨大 line/area を threshold 超過時に自動間引き＋マーカー抑制して SVG・PNG 両方を高速化する。

**Architecture:** 間引きはデータ段（`src/layout/line.rs` の `build()` 内、**gap セグメント分割の直後**）で**セグメント単位**に行う。新モジュール `src/layout/decimate.rs` に純粋なアルゴリズム関数（min-max / lttb）と発動判定（`resolve`）・単一セグメント間引き（`decimate_one`）を置き、`build()` から呼ぶ。間引きは論理ピクセル空間（`frame.plot_left/right` 由来）で行い、SVG（`svg.rs`）と PNG（`raster_direct.rs`）が同一 Scene を消費するため両出力が一致・決定的。既定は自動オン（`enabled=true`）だが threshold ゲートにより小チャートは今日とバイト不変。**間引き後に cat で再分割しないこと**（cat が非連続になり線が消える — Task 7 参照）。

**Tech Stack:** Rust、`schemars`（JSON schema 生成）、`serde`（JSON parse）、`insta`（スナップショット）、`cargo test`。

**Working dir:** `/home/ubuntu/fulgur-chart/.worktrees/line-decimation`（ブランチ `feat/line-decimation`）。全コマンドはこの worktree 内で実行する。

**設計根拠:** `docs/plans/2026-07-01-line-decimation.md` 参照。

---

## 前提・既知の事実（調査済み）

- 間引き挿入点: `src/layout/line.rs` `build()`、**segments 構築（62-77行）の直後**（gap 分割後・cat 保持）。発動判定は系列全体 `valid.len()` で行い、各セグメントを間引いてから flatten して `valid` を差し替える。`valid` は area(80-106)・markers(130-140)・labels(144-155) の供給源、segments は line(109-128) の供給源。
- `Frame.plot_left/plot_right`（`src/layout/common.rs` 35-42）は**論理座標**（`compute()` は DPR を取らない）。論理プロット幅 = `plot_right - plot_left`。
- マーカー描画は `build()` の `Prim::Circle` ループ（130-140）。`line_points()`（15-36）は `model.rs:191` の inspect モデル専用で、本計画では小データ（≤7点・threshold 未満）のみ通るため**変更不要**（スコープ外、フォローアップ）。
- strict parser: line は汎用 `check_unknown_keys`（`src/frontend/chartjs.rs` 836-952）を通る。`options.plugins` 許可キー = 891-897行（`title/legend/datalabels[/outlabels]`）。
- schema: line は `CommonPlugins`（`src/schema/chartjs.rs` 217-226）、bar は `BarPlugins`（129-138）。プラグイン構造体は `src/schema/common.rs`（`DataLabelsPlugin` 63-68 等）。汎用 check は line/bar/scatter/bubble/pie/radar を通すため、parity 維持に **decimation を `CommonPlugins` と `BarPlugins` の両方**へ追加する。
- tunable 定数パターン: `src/raster_direct.rs` 225/231（`/// doc` + `const NAME: T = lit;`）。
- 既存 line テスト/golden は全て ≤7点（`render_line__*`・`golden/line.png`・`inspect_model`）。bench のみ `line_large`(10000)・`line_small`(12)。`bench_cases.rs` の assertion は JSON バイト長のみ（描画点数は検査せず）。threshold（≈プロット幅×4 ≈ 数百〜2000超）なら既存テストは全て不変。
- 間引き対象は **`ChartKind::Line` の `line::build` のみ**。mixed（line+bar）はスコープ外（フォローアップ）。

---

## Task 0: 設計ドキュメント + 本計画をコミット

**Step 1: worktree の状態確認**

Run: `cd /home/ubuntu/fulgur-chart/.worktrees/line-decimation && git status --short`
Expected: `?? docs/plans/2026-07-01-line-decimation.md` と `?? docs/plans/2026-07-01-line-decimation-plan.md`

**Step 2: コミット**

```bash
git add docs/plans/2026-07-01-line-decimation.md docs/plans/2026-07-01-line-decimation-plan.md
git commit -m "docs(line-decimation): 設計合意と実装計画を追加 (fulgur-chart-43h)"
```

---

## Task 1: min-max アルゴリズム（純粋関数・TDD）

**Files:**
- Create: `crates/fulgur-chart/src/layout/decimate.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`（`mod decimate;` 追加）

**Step 1: モジュール登録**

`src/layout/mod.rs` の他 `mod` 宣言群に `mod decimate;` を追加（`pub` 不要、クレート内利用）。

**Step 2: 失敗するテストを書く**

`src/layout/decimate.rs` に作成:

```rust
//! line/area 用デシメーション（Chart.js options.plugins.decimation 互換）。
//! 論理ピクセル空間の点列 (x, y, cat) を間引く。x は index 単調を前提とする。

/// 列ごと min/max デシメーション。floor(x) を列キーにバケツ化し、各占有列で
/// start / min / max / end の最大4点を index 順に残す（Chart.js minMaxDecimation 準拠）。
/// 簡略化: Chart.js は min/max を列平均x に置くが、本実装は元 x を保つ（同一列内なのでサブピクセル差）。
pub fn min_max(points: &[(f64, f64, usize)]) -> Vec<(f64, f64, usize)> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_max_reduces_dense_columns_and_preserves_extremes() {
        // 2 列（x=0.x と x=1.x）に各5点。各列の min/max y が残ること。
        let pts: Vec<(f64, f64, usize)> = vec![
            (0.0, 5.0, 0), (0.2, 1.0, 1), (0.4, 9.0, 2), (0.6, 3.0, 3), (0.8, 7.0, 4),
            (1.0, 2.0, 5), (1.2, 8.0, 6), (1.4, 0.0, 7), (1.6, 6.0, 8), (1.8, 4.0, 9),
        ];
        let out = min_max(&pts);
        // 10点 → 各列最大4点 = 最大8点に削減
        assert!(out.len() < pts.len());
        // 列0の極値 y=9.0(idx2) と y=1.0(idx1) が含まれる
        assert!(out.iter().any(|p| p.2 == 2));
        assert!(out.iter().any(|p| p.2 == 1));
        // 列1の極値 y=8.0(idx6) と y=0.0(idx7) が含まれる
        assert!(out.iter().any(|p| p.2 == 6));
        assert!(out.iter().any(|p| p.2 == 7));
    }

    #[test]
    fn min_max_x_is_monotonic_nondecreasing() {
        let pts: Vec<(f64, f64, usize)> =
            (0..50).map(|i| (i as f64 * 0.1, ((i * 7) % 13) as f64, i)).collect();
        let out = min_max(&pts);
        for w in out.windows(2) {
            assert!(w[1].0 >= w[0].0, "x must be monotonic non-decreasing");
        }
    }

    #[test]
    fn min_max_passthrough_when_tiny() {
        let pts = vec![(0.0, 1.0, 0), (1.0, 2.0, 1)];
        assert_eq!(min_max(&pts), pts);
    }

    #[test]
    fn min_max_is_deterministic() {
        let pts: Vec<(f64, f64, usize)> =
            (0..200).map(|i| (i as f64 * 0.05, (i % 17) as f64, i)).collect();
        assert_eq!(min_max(&pts), min_max(&pts));
    }
}
```

**Step 3: テストが失敗することを確認**

Run: `cargo test -p fulgur-chart decimate:: 2>&1 | tail -20`
Expected: FAIL（`unimplemented!` で panic）

**Step 4: 最小実装**

```rust
pub fn min_max(points: &[(f64, f64, usize)]) -> Vec<(f64, f64, usize)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out: Vec<(f64, f64, usize)> = Vec::new();
    let mut push_unique = |out: &mut Vec<(f64, f64, usize)>, p: (f64, f64, usize)| {
        if out.last().map(|l| l.2) != Some(p.2) {
            out.push(p);
        }
    };
    let flush = |out: &mut Vec<(f64, f64, usize)>,
                 start: usize,
                 end: usize,
                 pts: &[(f64, f64, usize)],
                 push_unique: &mut dyn FnMut(&mut Vec<(f64, f64, usize)>, (f64, f64, usize))| {
        push_unique(out, pts[start]);
        let (mut min_i, mut max_i) = (start, start);
        for k in start..=end {
            if pts[k].1 < pts[min_i].1 {
                min_i = k;
            }
            if pts[k].1 > pts[max_i].1 {
                max_i = k;
            }
        }
        let (i1, i2) = (min_i.min(max_i), min_i.max(max_i));
        push_unique(out, pts[i1]);
        push_unique(out, pts[i2]);
        push_unique(out, pts[end]);
    };
    let mut col_start = 0usize;
    let mut prev_col = points[0].0.floor() as i64;
    for i in 1..points.len() {
        let col = points[i].0.floor() as i64;
        if col != prev_col {
            flush(&mut out, col_start, i - 1, points, &mut push_unique);
            col_start = i;
            prev_col = col;
        }
    }
    flush(&mut out, col_start, points.len() - 1, points, &mut push_unique);
    out
}
```

（クロージャの借用で詰まる場合は `flush` を通常の `fn` に切り出してよい。挙動が同じなら実装形は自由。）

**Step 5: テストが通ることを確認**

Run: `cargo test -p fulgur-chart decimate:: 2>&1 | tail -20`
Expected: PASS（4 tests）

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/layout/decimate.rs crates/fulgur-chart/src/layout/mod.rs
git commit -m "feat(decimate): 列ごと min/max デシメーションを追加"
```

---

## Task 2: lttb アルゴリズム（純粋関数・TDD）

**Files:**
- Modify: `crates/fulgur-chart/src/layout/decimate.rs`

**Step 1: 失敗するテストを書く**（`tests` モジュールに追記）

```rust
#[test]
fn lttb_hits_target_sample_count() {
    let pts: Vec<(f64, f64, usize)> =
        (0..1000).map(|i| (i as f64, ((i * 31) % 97) as f64, i)).collect();
    let out = lttb(&pts, 100);
    assert_eq!(out.len(), 100);
}

#[test]
fn lttb_keeps_first_and_last() {
    let pts: Vec<(f64, f64, usize)> =
        (0..500).map(|i| (i as f64, (i % 11) as f64, i)).collect();
    let out = lttb(&pts, 50);
    assert_eq!(out.first().unwrap().2, 0);
    assert_eq!(out.last().unwrap().2, 499);
}

#[test]
fn lttb_passthrough_when_count_le_samples() {
    let pts: Vec<(f64, f64, usize)> = (0..30).map(|i| (i as f64, 1.0, i)).collect();
    assert_eq!(lttb(&pts, 50), pts);
}

#[test]
fn lttb_is_deterministic() {
    let pts: Vec<(f64, f64, usize)> =
        (0..800).map(|i| (i as f64, ((i * 13) % 29) as f64, i)).collect();
    assert_eq!(lttb(&pts, 80), lttb(&pts, 80));
}
```

**Step 2: テスト失敗を確認**

Run: `cargo test -p fulgur-chart decimate::tests::lttb 2>&1 | tail -20`
Expected: FAIL（`lttb` 未定義 → コンパイルエラー）

**Step 3: 最小実装**（`min_max` の後に追記）

```rust
/// LTTB (Largest Triangle Three Buckets)。視覚形状を保ちつつ samples 点へ間引く。
/// 三角形面積は論理ピクセル空間で計算するため視覚的に正しい。count <= samples なら原データ返却。
pub fn lttb(points: &[(f64, f64, usize)], samples: usize) -> Vec<(f64, f64, usize)> {
    let n = points.len();
    if samples < 3 || n <= samples {
        return points.to_vec();
    }
    let mut out: Vec<(f64, f64, usize)> = Vec::with_capacity(samples);
    let bucket_width = (n - 2) as f64 / (samples - 2) as f64;
    out.push(points[0]);
    let mut a = 0usize; // 直前に採用した点の index
    for i in 0..(samples - 2) {
        // 次バケツの平均点（三角形の第3点）
        let mut avg_start = ((i + 1) as f64 * bucket_width).floor() as usize + 1;
        let mut avg_end = ((i + 2) as f64 * bucket_width).floor() as usize + 1;
        avg_start = avg_start.min(n - 1);
        avg_end = avg_end.min(n);
        if avg_end <= avg_start {
            avg_end = avg_start + 1;
        }
        let mut avg_x = 0.0;
        let mut avg_y = 0.0;
        for j in avg_start..avg_end {
            avg_x += points[j].0;
            avg_y += points[j].1;
        }
        let cnt = (avg_end - avg_start) as f64;
        avg_x /= cnt;
        avg_y /= cnt;
        // 候補バケツ
        let range_start = (i as f64 * bucket_width).floor() as usize + 1;
        let range_end = ((i + 1) as f64 * bucket_width).floor() as usize + 1;
        let (ax, ay) = (points[a].0, points[a].1);
        let mut max_area = -1.0_f64;
        let mut next_a = range_start.min(n - 1);
        for j in range_start..range_end.min(n) {
            let area = ((ax - avg_x) * (points[j].1 - ay)
                - (ax - points[j].0) * (avg_y - ay))
                .abs()
                * 0.5;
            if area > max_area {
                max_area = area;
                next_a = j;
            }
        }
        out.push(points[next_a]);
        a = next_a;
    }
    out.push(points[n - 1]);
    out
}
```

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart decimate::tests::lttb 2>&1 | tail -20`
Expected: PASS（4 tests）

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/decimate.rs
git commit -m "feat(decimate): LTTB デシメーションを追加"
```

---

## Task 3: 設定型 + threshold 判定 + セグメント間引き dispatcher

> **重要（advisor 指摘の再構成）:** gap 分割は `build()` 側で**先に**行い、間引きは
> **セグメント単位**で適用する。間引きは cat を非連続にするため、間引き後に cat で再分割
> すると全点が gap 扱いになり線が消える。よって本 Task は「間引き発動判定 `resolve`」と
> 「単一セグメント間引き `decimate_one`」のみを提供し、gap 分割は持たない（Task 7 が担う）。

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`（`Decimation` / `DecimationAlgorithm` 型追加）
- Modify: `crates/fulgur-chart/src/layout/decimate.rs`（`resolve` / `decimate_one` 追加）

**Step 1: IR 型を追加**（`src/ir.rs`、`ChartSpec` 定義の近く）

```rust
/// デシメーションアルゴリズム（Chart.js 互換）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimationAlgorithm {
    MinMax,
    Lttb,
}

/// options.plugins.decimation の解決済み設定。
/// 既定は自動オン（enabled=true）。Chart.js（false）からの意図的乖離。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decimation {
    pub enabled: bool,
    pub algorithm: DecimationAlgorithm,
    /// lttb の目標サンプル数。None なら論理プロット幅px。
    pub samples: Option<f64>,
    /// 間引き発動の点数しきい値。None なら論理プロット幅px × 4。
    pub threshold: Option<f64>,
}

impl Default for Decimation {
    fn default() -> Self {
        Decimation {
            enabled: true,
            algorithm: DecimationAlgorithm::MinMax,
            samples: None,
            threshold: None,
        }
    }
}
```

**Step 2: 失敗するテストを書く**（`src/layout/decimate.rs` tests）

```rust
#[test]
fn resolve_none_below_threshold() {
    use crate::ir::Decimation;
    let cfg = Decimation::default(); // enabled, min-max, threshold=None → width*4
    assert!(resolve(&cfg, 100.0, 50).is_none()); // width=100 → threshold=400 > 50
}

#[test]
fn resolve_some_above_threshold() {
    use crate::ir::Decimation;
    let cfg = Decimation::default();
    let got = resolve(&cfg, 100.0, 1000); // threshold=400 < 1000
    assert!(got.is_some());
}

#[test]
fn resolve_none_when_disabled() {
    use crate::ir::Decimation;
    let cfg = Decimation { enabled: false, ..Decimation::default() };
    assert!(resolve(&cfg, 100.0, 1000).is_none());
}

#[test]
fn decimate_one_dispatches_min_max() {
    use crate::ir::DecimationAlgorithm;
    let pts: Vec<(f64, f64, usize)> =
        (0..1000).map(|i| (i as f64 * 0.1, (i % 7) as f64, i)).collect();
    let out = decimate_one(&pts, DecimationAlgorithm::MinMax, 100);
    assert!(out.len() < pts.len());
}
```

**Step 3: テスト失敗を確認**

Run: `cargo test -p fulgur-chart decimate::tests::resolve decimate::tests::decimate_one 2>&1 | tail -20`
Expected: FAIL（`resolve` / `decimate_one` 未定義）

**Step 4: 最小実装**（`src/layout/decimate.rs` 冒頭付近に `use`、関数を追記）

```rust
use crate::ir::{Decimation, DecimationAlgorithm};

/// threshold 既定 = 論理プロット幅px × この係数（Chart.js 準拠）。
const DECIMATION_THRESHOLD_FACTOR: f64 = 4.0;

/// 間引きを発動すべきか判定。発動するなら (algorithm, samples) を返す。
/// enabled=false / threshold 未満なら None。判定は**系列全体**の点数で（Chart.js セマンティクス）。
/// 呼び出し側は gap 分割の前に全点数でこれを呼ぶこと（小セグメント乱立による発動漏れ防止）。
pub fn resolve(
    cfg: &Decimation,
    plot_width: f64,
    total_points: usize,
) -> Option<(DecimationAlgorithm, usize)> {
    if !cfg.enabled {
        return None;
    }
    let threshold = cfg
        .threshold
        .unwrap_or(plot_width.max(1.0) * DECIMATION_THRESHOLD_FACTOR);
    if (total_points as f64) <= threshold {
        return None;
    }
    let samples = cfg.samples.unwrap_or(plot_width.max(1.0)).max(3.0) as usize;
    Some((cfg.algorithm, samples))
}

/// 単一セグメント（gap を含まない連続点列）を間引く。gap 分割は呼び出し側の責務。
pub fn decimate_one(
    seg: &[(f64, f64, usize)],
    algo: DecimationAlgorithm,
    samples: usize,
) -> Vec<(f64, f64, usize)> {
    match algo {
        DecimationAlgorithm::MinMax => min_max(seg),
        DecimationAlgorithm::Lttb => lttb(seg, samples),
    }
}
```

**Step 5: テスト通過を確認**

Run: `cargo test -p fulgur-chart decimate::tests::resolve decimate::tests::decimate_one 2>&1 | tail -20`
Expected: PASS（4 tests）

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/layout/decimate.rs
git commit -m "feat(decimate): 設定型と発動判定 resolve / 単一セグメント間引き decimate_one を追加"
```

---

## Task 4: フロントエンド parse（RawDecimation → IR Decimation）

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`（`RawPlugins`, 新 `RawDecimation`, `parse()` 解決, `ChartSpec` 構築）
- Modify: `crates/fulgur-chart/src/ir.rs`（`ChartSpec` に `decimation` フィールド追加）

**Step 1: 失敗するテストを書く**（`src/frontend/chartjs.rs` のテストモジュール、または `tests/frontend_chartjs.rs`）

```rust
#[test]
fn decimation_defaults_to_enabled_minmax_when_absent() {
    let spec = parse(r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#).unwrap();
    assert!(spec.decimation.enabled);
    assert_eq!(spec.decimation.algorithm, crate::ir::DecimationAlgorithm::MinMax);
}

#[test]
fn decimation_explicit_disable_and_lttb() {
    let json = r#"{"type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
        "options":{"plugins":{"decimation":{"enabled":false,"algorithm":"lttb","samples":300,"threshold":1000}}}}"#;
    let spec = parse(json).unwrap();
    assert!(!spec.decimation.enabled);
    assert_eq!(spec.decimation.algorithm, crate::ir::DecimationAlgorithm::Lttb);
    assert_eq!(spec.decimation.samples, Some(300.0));
    assert_eq!(spec.decimation.threshold, Some(1000.0));
}

#[test]
fn decimation_invalid_algorithm_errors() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"algorithm":"bogus"}}}}"#;
    assert!(parse(json).is_err());
}
```

（`parse` の正確な可視性・呼び出し名は既存テストに合わせる。`tests/frontend_chartjs.rs` の既存テストの呼び出し形を踏襲すること。）

**Step 2: テスト失敗を確認**

Run: `cargo test -p fulgur-chart decimation_ 2>&1 | tail -20`
Expected: FAIL（`spec.decimation` フィールド無し → コンパイルエラー）

**Step 3: 実装**

(a) `src/ir.rs` `ChartSpec`（333-348行）に追加:
```rust
    pub decimation: Decimation,
```

(b) `src/frontend/chartjs.rs` `RawPlugins`（67-73行）に追加:
```rust
    decimation: Option<RawDecimation>,
```

(c) `RawDecimation` 構造体を新規追加（`RawDataLabels` 75-79行の近く）:
```rust
#[derive(Deserialize)]
struct RawDecimation {
    enabled: Option<bool>,
    algorithm: Option<String>,
    samples: Option<f64>,
    threshold: Option<f64>,
}
```

(d) `parse()` 内、`data_labels` 解決（441-447行）の近くで decimation を解決:
```rust
let decimation = match &raw.options.plugins.decimation {
    Some(d) => {
        let algorithm = match d.algorithm.as_deref() {
            None | Some("min-max") => crate::ir::DecimationAlgorithm::MinMax,
            Some("lttb") => crate::ir::DecimationAlgorithm::Lttb,
            Some(other) => return Err(/* 既存エラー型に合わせる */ format!("unknown decimation algorithm: {other}").into()),
        };
        crate::ir::Decimation {
            enabled: d.enabled.unwrap_or(true),
            algorithm,
            samples: d.samples,
            threshold: d.threshold,
        }
    }
    None => crate::ir::Decimation::default(),
};
```
（エラー生成は既存 `check_unknown_keys` 等が使うエラー型・マクロに合わせる。`parse` の戻り値型を確認して整合させること。）

(e) `ChartSpec { ... }` 構築（646-681行）に `decimation,` を追加。

(f) **他の `ChartSpec` リテラル構築箇所**をコンパイラ任せに修正:
Run: `cargo build -p fulgur-chart 2>&1 | grep -n "missing field .decimation" | head`
で列挙される各箇所（`guard.rs`・各種テストビルダー等）に `decimation: crate::ir::Decimation::default(),` を追加。

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart decimation_ 2>&1 | tail -20`
Expected: PASS（3 tests）。続けて `cargo build -p fulgur-chart` がエラー無し。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/src/guard.rs
git commit -m "feat(decimate): chartjs frontend で decimation を parse し IR へ解決"
```

---

## Task 5: strict parser が decimation キーを許可

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`（`check_unknown_keys` 891-900行付近）

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn strict_accepts_decimation_keys() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"enabled":true,"algorithm":"min-max","samples":100,"threshold":500}}}}"#;
    assert!(parse(json).is_ok());
}

#[test]
fn strict_rejects_unknown_decimation_subkey() {
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"bogus":1}}}}"#;
    assert!(parse(json).is_err());
}
```

**Step 2: テスト失敗を確認**

Run: `cargo test -p fulgur-chart strict_..._decimation 2>&1 | tail -20`
Expected: `strict_accepts_decimation_keys` が FAIL（`decimation` が未許可キーで弾かれる）

**Step 3: 実装**（`check_unknown_keys` 891-897行）

`allowed_plugins` に `"decimation"` を追加:
```rust
let allowed_plugins: &[&str] = if allow_outlabels {
    &["title", "legend", "datalabels", "outlabels", "decimation"]
} else {
    &["title", "legend", "datalabels", "decimation"]
};
```
datalabels サブ check（898-900行）の直後に decimation サブ check を追加:
```rust
if let Some(dec) = plugins.get("decimation") {
    check_object(dec, &["enabled", "algorithm", "samples", "threshold"], "options.plugins.decimation")?;
}
```

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart strict_..._decimation 2>&1 | tail -20`
Expected: PASS（2 tests）

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs
git commit -m "feat(decimate): strict parser で options.plugins.decimation を許可"
```

---

## Task 6: JSON schema（DecimationPlugin）+ parity テスト

**Files:**
- Modify: `crates/fulgur-chart/src/schema/common.rs`（`DecimationPlugin` 追加）
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`（`CommonPlugins` 217-226 / `BarPlugins` 129-138 に field 追加）

**Step 1: 失敗するテストを書く**（parity: schema が受理する decimation を strict も受理）

```rust
#[test]
fn schema_strict_parity_decimation_line() {
    // schema 上 valid な decimation 付き line config が strict parse でも通ること。
    let json = r#"{"type":"line","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"plugins":{"decimation":{"enabled":true,"algorithm":"lttb","samples":50,"threshold":200}}}}"#;
    // strict 側
    assert!(crate::frontend::chartjs::parse(json).is_ok());
    // schema 側（既存の schema deserialize 経路に合わせる。例: LineSpec を serde_json で deserialize）
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    let line: crate::schema::chartjs::LineSpec = serde_json::from_value(v).unwrap();
    assert!(line.options.and_then(|o| o.plugins).and_then(|p| p.decimation).is_some());
}
```
（既存 schema テストの deserialize 形に合わせて調整。`LineSpec`/`LineOptions`/`CommonPlugins` の公開度を確認。）

**Step 2: テスト失敗を確認**

Run: `cargo test -p fulgur-chart parity_decimation 2>&1 | tail -20`
Expected: FAIL（`CommonPlugins` に `decimation` field 無し → コンパイルエラー）

**Step 3: 実装**

(a) `src/schema/common.rs`（`DataLabelsPlugin` 63-68 の近く）:
```rust
/// options.plugins.decimation（Chart.js 互換）。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct DecimationPlugin {
    pub enabled: Option<bool>,
    /// "min-max" | "lttb"
    pub algorithm: Option<String>,
    pub samples: Option<f64>,
    pub threshold: Option<f64>,
}
```

(b) `src/schema/chartjs.rs` `CommonPlugins`（217-226）に追加:
```rust
    pub decimation: Option<crate::schema::common::DecimationPlugin>,
```

(c) 同 `BarPlugins`（129-138）にも同じ field を追加（parity: 汎用 check_unknown_keys が bar も通すため）。

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart parity_decimation 2>&1 | tail -20`
Expected: PASS

**Step 5: schema スナップショットの更新（あれば）**

Run: `cargo test -p fulgur-chart 2>&1 | grep -i snapshot | tail`
schema を JSON 出力するスナップショット（例: `tests/snapshots/*schema*`）があれば `cargo insta review` で承認。無ければスキップ。

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/schema/common.rs crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "feat(decimate): JSON schema に DecimationPlugin を追加 (CommonPlugins/BarPlugins)"
```

---

## Task 7: line::build に間引きを配線（セグメント先行分割）

> **重要:** advisor 指摘の二重分割バグを避けるため、**gap 分割を先に行い（cat 保持）→
> 各セグメントを間引き → line はその間引き済みセグメントから直接描画 → area/marker/label
> 用に flatten** する。間引き後に cat で再分割してはならない（全点が gap 扱いになり線が消える）。

**Files:**
- Modify: `crates/fulgur-chart/src/layout/line.rs`（`build()` の segments 構築 62-77 と
  area/line/marker/label の供給を再構成）

**Step 1: 失敗するテストを書く**（`tests/render_line.rs`）

堅牢な比較: 「既定（自動オン）」と「enabled:false」の Polyline 総点数を比べ、前者が大幅に
少ないこと（プロット幅依存の固定しきい値に頼らない）。

```rust
fn polyline_pts(json: &str) -> usize {
    let spec = crate::frontend::chartjs::parse(json).unwrap();
    let m = /* 既存テストの TextMeasurer 生成に合わせる */;
    let scene = crate::layout::line::build(&spec, &m);
    scene.items.iter().filter_map(|p| match p {
        crate::scene::Prim::Polyline { points, .. } => Some(points.len()),
        _ => None,
    }).sum()
}

#[test]
fn large_line_is_decimated_vs_disabled() {
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    let body = format!("\"labels\":[{}],\"datasets\":[{{\"data\":[{}]}}]", labels.join(","), data.join(","));
    let on = format!(r#"{{"type":"line","data":{{{body}}}}}"#);
    let off = format!(r#"{{"type":"line","data":{{{body}}},"options":{{"plugins":{{"decimation":{{"enabled":false}}}}}}}}"#);
    let on_pts = polyline_pts(&on);
    let off_pts = polyline_pts(&off);
    assert_eq!(off_pts, n, "disabled must keep all points (single segment)");
    assert!(on_pts > 0 && on_pts < off_pts, "default must decimate: {on_pts} vs {off_pts}");
}

#[test]
fn small_line_polyline_unchanged() {
    // 3点 line → 間引きされず 3点のまま（既存 golden と整合）。
    let pts = polyline_pts(r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#);
    assert_eq!(pts, 3);
}
```
（`TextMeasurer` 生成・`Scene.items` の正確な field 名は既存 `tests/render_line.rs` を踏襲。）

**Step 2: テスト失敗を確認**

Run: `cargo test -p fulgur-chart large_line_is_decimated_vs_disabled 2>&1 | tail -20`
Expected: FAIL（間引き未配線 → on_pts == off_pts == n）

**Step 3: 実装**（`build()` の `for ser in &spec.series` ループ内を再構成）

(a) segments を **cat 保持**に変更（62-77 行。`Vec<Vec<(f64,f64)>>` → `Vec<Vec<(f64,f64,usize)>>`、
`cur.push((x, y))` → `cur.push((x, y, cat))`）。元の `for &(x, y, cat) in &valid` はそのまま使える。

(b) segments 構築の**直後**に、発動判定→各セグメント間引き→`valid` 再構築を挿入:
```rust
            // デシメーション判定は系列全体の点数で（gap 分割の前後で一貫）。
            let plot_width = frame.plot_right - frame.plot_left;
            let dec = crate::layout::decimate::resolve(&spec.decimation, plot_width, valid.len());
            let decimated = dec.is_some();
            let segments: Vec<Vec<(f64, f64, usize)>> = if let Some((algo, samples)) = dec {
                segments
                    .iter()
                    .map(|s| crate::layout::decimate::decimate_one(s, algo, samples))
                    .collect()
            } else {
                segments
            };
            // area/marker/label 用に間引き後の点列へ差し替え（Chart.js dataset.data 差し替えモデル）。
            let valid: Vec<(f64, f64, usize)> = segments.iter().flatten().copied().collect();
```

(c) area（80-106）・markers（130-140）・labels（144-155）は上記 shadow した `valid` を使う
（コード変更不要。順序上 area より前にこのブロックが来るよう、segments 構築の直後＝area の前に置く）。

(d) line 描画（109-128）は segments をイテレートするが、`seg` は `(f64,f64,usize)` になったので
Polyline / catmull_rom_path には x,y のみ渡す:
```rust
        for seg in &segments {
            if seg.len() < 2 {
                continue;
            }
            let xy: Vec<(f64, f64)> = seg.iter().map(|&(x, y, _)| (x, y)).collect();
            if ser.tension <= 0.0 {
                items.push(Prim::Polyline { points: xy, stroke: ser.stroke_at(0), stroke_width: ser.stroke_width });
            } else {
                let d = catmull_rom_path(&xy, ser.tension);
                items.push(Prim::Path { d, fill: None, stroke: Some(ser.stroke_at(0)), stroke_width: ser.stroke_width });
            }
        }
```

注意: `valid` は元 immutable。shadow で問題なし。`decimated: bool` は Task 8 で使う。

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart large_line_is_decimated_vs_disabled small_line_polyline_unchanged 2>&1 | tail -20`
Expected: PASS（2 tests）

**Step 5: 既存 line テストが壊れていないことを確認**

Run: `cargo test -p fulgur-chart --test render_line --test inspect_model --test golden_png 2>&1 | tail -20`
Expected: 全 PASS（小データは threshold 未満で不変）。万一 golden_png が差分を出したら **間引きが小データに誤発動**しているので threshold 実装を見直す（修正前に commit しない）。

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/layout/line.rs crates/fulgur-chart/tests/render_line.rs
git commit -m "feat(decimate): line::build に間引きを配線 (line/area/label)"
```

---

## Task 8: threshold 超過でマーカー自動抑制

**Files:**
- Modify: `crates/fulgur-chart/src/layout/line.rs`（marker ループ 130-140行）

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn large_line_suppresses_markers_by_default() {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", i % 50)).collect();
    let json = format!(r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","), data.join(","));
    let spec = crate::frontend::chartjs::parse(&json).unwrap();
    let m = /* TextMeasurer */;
    let scene = crate::layout::line::build(&spec, &m);
    let markers = scene.items.iter().filter(|p| matches!(p, crate::scene::Prim::Circle { .. })).count();
    assert_eq!(markers, 0, "large line should suppress markers by default");
}

#[test]
fn large_line_keeps_markers_when_pointradius_set() {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", i % 50)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}],"pointRadius":2}}]}}}}"#,
        labels.join(","), data.join(","));
    let spec = crate::frontend::chartjs::parse(&json).unwrap();
    let m = /* TextMeasurer */;
    let scene = crate::layout::line::build(&spec, &m);
    let markers = scene.items.iter().filter(|p| matches!(p, crate::scene::Prim::Circle { .. })).count();
    assert!(markers > 0, "explicit pointRadius should keep markers");
}
```

**Step 2: テスト失敗を確認**

Run: `cargo test -p fulgur-chart large_line_suppresses_markers large_line_keeps_markers 2>&1 | tail -20`
Expected: `large_line_suppresses_markers_by_default` が FAIL（現状マーカー全描画）

**Step 3: 実装**（marker ループ 130-140行）

```rust
        // マーカー。threshold 超過で間引いた場合は既定で抑制（pointRadius 明示時のみ描画）。
        let marker_r = match (decimated, ser.point_radius) {
            (true, None) => None,                 // 抑制
            (true, Some(r)) if r > 0.0 => Some(r), // 明示半径で描画
            (true, Some(_)) => None,              // pointRadius:0 → 抑制
            (false, _) => Some(MARKER_R),         // 通常（バイト不変）
        };
        if let Some(r) = marker_r {
            for &(cx, cy, _) in &valid {
                items.push(Prim::Circle {
                    cx, cy, r,
                    fill: ser.stroke_at(0),
                    stroke: ser.stroke_at(0),
                    stroke_width: 0.0,
                });
            }
        }
```
（`decimated` は Task 7 で得たフラグ。`ser.point_radius: Option<f64>` は IR 既存。下方 130-140 の元ループを上記で置換。）

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart large_line_suppresses_markers large_line_keeps_markers 2>&1 | tail -20`
Expected: PASS（2 tests）。`cargo test -p fulgur-chart --test render_line` も再確認（小データのマーカーは radius 3 のまま）。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/line.rs
git commit -m "feat(decimate): threshold 超過 line のマーカーを既定抑制 (pointRadius で復活)"
```

---

## Task 9: 決定性・SVG↔PNG 一致・no-op 証明・新規 golden

**Files:**
- Modify: `crates/fulgur-chart/tests/render_line.rs`（決定性・no-op）
- Create: 新 golden（`tests/golden/` に追加する場合は `golden_png.rs` の `NAMES` と `examples/specs/` に対応エントリ）

**Step 1: バイト不変の保証（既存 golden が真の証拠）**

> **advisor 指摘:** 「enabled:false vs threshold:巨大」の比較は**両方とも非間引き経路同士**の
> 比較に過ぎず、機能追加前バイトとの一致を証明しない。**真の pre-feature 保証は、threshold
> 未満で不変のまま緑であり続ける既存の小 golden**（`render_line__*` スナップショット・
> `golden/line.png`・`inspect_model`）である。これは Task 7 Step 5 で確認済み。本 Step では
> その事実を計画上明記し、追加で「enabled:false の巨大 line は全点を保持（=非間引きと同形）」
> という**サニティ**テストのみ置く（バイト履歴の証明とは呼ばない）。

```rust
#[test]
fn disabled_decimation_keeps_all_points_sanity() {
    // サニティ: enabled:false の巨大 line は単一セグメント全点を保持し、間引きされない。
    let n = 3000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 13) % 50)).collect();
    let off = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}},"options":{{"plugins":{{"decimation":{{"enabled":false}}}}}}}}"#,
        labels.join(","), data.join(","));
    assert_eq!(polyline_pts(&off), n);
}
```
（`polyline_pts` は Task 7 のヘルパを再利用。）

**真の回帰保証**: 既存の `render_line__*` / `golden/line.png` / `inspect_model` が**無改変で緑**
であることをもって、threshold 未満の出力がバイト不変であることを担保する（Task 7 Step 5 で確認）。

**Step 2: 決定性テスト（同入力→同バイト、SVG・PNG）**

```rust
#[test]
fn decimated_line_is_deterministic() {
    let n = 4000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i*29)%83)).collect();
    let json = format!(r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","), data.join(","));
    assert_eq!(render_svg_string(&json), render_svg_string(&json));
    // PNG も（既存 PNG レンダヘルパで）
}
```

**Step 3: SVG↔PNG 一致（同一 Scene を消費していることの担保）**

`build()` の Scene は SVG/PNG 共通。明示テストとして「decimation-on の line を SVG・PNG 双方でレンダしエラー無く決定的」を確認（既存 `wasm_runtime.rs` の決定性テスト形を参考）。

**Step 4: 新規 golden（min-max / lttb 可視化）**

`examples/specs/` に `line_decimated.json`（中規模・threshold 超過、min-max）と `line_decimated_lttb.json`（lttb 指定）を追加し、`tests/golden_png.rs` の `NAMES`（17行）に登録。

Run: `cargo test -p fulgur-chart --test golden_png 2>&1 | tail`
初回は golden 不在で FAIL → golden 生成手順（既存の golden 更新フロー。`UPDATE_GOLDEN=1` 等のenv があれば使用。`golden_png.rs` 冒頭の更新方法コメントを確認）に従い生成 → 再実行で PASS。

**Step 5: テスト通過を確認**

Run: `cargo test -p fulgur-chart 2>&1 | tail -30`
Expected: 全 PASS。

**Step 6: コミット**

```bash
git add crates/fulgur-chart/tests/ crates/fulgur-chart/examples/specs/ crates/fulgur-chart/tests/golden/
git commit -m "test(decimate): 決定性・no-op 証明・SVG↔PNG 一致・新規 golden を追加"
```

---

## Task 10: bench 変種・CHANGELOG・目視確認

**Files:**
- Modify: `crates/fulgur-chart/benches/cases.rs`（`line_large_decimated` 追加）
- Modify: `CHANGELOG.md`（あれば）

**Step 1: bench に decimation-on 変種を追加**（`benches/cases.rs` の `all()` 14-41行）

`line(n)` と同形で `options.plugins.decimation.enabled` を省略（=既定オン）した 10000 点 line は既に自動間引きされるが、**off-path ベースライン維持のため `line_large` は enabled:false を明示**し、新たに `line_large_decimated`（既定オン）を追加する。

```rust
// 既存 line_large は off-path ベースラインとして enabled:false を明示
Case { name: "line_large", json: line_with_decimation(10_000, false) },
// 新規: 自動間引き経路
Case { name: "line_large_decimated", json: line_with_decimation(10_000, true) },
```
`fn line_with_decimation(n, enabled)` を追加（`fn line` 70-76 を複製し options.plugins.decimation を付与）。

注意: `tests/bench_cases.rs` の `large_cases_have_expected_scale`（37-42）は `line_large` を name で引き JSON バイト長のみ assert。enabled:false 付与で JSON は長くなる（>10000 維持）ので assertion は通る。要再確認。

**Step 2: bench ビルド確認**

Run: `cargo build -p fulgur-chart --benches 2>&1 | tail`
Expected: エラー無し。

**Step 3: 実物の目視確認（受け入れ条件）**

decimation-on の中規模 line（例: Task 9 の `line_decimated.json`）を PNG レンダし、**マーカー帯が消えクリーンな線になっているか目視**:
```bash
cargo run -p fulgur-chart --example <render-example> -- examples/specs/line_decimated.json /tmp/dec.png 2>&1 | tail
```
（正確な example 名は `crates/fulgur-chart/examples/` を確認。無ければ簡易レンダのテストを一時追加して PNG を吐く。）
生成画像をユーザーに提示し「綺麗」を**目で確認してから**受け入れる。思い込みで断定しない。

**Step 4: CHANGELOG / 互換性乖離の明記**

`CHANGELOG.md`（あれば）に追記:
```
- line/area: 巨大データ（点数 > プロット幅×4）を既定で自動デシメーション（min-max）＋マーカー抑制。
  Chart.js は decimation を既定無効にするため出力が異なる。無効化は
  `options.plugins.decimation.enabled=false`、マーカー復活は `pointRadius` 明示。
```

**Step 5: 全テスト + clippy + fmt**

Run:
```bash
cargo test -p fulgur-chart 2>&1 | tail -15
cargo clippy -p fulgur-chart 2>&1 | tail -15
cargo fmt -p fulgur-chart -- --check 2>&1 | tail
```
Expected: 全 PASS、clippy 警告無し、fmt 差分無し（差分あれば `cargo fmt -p fulgur-chart` で整形しコミット）。

**Step 6: コミット**

```bash
git add -A
git commit -m "test(decimate): bench 変種を追加し CHANGELOG に互換性乖離を記載"
```

---

## 完了の定義（設計ドキュメントの受け入れ条件と対応）

1. ✅ `enabled:false` 時、巨大データでも変更前とバイト一致（Task 9 Step 1）
2. ✅ 既定（自動オン）で巨大 line が間引かれ SVG・PNG が一致・決定的（Task 7, 9）
3. ✅ min-max / lttb 両方動作、`algorithm` で切替（Task 1, 2, 4）
4. ✅ threshold 超過でマーカー抑制、目視でクリーン（Task 8, 10 Step 3）
5. ✅ schema↔strict parity（Task 5, 6）
6. ✅ `line_large` decimation-on bench で高速化（Task 10）
7. ✅ 全テスト緑、CHANGELOG 記載（Task 10）

## スコープ外（フォローアップ issue 候補）

- mixed チャート（line+bar）の line 間引き（`src/layout/mixed.rs`）。
- inspect モデル（`line_points()` / `model.rs`）の巨大 line 整合。
- sparkline（`src/layout/sparkline.rs`）の間引き。
