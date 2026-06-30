# マーカー stamp キャッシュ (raster 最適化 MVP) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 大データ PNG の支配項である per-point 円マーカー AA fill を、円を一度だけ AA ラスタライズした stamp の手書き source-over blit に置換し、scatter ~3× / line ~1.6× を狙う（beads: fulgur-chart-4pn）。

**Architecture:** `raster_direct.rs`（PNG 経路）の `scene_to_pixmap` 描画ループのみを変更。連続する均一 `Prim::Circle` run（同一 `r/fill/stroke/stroke_width`）を検出し、run 長が `STAMP_MIN_RUN`(=128) 以上なら B=8 サブピクセル stamp（**fill + stroke を焼く**）を1度生成して各点に整数 blit、未満・bubble・per-point多色はフォールバックで従来 `fill_path`。**重要(実測): 既定 scatter マーカーは `borderWidth=1`＝fill+stroke。stroke はフォールバックせず stamp に焼く**（さもないと既定 scatter が全フォールバックで stamp 無効化）。SVG 経路・scene・layout・意味モデルには触れない（chart.js 互換ハーネスは無影響）。

**Tech Stack:** Rust, tiny-skia 0.11.4（`Pixmap`/`PathBuilder::from_circle`/`fill_path`、blit は自前の premultiplied source-over）、criterion(render bench)、insta/golden_png。

**前提知識（spike 実証済 / `examples/spike_stamp.rs`）:**
- マーカー fill = scatter_large の 75%(33.6/44.7ms)。pointRadius:0 床=11ms。
- 手書き source-over blit は tiny-skia `draw_pixmap` と **バイト完全一致**かつ ~7× 速い（`draw_pixmap` は位置が Pattern transform に焼かれ再利用不可）。
- blit 式: 整数 premultiplied source-over `out_c = s_c + (d_c·(255−s_a)+127)/255`（identity/Nearest/SourceOver で tiny-skia と一致）。
- 閾値 break-even ≈ 68(isolated)/38(実パイプライン)点 → `STAMP_MIN_RUN=128` は保守値。
- golden fixture 実マーカー数: line=7/area=4/bar=5/pie=4（全て <128 → fill_path 据え置き）。

**主要参照:**
- 変更点ループ: `crates/fulgur-chart/src/raster_direct.rs:183-185`
- 既存 Circle 描画: `raster_direct.rs:450-477`（`PathBuilder::from_circle`+`fill_path`、`stroke_width>0` で stroke）
- Prim 定義: `crates/fulgur-chart/src/scene.rs:58-65`（`Circle{cx,cy,r:f64, fill:Color, stroke:Color, stroke_width:f64}`）
- 型: `Color{r,g,b:u8, a:f32}` は `PartialEq` 派生済（`ir.rs:5`）
- ヘルパ: `solid_paint`(:837) / `make_stroke`(:843)
- bench: `crates/fulgur-chart/benches/render.rs`（`e2e_png/scatter_large` 等）、cases: `benches/cases.rs`
- 決定性テスト: `crates/fulgur-chart/tests/wasm_runtime.rs`（PNG を fnv1a ハッシュ。PNG は native 間でも byte 非一致だが wasm32==ubuntu native は成立）

---

### Task 0: ベースライン確認（変更前の緑と数値を固定）

**Step 1:** render bench を実行し scatter_large / line_large の現値を記録。
Run: `cargo bench -p fulgur-chart --bench render -- 'e2e_png/(line_large|scatter_large)'`
Expected: scatter_large ≈ 44–48ms, line_large ≈ 78–80ms（環境依存・数値メモ）。

**Step 2:** 既存テスト・互換ハーネス緑を確認。
Run: `cargo test -p fulgur-chart` / `cargo clippy -p fulgur-chart -- -D warnings`
Run: `cd tools && npm run compat -- scatter line bubble; cd ..`
Expected: 全緑。これが回帰判定の基準。

**Step 3: Commit**（コード変更なし、メモのみなら skip 可）。

---

### Task 1: MarkerKey と連続 run 検出（純関数・TDD）

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs`（private fn 追加 + `#[cfg(test)]`）

**Step 1: 失敗するテストを書く**（`raster_direct.rs` の test mod）
```rust
#[test]
fn uniform_circle_run_len_counts_consecutive_identical_markers() {
    let c = Color { r: 1, g: 2, b: 3, a: 1.0 };
    let s = Color { r: 9, g: 9, b: 9, a: 1.0 };
    let mk = |cx: f64| Prim::Circle { cx, cy: 0.0, r: 3.0, fill: c, stroke: s, stroke_width: 0.0 };
    let items = vec![mk(0.0), mk(1.0), mk(2.0),
        Prim::Circle { cx: 3.0, cy: 0.0, r: 4.0, fill: c, stroke: s, stroke_width: 0.0 }];
    assert_eq!(uniform_circle_run_len(&items, 0), 3); // r が変わる4個目で切れる
    assert_eq!(uniform_circle_run_len(&items, 3), 1);
    assert_eq!(uniform_circle_run_len(&items, 0 /* 非Circleなら */), 3);
}
```

