# Vega-Lite `circle` mark 実装プラン

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Vega-Lite の `mark: "circle"` を受理し、point mark と同じ scatter 経路で描画する(shape 非対応の構造的差分あり)。

**Architecture:** ランタイムパーサ (`frontend/vegalite.rs`) は `serde_json::Value` を直接扱うため、`parse_mark` に `"circle" => Ok(ChartKind::Scatter)` の 1 分岐を足すだけで point と同じ経路が使える。JSON Schema エクスポート (`schema/vegalite.rs`) には別 variant `VegaLiteSpec::Circle(VlCircleSpec)` を追加し、`VlCircleEncoding` に `shape` を含めないことで構造的に "shape 非対応" を担保する。テストは `tests/frontend_vegalite.rs` に circle 用を追加。

**Tech Stack:** Rust, serde, schemars, cargo test。

---

## Baseline

- 作業 worktree: `/home/ubuntu/fulgur-chart/.worktrees/vl-circle-mark`
- ブランチ: `feat/vl-circle-mark`(base: main @ 752cbcb)
- 事前確認: `cargo test -p fulgur-chart --tests --lib` が全 pass 状態

## Task 1: circle mark を parse_mark で受理する(TDD)

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs:201-211` (`parse_mark` の match)
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`(末尾に追加)

**Step 1: 失敗テストを追加**

`crates/fulgur-chart/tests/frontend_vegalite.rs` 末尾に以下を追加。

```rust
#[test]
fn circle_mark_maps_to_scatter_with_points() {
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
    assert_eq!(spec.series.len(), 1);
    let pts = &spec.series[0].points;
    assert_eq!(pts.len(), 2);
    assert_eq!((pts[0].x, pts[0].y), (1.0, 2.0));
    assert_eq!((pts[1].x, pts[1].y), (3.0, 4.0));
}

#[test]
fn circle_mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "circle"},
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x"}, "y": {"field":"y"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Scatter));
}
```

**Step 2: 失敗を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite circle_mark 2>&1 | tail -20
```
Expected: 2 テストが `未対応の mark: circle` で fail。

**Step 3: `parse_mark` に circle 分岐を追加**

`crates/fulgur-chart/src/frontend/vegalite.rs:208` の `"point" => Ok(ChartKind::Scatter),` の直後に以下を追加。

```rust
        "circle" => Ok(ChartKind::Scatter),
```

**Step 4: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite circle_mark 2>&1 | tail -20
```
Expected: 2 tests pass。

**Step 5: 既存テスト全体の回帰なしを確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite 2>&1 | tail -5
```
Expected: `test result: ok. XX passed; 0 failed`

**Step 6: commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(vegalite): accept `mark: "circle"` in frontend parser

circle mark を point と同じ ChartKind::Scatter 経路にマップする。
描画は scatter レイアウトが常に Prim::Circle を出すため既存経路で正しい。

refs: fulgur-chart-ov1
EOF
)"
```

## Task 2: JSON Schema に circle variant を追加(構造的 shape 非対応)

**Files:**
- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`

**Step 1: `MarkCircle` enum を追加**

`crates/fulgur-chart/src/schema/vegalite.rs:67`(`MarkPoint` の直後)に追加。

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkCircle {
    Circle,
}
```

**Step 2: `VegaLiteSpec::Circle` variant を追加**

`crates/fulgur-chart/src/schema/vegalite.rs:13` の `Point(VlPointSpec),` の次行に追加。

```rust
    Circle(VlCircleSpec),
```

**Step 3: `VlCircleSpec` と `VlCircleEncoding` を追加**

`crates/fulgur-chart/src/schema/vegalite.rs:151`(`VlPointEncoding` の直後、arc セクション区切り `// ───...` の前)に追加。`shape` フィールドを持たない `VlPointEncoding` のミラー。

