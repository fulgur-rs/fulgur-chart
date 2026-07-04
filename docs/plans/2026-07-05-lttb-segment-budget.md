# LTTB × gap セグメント予算按分 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** gap で多数セグメントに割れた系列を LTTB 間引きしたとき、合計点数が `samples × セグメント数` に膨れる予算超過を、セグメント長で予算を按分することで解消する。

**Architecture:** 間引き予算 `samples` をセグメントごとに長さ比で按分する純粋関数 `decimate_segments` を `decimate.rs` に抽出し、`line.rs` の inline ループを置換する。整数演算のみで決定的（SVG↔PNG バイト一致）。既存の `min_max`/`lttb` アルゴリズム関数は無変更（FLOOR=3 が LTTB の `samples≥3` ガードを満たすため）。

**Tech Stack:** Rust (edition 2024)、`crates/fulgur-chart`、`cargo test` / `cargo clippy` / `cargo fmt`。

**問題の要約（現状 `line.rs:88-97`）:**
- 少数大セグメント (5×5000, samples=100): 各 5000→100 = 合計500点（**5倍超過** ← 実害）
- 多数小セグメント (100×10, samples=100): 各 n≤samples で passthrough → 1000点（削減ゼロ）
- min-max は列バケツで自己制限するため無影響（既定 min-max、lttb 明示時のみの限定事項）

**修正の骨子:** `budget_i = max(3, samples × len_i / total)`（整数演算）。
`sum(floor(samples·len_i/total)) ≤ samples` かつ各出力 `output_i ≤ budget_i` より、
**出力上限 `total_out ≤ samples + 3×num_segments`** が保証される。

---

### Task 1: `decimate_segments` を抽出し予算按分を実装（純粋関数・unit TDD）

**Files:**
- Modify: `crates/fulgur-chart/src/layout/decimate.rs`（`decimate_one` の直後に追加、`#[cfg(test)] mod tests` に unit テスト追加）

**Step 1: Write the failing tests**

`decimate.rs` の `mod tests` 内（既存 `decimate_one_dispatches_lttb` の後）に追加:

```rust
    // ヘルパ: k セグメント × 各 seg_len 点の連続系列群を作る（cat は連番、gap 相当に非連続）。
    fn make_segments(k: usize, seg_len: usize) -> Vec<Vec<(f64, f64, usize)>> {
        (0..k)
            .map(|s| {
                (0..seg_len)
                    .map(|i| {
                        let idx = s * (seg_len + 1) + i; // +1 で cat に隙間（gap）を作る
                        (idx as f64, ((idx * 31) % 97) as f64, idx)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn decimate_segments_lttb_bounds_few_large() {
        // 少数大セグメント: 5×5000, samples=100。素朴実装なら 5×100=500 点だが、
        // 按分すれば合計 ≈ samples に収まる。
        let segs = make_segments(5, 5000);
        let out = decimate_segments(&segs, DecimationAlgorithm::Lttb, 100);
        let total: usize = out.iter().map(|s| s.len()).sum();
        // 証明済み上限: samples + 3×num_segments。
        assert!(
            total <= 100 + 3 * 5,
            "total {total} must be <= samples + 3*num_segments"
        );
        // 素朴実装（各セグメントに full samples）なら 500。按分でそれを大幅に下回る。
        assert!(total < 100 * 5, "proration must beat naive per-segment budget");
    }

    #[test]
    fn decimate_segments_lttb_many_small_no_passthrough() {
        // 多数小セグメント: 100×10, samples=100。素朴実装は各 n(=10)≤samples(=100) で
        // passthrough → 1000 点（削減ゼロ）。按分後は budget=max(3,1)=3 で LTTB 発動。
        let segs = make_segments(100, 10);
        let out = decimate_segments(&segs, DecimationAlgorithm::Lttb, 100);
        let total: usize = out.iter().map(|s| s.len()).sum();
        assert!(
            total <= 100 + 3 * 100,
            "total {total} must be <= samples + 3*num_segments"
        );
        // 元は 1000 点。passthrough せず明確に削減されること。
        assert!(total < 100 * 10, "many-small must not pass through unreduced");
    }

    #[test]
    fn decimate_segments_min_max_ignores_budget() {
        // min-max は samples を無視するので、按分の有無で結果は変わらず、
        // 各セグメントを個別に min_max した結果と一致する。
        let segs = make_segments(4, 800);
        let out = decimate_segments(&segs, DecimationAlgorithm::MinMax, 100);
        let expected: Vec<Vec<(f64, f64, usize)>> = segs.iter().map(|s| min_max(s)).collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn decimate_segments_is_deterministic() {
        let segs = make_segments(6, 700);
        assert_eq!(
            decimate_segments(&segs, DecimationAlgorithm::Lttb, 80),
            decimate_segments(&segs, DecimationAlgorithm::Lttb, 80)
        );
    }

    #[test]
    fn decimate_segments_single_segment_matches_decimate_one() {
        // 単一セグメントでは total==len なので budget==samples。既存単体 golden が
        // バイト不変であることの根拠（decimate_one を素通しするのと一致）。
        let segs = make_segments(1, 5000);
        let out = decimate_segments(&segs, DecimationAlgorithm::Lttb, 100);
        let expected = vec![decimate_one(&segs[0], DecimationAlgorithm::Lttb, 100)];
        assert_eq!(out, expected);
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p fulgur-chart --lib decimate_segments 2>&1 | tail -20`
Expected: コンパイルエラー `cannot find function decimate_segments`（未定義）。