**Step 2:** 失敗確認。Run: `cargo test -p fulgur-chart uniform_circle_run_len -- --nocapture` → FAIL（未定義）。

**Step 3: 最小実装**
```rust
/// items[start] から始まる、同一 appearance(r/fill/stroke/stroke_width) の
/// 連続 Prim::Circle の個数を返す。items[start] が Circle でなければ 0。
fn uniform_circle_run_len(items: &[Prim], start: usize) -> usize {
    let Prim::Circle { r, fill, stroke, stroke_width, .. } = &items[start] else { return 0 };
    let mut n = 0;
    for it in &items[start..] {
        match it {
            Prim::Circle { r: r2, fill: f2, stroke: s2, stroke_width: w2, .. }
                if r2 == r && f2 == fill && s2 == stroke && w2 == stroke_width => n += 1,
            _ => break,
        }
    }
    n
}
```

**Step 4:** PASS 確認。Run: `cargo test -p fulgur-chart uniform_circle_run_len`

**Step 5: Commit** `git add -A && git commit -m "feat(raster): 連続均一マーカー run 検出"`

---

### Task 2: stamp ビルダ（B=8 サブピクセル・TDD）

**Files:** Modify `crates/fulgur-chart/src/raster_direct.rs`

**設計メモ:** device 空間で焼く。`r_dev = r*scale`、`pad = ceil(r_dev)+2`、`size = 2*pad+2`。stamp k=(sx,sy) は中心 `(pad+sx/B, pad+sy/B)` の円を `from_circle`+`fill_path`(identity) で焼き、`stroke_width>0` なら `stroke_path` も（device stroke = `stroke_width*scale`）。本体の per-point 描画と同一エンジン＝決定性も同条件。

**Step 1: 失敗するテスト**
```rust
const STAMP_B: u32 = 8;
#[test]
fn stamp_matches_fill_path_within_tolerance() {
    // 単一マーカーを (a) 従来 fill_path と (b) stamp blit で描き、
    // golden 同等の指標(チャンネル|d|>4 の画素割合)が小さいこと。
    // 中心は整数+0.5px、scale=1。
    let fill = Color { r: 54, g: 162, b: 235, a: 1.0 };
    let key = MarkerKey { r: 3.0, fill, stroke: fill, stroke_width: 0.0 };
    let stamps = build_stamp_set(&key, 1.0);
    // ... 40x40 pixmap に baseline と stamp を描いて diff_frac < 0.01 を assert
}
```

**Step 2:** 失敗確認。

**Step 3: 実装**（`MarkerKey` 構造体、`build_stamp_set(&MarkerKey, scale) -> StampSet`）。`StampSet{ stamps: Vec<Pixmap>, pad: i32, b: u32 }`。stroke は `stroke_width>0` のときのみ焼く。fill/stroke の Paint は既存 `solid_paint`/`make_stroke` を device 値で流用。

**Step 4:** PASS 確認。

**Step 5: Commit** `feat(raster): B=8 サブピクセル stamp ビルダ`

---

### Task 3: 手書き source-over blit（TDD・tiny-skia 一致をロック）

**Files:** Modify `crates/fulgur-chart/src/raster_direct.rs`

**Step 1: 失敗するテスト**
```rust
#[test]
fn manual_blit_byte_identical_to_draw_pixmap() {
    // 同一 stamp を (a) 自前 blit, (b) tiny-skia draw_pixmap で同座標に重ね描き、
    // 重なりを含めて data() がバイト一致すること（spike で実証済の不変条件をロック）。
    // opaque と alpha 両方、重なり位置を含める。
}
```

**Step 2:** 失敗確認。

**Step 3: 実装** `blit_stamp(pm: &mut Pixmap, set: &StampSet, cx_dev: f32, cy_dev: f32)`：`pick` で stamp 選択 → bbox クリップ → 整数 premultiplied source-over。式は前提知識の通り。

**Step 4:** PASS 確認（バイト一致 0 差）。

**Step 5: Commit** `feat(raster): 手書き premultiplied source-over blit`

---