```rust
// ────────────────────────────────────────────────
// Circle plot (mark: "circle")
//
// point mark の常に塗りつぶし円バリアント。`shape` フィールドは意図的に持たない
// ため、将来 point mark に shape エンコーディングが加わっても circle は shape
// 非対応のまま構造的に保たれる。
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCircleSpec {
    pub mark: MarkCircle,
    pub data: VlData,
    pub encoding: VlCircleEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCircleEncoding {
    pub x: VlChannel,
    pub y: VlChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlChannel>,
}
```

**Step 4: build を確認**

```bash
cargo build -p fulgur-chart 2>&1 | tail -10
```
Expected: 警告なし・エラーなしでビルド成功。

**Step 5: 全テスト回帰なし**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail -5
```
Expected: 全 pass。既存 322/91 等がそのまま緑。

**Step 6: JSON Schema エクスポートに Circle が現れることを確認**

```bash
cargo run -p fulgur-chart-cli -- schema vegalite 2>&1 | grep -c '"MarkCircle"\|"VlCircleSpec"'
```
Expected: `2` 以上(両方の定義がスキーマに含まれる)。

**Step 7: commit**

```bash
git add crates/fulgur-chart/src/schema/vegalite.rs
git commit -m "$(cat <<'EOF'
feat(vegalite): add Circle variant to JSON Schema (mark: "circle")

VegaLiteSpec::Circle(VlCircleSpec) を追加し、`VlCircleEncoding` から
`shape` フィールドを意図的に外す。将来 point mark に shape エンコーディングが
加わっても circle は shape 非対応のまま構造的に保たれる。

refs: fulgur-chart-ov1
EOF
)"
```

## Task 3: SVG レンダリングの回帰テスト(smoke)

**Files:**
- Modify: `crates/fulgur-chart/tests/frontend_vegalite.rs`(末尾に追記)

**Step 1: レンダリング smoke テストを追加**

```rust
#[test]
fn circle_mark_renders_svg() {
    let json = r#"{
        "mark": "circle",
        "data": {"values": [{"x":1,"y":2},{"x":3,"y":4}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}}
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
    // Prim::Circle が出るので <circle 要素が含まれる。
    assert!(svg.contains("<circle "));
}
```

**Step 2: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite circle_mark_renders_svg 2>&1 | tail -10
```
Expected: pass。

**Step 3: commit**

```bash
git add crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
test(vegalite): add SVG smoke test for `mark: "circle"`

circle mark 経由で描画された SVG に <circle> 要素が出ることを確認する。

refs: fulgur-chart-ov1
EOF
)"
```

## Task 4: 最終検証と clippy

**Step 1: 全テスト**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail -30
```
Expected: 全 pass。

**Step 2: clippy**

```bash
cargo clippy -p fulgur-chart --tests -- -D warnings 2>&1 | tail -20
```
Expected: warnings 0。

**Step 3: fmt**

```bash
cargo fmt -p fulgur-chart --check 2>&1 | tail -5
```
Expected: 差分なし。差分ありなら `cargo fmt -p fulgur-chart` して commit する。

---

## YAGNI / 非対応(この plan では扱わない)

- `shape` エンコーディング全般(point 側も未対応。circle が「shape 無視」であることは構造で担保)
- `size` / `opacity` encoding(point 側も未対応)
- circle 専用の SVG スナップショット(scatter レンダーと同一経路)
- CLI/バインディング側の追加(schema export 経由で自動で追随する)
- example / spec ファイルの追加

## 受け入れ基準(from bd issue)

- [x] `mark: "circle"` を含む Vega-Lite 仕様が正常にパースされる → Task 1
- [x] 内部的に `ChartKind::Scatter` として扱われ、既存 scatter レンダリング経路で SVG が生成される → Task 1, 3
- [x] `shape` フィールドを含む circle mark はスキーマ段階で拒否される(strict encoding check) → 既存 `check_unknown_keys` が `shape` を許容していないため既に成立(Task 2 の struct レベルでも構造的に不許可)
- [x] 既存 point mark テスト・スナップショットが引き続き通る → 各タスクの回帰確認で保証