**Step 3: Write minimal implementation**

`decimate.rs` の `decimate_one` 関数の直後（`#[cfg(test)]` の前）に追加:

```rust
/// gap 分割済みの全セグメントを間引く。`samples` はセグメント長で按分する
/// （`budget_i = max(3, samples × len_i / total)`）。これにより LTTB のマルチ
/// セグメント予算超過（合計 samples×セグメント数）を防ぎ、出力を
/// `samples + 3×セグメント数` 以下に上限化する。整数演算のみ = 決定的。
/// min-max は samples を無視するため按分は実質 LTTB のみに効く（呼び出しは一様）。
/// FLOOR=3 は LTTB の `samples≥3` ガードを満たすので decimate_one 側は無変更。
pub fn decimate_segments(
    segments: &[Vec<(f64, f64, usize)>],
    algo: DecimationAlgorithm,
    samples: usize,
) -> Vec<Vec<(f64, f64, usize)>> {
    let total: usize = segments.iter().map(|s| s.len()).sum();
    segments
        .iter()
        .map(|s| {
            let budget = if total == 0 {
                samples
            } else {
                (samples * s.len() / total).max(3)
            };
            decimate_one(s, algo, budget)
        })
        .collect()
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p fulgur-chart --lib decimate 2>&1 | tail -20`
Expected: 全 decimate 系テスト PASS（新規5件含む）。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/decimate.rs
git commit -m "feat(chart): add decimate_segments with per-segment budget proration (fulgur-chart-vzd)"
```

---

### Task 2: `line.rs` を `decimate_segments` に配線（統合 TDD）

**Files:**
- Modify: `crates/fulgur-chart/src/layout/line.rs:88-97`（inline ループを置換）
- Test: `crates/fulgur-chart/tests/render_line.rs`（新規統合テスト追加）

**Step 1: Write the failing integration test**

`render_line.rs` の末尾付近（`gapped_large_line_keeps_segments_and_decimates` の隣が自然）に追加:

```rust
#[test]
fn gapped_large_line_lttb_prorates_segment_budget() {
    // 回帰: LTTB × gap 多数セグメント時、per-segment に full samples を与えると
    // 合計 samples×セグメント数 点に膨れる（fulgur-chart-vzd）。セグメント長で
    // 予算を按分し、合計を samples+3×セグメント数 以下に上限化することを検証する。
    // JSON は非有限値を表現できないため parse 後に NaN を注入して gap を作る。
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    // samples/threshold を明示し、確実に LTTB 間引きを発動させる。
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}},"options":{{"plugins":{{"decimation":{{"enabled":true,"algorithm":"lttb","samples":100,"threshold":500}}}}}}}}"#,
        labels.join(","),
        data.join(",")
    );
    let mut spec = chartjs::parse(&json, false).unwrap();
    // 3 箇所に孤立 NaN を注入 → 3 gap → 4 セグメント。
    for p in [n / 4, n / 2, 3 * n / 4] {
        spec.series[0].values[p] = f64::NAN;
    }

    let polys = polylines(&spec);
    let counts: Vec<usize> = polys.iter().map(|p| p.len()).collect();
    let num_seg = counts.len();
    let total: usize = counts.iter().sum();

    // 複数セグメントに割れている（gap が保たれている）。
    assert!(num_seg >= 2, "gaps must yield >=2 polylines, got {counts:?}");
    // 各セグメントは崩壊していない。
    assert!(counts.iter().all(|&c| c >= 2), "no segment collapse: {counts:?}");
    // 証明済み上限: samples(=100) + 3×num_segments。素朴実装なら 100×num_seg に膨れる。
    assert!(
        total <= 100 + 3 * num_seg,
        "budget must be prorated across segments: total={total}, num_seg={num_seg}"
    );
    // 素朴 per-segment 予算（samples×num_seg）を明確に下回る。
    assert!(total < 100 * num_seg, "must beat naive per-segment budget: total={total}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_line gapped_large_line_lttb_prorates 2>&1 | tail -20`
Expected: FAIL — 旧 inline 実装は各セグメントに full samples(=100) を与えるため
`total ≈ 100×num_seg`（例 400）となり `total <= 100 + 3*num_seg`（例 112）を満たさない。

**Step 3: Modify `line.rs` to call `decimate_segments`**

`crates/fulgur-chart/src/layout/line.rs` の現行 88-97 行:

```rust
        let decimated = dec.is_some();
        let segments: Vec<Vec<(f64, f64, usize)>> = if let Some((algo, samples)) = dec {
            segments
                .iter()
                // samples はセグメント単位で適用される。LTTB の場合、マルチセグメント系列では
                // 最大 samples × セグメント数 点になりうる（min-max は占有ピクセル列数で自己制限）。
                .map(|s| crate::layout::decimate::decimate_one(s, algo, samples))
                .collect()
        } else {
            segments
        };
```

を以下へ置換:

```rust
        let decimated = dec.is_some();
        let segments: Vec<Vec<(f64, f64, usize)>> = if let Some((algo, samples)) = dec {
            // samples はセグメント長で按分される（decimate_segments）。これにより gap で
            // 多数セグメントに割れた LTTB 系列でも合計が samples+3×セグメント数 以下に収まる
            // （min-max は samples を無視し占有ピクセル列数で自己制限）。
            crate::layout::decimate::decimate_segments(&segments, algo, samples)
        } else {
            segments
        };
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p fulgur-chart --test render_line 2>&1 | tail -25`
Expected: 新規テスト含め全 render_line テスト PASS。特に既存
`gapped_large_line_keeps_segments_and_decimates`（既定 min-max）が緑のまま
（min-max は budget 無視なので不変）。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/line.rs crates/fulgur-chart/tests/render_line.rs
git commit -m "fix(chart): prorate LTTB samples across gap segments (fulgur-chart-vzd)"
```

---

### Task 3: フル検証・golden 不変確認・CHANGELOG

**Files:**
- Modify: `CHANGELOG.md`（存在すれば。パスは実行時に確認）

**Step 1: 既存 golden がバイト不変であることを確認**

Run: `cargo test -p fulgur-chart --test golden_png 2>&1 | tail -15`
Expected: PASS。特に `line_decimated_lttb`（単一セグメント5000点・null無し）は
budget=samples となり出力不変。

**Step 2: フルテスト実行**

Run: `cargo test -p fulgur-chart 2>&1 | tail -25`
Expected: 全テスト PASS、0 failed。

**Step 3: clippy / fmt**

Run: `cargo clippy -p fulgur-chart --all-targets 2>&1 | tail -15`
Expected: warnings 0。
Run: `cargo fmt -p fulgur-chart -- --check 2>&1`
Expected: 差分なし（あれば `cargo fmt -p fulgur-chart` で整形）。

**Step 4: CHANGELOG エントリ追加（存在時）**

`ls CHANGELOG.md crates/fulgur-chart/CHANGELOG.md 2>/dev/null` で場所を確認し、
Unreleased/該当セクションに一行追記:

```
- Fixed: LTTB decimation now prorates its sample budget across gap-split
  segments, preventing per-segment budget overrun (fulgur-chart-vzd).
```

**Step 5: Commit**

```bash
git add -A
git commit -m "docs(chart): note LTTB segment-budget proration in CHANGELOG (fulgur-chart-vzd)"
```

---

## 受け入れ基準（beads: fulgur-chart-vzd）

1. ✅ 新テスト（load-bearing）: 多数セグメント LTTB（threshold 超）で合計出力 ≤ `samples + 3×num_segments`（Task 1 unit + Task 2 統合）
2. ✅ 少数大セグメント LTTB で合計 ≈ samples（±床）（Task 1 `decimate_segments_lttb_bounds_few_large`）
3. ✅ 既存 golden 不変（既定 min-max・単一セグメント lttb で churn なし）（Task 3 Step 1）
4. ✅ `cargo test` 緑 / `clippy` 0 / `fmt` クリーン（Task 3）

## 不採用案（設計より）

- 連結系列を全体 LTTB → セグメント再導出: gap 境界をまたぐ三角形を描画し、cat 再分割で
  線が消える（`line.rs` 既存コメントで却下済み）。セグメント先・按分後が正しいモデル。
- FLOOR=2（端点のみ）: 上限は締まるが LTTB を `samples==2` で端点返却へ拡張する必要があり
  `decimate.rs` のアルゴリズム関数に変更が及ぶ。今回は FLOOR=3 を採用。