### Task 4: 描画ループへ配線 + フォールバック（統合・TDD）

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs:183-185`（ループ再構成）
- Test: `crates/fulgur-chart/tests/render_scatter.rs`, `render_line.rs`, `render_bubble.rs`

**Step 1: 失敗する統合テスト**（`render_scatter.rs`）
```rust
#[test]
fn large_uniform_scatter_stamp_path_within_tolerance_of_fillpath() {
    // 200点(>=128)の均一 scatter を render。stamp 経路を通る。
    // 事前に保存した fill_path 版（環境変数で強制 or 別関数）とピクセル許容比較。
}
#[test]
fn per_point_color_falls_back_to_fillpath() {
    // backgroundColor 配列で色を点ごとに変える → run=1 → fill_path 経路。
    // 出力が従来と完全一致(バイト)すること。
}
```
同様に bubble（点ごと r）・per-point色配列・点数<128 が fill_path 経路（出力不変）になるテストを `render_bubble.rs` 等に追加。**stroke 付き(borderWidth>0)はフォールバックしない** — 既定 scatter(stroke=1) >=128点が stamp 経路を通り fill_path 許容内(~2%)であるテストを追加（stroke が焼かれていることの確認）。

**Step 2:** 失敗確認。

**Step 3: 実装** — `scene_to_pixmap` のループを index ベースに再構成:
```rust
let mut i = 0;
while i < scene.items.len() {
    let run = uniform_circle_run_len(&scene.items, i);
    if run >= STAMP_MIN_RUN {
        if let Prim::Circle { r, fill, stroke, stroke_width, .. } = &scene.items[i] {
            let key = MarkerKey { r: *r, fill: *fill, stroke: *stroke, stroke_width: *stroke_width };
            let set = build_stamp_set(&key, scale);
            for it in &scene.items[i..i + run] {
                if let Prim::Circle { cx, cy, .. } = it {
                    blit_stamp(&mut pixmap, &set, (*cx as f32) * scale, (*cy as f32) * scale);
                }
            }
        }
        i += run;
    } else {
        render_prim(&mut pixmap, &scene.items[i], transform, face, &mut glyph_cache);
        i += 1;
    }
}
```
`STAMP_MIN_RUN: usize = 128` を名前付き定数で定義（コメントに break-even 由来と tunable を明記）。

**Step 4:** PASS 確認（stamp 経路は許容内、フォールバックは出力不変）。
Run: `cargo test -p fulgur-chart render_scatter render_line render_bubble`

**Step 5: Commit** `feat(raster): stamp cache をループへ配線 + フォールバック`

---

### Task 5: 決定性ゲート（native↔wasm、stamp 経路）

**Files:** Modify `crates/fulgur-chart/tests/wasm_runtime.rs`

**設計メモ:** 既存 fixture は <128点で stamp 経路を通らない。stamp 経路を通る spec（>=128 均一マーカー）を追加し、PNG を fnv1a ハッシュ。blit は整数で決定的、stamp 生成は fill_path と同エンジンなので、既存 PNG と同じ native↔wasm 関係（wasm32==ubuntu native）が保たれることを確認。

**Step 1:** stamp 経路を通る `sample_stamp_spec()`（>=128 点均一 scatter）を追加し、PNG ハッシュの決定性（同一 target で2回一致）を assert。

**Step 2:** 失敗/緑確認。Run: `cargo test -p fulgur-chart --test wasm_runtime`
（可能なら）Run: `wasm-pack test --node crates/fulgur-chart` 相当で wasm32 緑。

**Step 3: Commit** `test(raster): stamp 経路の決定性ゲート`

---

### Task 6: bench + 閾値の実測再確認

**Step 1:** render bench 実行、Task 0 比で scatter_large の高速化を確認。
Run: `cargo bench -p fulgur-chart --bench render -- 'e2e_png/(line_large|scatter_large)'`
Expected: scatter_large が有意に高速化（受入: >=2×、投影 ~3×）。line_large も改善（マーカー分）。

**Step 2:** 実パイプライン break-even を確認し、必要なら `STAMP_MIN_RUN` を微調整（128 近傍）。判断を issue notes に記録。

**Step 3: Commit**（調整があれば）。

---

### Task 7: 品質ゲート + golden/CHANGELOG

**Step 1:** 全テスト・clippy・互換ハーネス。
Run: `cargo test -p fulgur-chart` / `cargo clippy --all-targets -- -D warnings`
Run: `cd tools && npm run compat; cd ..`
Expected: 全緑。golden_png は fixture <128点で fill_path のまま → **再生成不要**で緑のはず。万一差分が出たら原因調査（閾値漏れ/フォールバック不発のバグの可能性）。

**Step 2:** patch coverage 確認（プロジェクト基準）。

**Step 3:** `CHANGELOG.md` に「128点以上のマーカー図で PNG が高速化、出力は視覚的に同等だが byte は変化し得る」旨を追記。

**Step 4: Commit** `docs: CHANGELOG に stamp cache を記載`

---

### Task 8: spike 後始末

**Step 1:** スパイクを削除。Run: `rm crates/fulgur-chart/examples/spike_stamp.rs`（空になれば examples ディレクトリも）。
**Step 2:** ビルド確認。Run: `cargo build -p fulgur-chart`
**Step 3: Commit** `chore: スパイク削除`

---

## 受入基準（issue acceptance と一致）
1. render bench で scatter_large が end-to-end で有意高速化（>=2×、投影~3× を実測裏取り）
2. stamp-path が fill_path-path の許容内である新テスト緑（golden 再生成後も残るオラクル）
3. native↔wasm32 で stamp PNG が決定的（既存 PNG と同関係）
4. フォールバック（bubble/per-point多色/run<128/巨大半径）が fill_path で出力不変（stroke 付きは stamp に焼くのでフォールバックしない）
5. tools/chartjs-compat（diffModels+crosscheck）緑・既存全テスト緑・clippy -D warnings 緑・patch coverage 維持
6. line ポリライン残差のデシメーションは別 issue（スコープ外）
