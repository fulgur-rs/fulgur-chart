# Vega-Lite layer / concat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Follow tasks and steps in order; track issue status in Beads.

**Goal:** Vega-Lite の既存 unit mark を葉にして、継承・resolve・Scene 合成を備えた `layer` / `hconcat` / `vconcat` を native と WASM で描画する。

**Architecture:** `schema/vegalite.rs` と再帰 composition IR が入力構造を表す。専用 frontend が inline data と layer encoding を解決し、正規化前の有効データから共有 color/size domain を計算して既存 unit parser で葉を正規化する。x/y の実 domain は正規化済み leaf IR から union する。composition layout は解決済み scale/guide context を使い、Cartesian layer は共通 plot frame 上に重ね、concat は `Prim::Group` で子 Scene を並べる。既存 renderer と WASM は同じ Scene を使う。

**Tech Stack:** Rust 2024、serde/serde_json、schemars、既存 `ChartSpec` / `Scene` renderer、Cargo integration tests、insta SVG snapshots、WASM bindings、PNG goldens。

**Spec:** [docs/superpowers/specs/2026-09-30-fulgur-chart-t38-layer-concat.md](../specs/2026-09-30-fulgur-chart-t38-layer-concat.md)

## Global Constraints

- `layer` / `hconcat` / `vconcat` と仕様書で定めた再帰だけを受理し、`transform` / `facet` / `repeat` / 一般 `concat` / URL data は strict/non-strict 両方で明示エラーにする。
- 現在の全 unit mark は composition leaf として保持する。`layer` は互換 Cartesian view のみ受理し、`arc` / `geoshape` leaf は concat で受理する。
- composition 深さ上限は `InputLimits.max_vega_composition_depth` の既定 32、unit view 上限は `max_vega_composition_views` の既定 256 とする。
- 既存 `InputLimits` のデータ点・生成 primitive 上限は composition 全体に適用し、複数 leaf で再利用される data も leaf ごとに数える。
- layer の既定 scale/axis/legend resolution は shared、concat は position scale/axis independent、non-position scale/legend shared とする。shared domain のカテゴリ順は child 順・入力順の first-seen を保つ。
- concat `spacing` の既定値は 20 px。各 node の width/height は per-view default とし、concat の出力寸法は子寸法と spacing から導く。
- 先にある layer child は後の child の背面に描く。guide は resolution に従って一度だけ描画し、各 child の clip を維持する。
- Vega-Lite image leaf は SVG `<image>` reference のまま保持し、画像を含む composition の PNG/WebP は既存の明示エラーにする。
- ユーザー向け error は英語で返し、順序・色解決は deterministic にする。新しい dependency は追加しない。

## Review Focus

- 子 `data` が親を置き換え、layer encoding は channel 単位で継承・子優先となる。`vegalite_composition_inherits_and_overrides_data_and_encoding` で effective raw leaf と最終 leaf IR の field・record を固定する (Task 2 expansion、Task 4 public parser)。
- non-strict 入力にも nested `transform` / URL data / 混在 operator を置けるため、`vegalite_composition_rejects_unsupported_nodes_in_both_modes` で各 path の明示エラーを固定する (Task 2 expansion、Task 4 public parser)。
- shared positional scale の子が category/quantitative/temporal を混在すると誤った frame になり得る。`composition_shared_scale_rejects_incompatible_channel_types` で error と path を固定する (Task 3)。
- independent layer axes と clip を単に重ねると軸が隠れるか data が隣接 view へ漏れる。`layer_independent_axes_get_separate_gutters_and_keep_marks_clipped` で軸順・gutter・clip を固定する (Task 4)。
- 子数・深さだけでなく同一 data を使う leaf 数でもメモリ/primitive が増える。`composition_guard_counts_reused_data_and_primitives_across_leaves` で既存 point/primitive limits が composition 全体に適用されることを固定する (Task 2 preflight、Task 4 final guard)。

---

## File Structure

- `crates/fulgur-chart/src/schema/vegalite.rs`: recursive `VegaLiteSpec` composition schema、layer/shared encoding、resolve fields を定義する。
- `crates/fulgur-chart/src/ir.rs`: `VegaCompositionNode`、node dimensions/title/spacing、解決済み scale/axis/legend mode を定義する。composition は `ChartKind::VegaComposition` から参照する。
- `crates/fulgur-chart/src/frontend/vegalite_composition.rs`: recursive preflight、path 管理、data/encoding/resolve 継承、effective raw leaf の保持、shared scale domain 解決、unit parser 呼び出しを担当する。
- `crates/fulgur-chart/src/frontend/vegalite.rs`: unit parser を値入力 helper に分け、root の composition dispatch を追加する。
- `crates/fulgur-chart/src/guard.rs`: composition tree の depth/view/aggregate point/primitive を検証し、nested image を再帰検証する。
- `crates/fulgur-chart/src/model.rs`: composition の chart type を報告し、unit-only model access が誤解を招く値を返さないようにする。
- `crates/fulgur-chart/src/layout/vega_composition.rs`: resolved scale/guide context の適用、共通 layer frame、nested concat 寸法と Scene group を担当する。
- `crates/fulgur-chart/src/layout/{bar,line,scatter,vega_rect,error_mark,vega_boxplot}.rs`: composition frame を受ける内部 entry point を追加し、既存単体 layout API は維持する。
- `crates/fulgur-chart/src/layout/mod.rs` と `crates/fulgur-chart/src/raster_direct.rs`: composition layout dispatch と nested image の raster unsupported 判定を追加する。
- `crates/fulgur-chart/tests/frontend_vegalite_composition.rs`: schema、inheritance、unsupported input、resolution、limit の frontend 回帰をまとめる。
- `crates/fulgur-chart/tests/render_vegalite_composition.rs`: layer/concat Scene、scale、guide、clip、寸法と SVG snapshot を検証する。
- `crates/fulgur-chart/tests/wasm_runtime.rs`: composition example を同じ native/WASM runtime test から parse/render する。
- `examples/specs/vegalite-layer.json` と `examples/specs/vegalite-nested-concat.json`: layer bar+line と nested h/v concat を示す。
- `crates/fulgur-chart/tests/golden_png.rs` と `crates/fulgur-chart/tests/golden/`: 両 composition example の PNG golden を登録する。
- `crates/bindings/wasm/src/vegalite-schema.json`: schema generator で更新し、embedded schema parity test を通す。
- `README.md`: supported Vega-Lite subset に composition operators、inherited data/encoding、明示 unsupported 範囲と examples を記載する。

---

### Task 1: Recursive schema and composition IR

**Files:**

- Modify `crates/fulgur-chart/src/schema/vegalite.rs`.
- Modify `crates/fulgur-chart/src/ir.rs`.
- Create `crates/fulgur-chart/tests/frontend_vegalite_composition.rs`.

**Interfaces:**

- Add `VegaLiteSpec::{Layer(Box<VlLayerSpec>), HConcat(Box<VlHConcatSpec>), VConcat(Box<VlVConcatSpec>)}`. Each composition schema has its operator array, optional inline data, optional layer encoding where allowed, optional `resolve`, optional width/height/title, and `deny_unknown_fields`.
- Add `VegaResolutionMode::{Shared, Independent}` and resolved `VegaCompositionResolve` fields `x_scale`, `y_scale`, `color_scale`, `size_scale`, `x_axis`, `y_axis`, `color_legend`, and `size_legend`.
- Add recursive `VegaCompositionNode::{Unit(Box<VegaCompositionLeaf>), Layer(Box<VegaLayerNode>), HConcat(Box<VegaConcatNode>), VConcat(Box<VegaConcatNode>)}`. Nodes and leaves retain a stable JSON path; composition nodes carry ordered children, resolved width/height, title, resolution, and concat spacing. `ChartKind::VegaComposition` is added with the root parser/layout integration in Task 4.
- `VegaCompositionLeaf` holds the existing parsed `ChartSpec` plus effective resolved channel domains needed by layout. `VegaScaleDomain` represents ordered categories, finite numeric extrema, or temporal millisecond extrema; no mark-specific series format is duplicated.

- **Step 1: Write failing schema tests** `vegalite_composition_schema_accepts_each_operator_and_recursive_children` and `vegalite_composition_schema_rejects_empty_arrays_and_unknown_composition_keys`. Assert valid layer/hconcat/vconcat JSON deserializes as `VegaLiteSpec`, nested supported shapes deserialize, and empty arrays / unknown `resolve` keys fail.
- **Step 2: Run the tests to verify failure**

Run: `cargo test -p fulgur-chart --test frontend_vegalite_composition vegalite_composition_schema`

Expected: FAIL because the root schema has no composition variants.

- **Step 3: Add typed recursive composition schema and IR node types** in `schema/vegalite.rs` and `ir.rs`. Keep unit schema definitions and existing `ChartSpec` fields unchanged. Use a recursive `Box` at enum edges and fixed fields for the supported resolution channels. Do not add a `ChartKind` variant yet; integrate it together with all exhaustive dispatches in Task 4.
- **Step 4: Run the tests to verify the schema contract**

Run: `cargo test -p fulgur-chart --test frontend_vegalite_composition vegalite_composition_schema`

Expected: PASS; all supported operator forms deserialize and malformed schema objects fail.

- **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/vegalite.rs crates/fulgur-chart/src/ir.rs crates/fulgur-chart/tests/frontend_vegalite_composition.rs
git commit -m "feat(vegalite): add recursive composition schema"
```

### Task 2: Recursive parser, inheritance, and composition guards

**Files:**

- Create `crates/fulgur-chart/src/frontend/vegalite_composition.rs`.
- Modify `crates/fulgur-chart/src/frontend/vegalite.rs` and `crates/fulgur-chart/src/frontend/mod.rs`.
- Modify `crates/fulgur-chart/src/guard.rs`.
- Add private module tests in `frontend/vegalite_composition.rs`; public parser integration tests are added in Task 4 after composition root dispatch exists.

**Interfaces:**

- Add `InputLimits.max_vega_composition_depth: usize` (default `32`) and `max_vega_composition_views: usize` (default `256`). Existing point and primitive limits apply to the complete composition tree, not once per child.
- Add private `ExpandedCompositionNode::{Unit, Layer, HConcat, VConcat}` and `ExpandedUnitSpec { effective_spec: Value, path: String, inherited_resolve: ... }`. The expanded tree retains each unit's effective inline data, merged layer encoding, dimensions, title, and path without prematurely converting raw channel values into leaf IR.
- Add `fn preflight_composition(value: &Value, inherited: &VegaInheritedSpec, path: &str, depth: usize, limits: &InputLimits, budget: &mut CompositionBudget) -> Result<(), String>`; run it before expansion so depth, view count, child count, and repeated inherited row count are bounded before per-leaf IR allocation.
- Add `fn expand_composition(value: &Value, inherited: &VegaInheritedSpec, strict: bool, path: &str, depth: usize) -> Result<ExpandedCompositionNode, String>`. It validates node/operator shape, rejects unsupported constructs at every depth, and applies data/encoding/resolve inheritance while keeping raw effective leaves.
- Refactor the current public unit path through private `parse_unit_value(value: &mut Value, strict: bool, limits: &InputLimits) -> Result<ChartSpec, String>`. Keep `parse_with_limits` unit behavior unchanged for now; composition root dispatch is added in Task 4.

- **Step 1: Add failing expansion tests** `vegalite_composition_inherits_and_overrides_data_and_encoding`, `vegalite_composition_child_data_replaces_parent_data`, and `vegalite_composition_rejects_unsupported_nodes_in_both_modes`. Assert effective leaf JSON keeps inherited parent records/channels, a child replaces only its repeated channel and data, and nested `transform`, `facet`, `repeat`, `concat`, mixed operators, or `data.url` returns an error containing the exact JSON path in strict and non-strict modes.
- **Step 2: Add failing preflight boundary tests** `composition_guard_checks_depth_and_view_boundaries` and `composition_guard_counts_reused_data_across_leaves`. Use custom limits to assert the exact boundary is accepted, one over is rejected before expansion allocates leaf specs, and inherited rows count once per rendered leaf.
- **Step 3: Run the focused module tests to verify failure**

Run: `cargo test -p fulgur-chart composition_`

Expected: FAIL because recursive expansion and composition limits are absent.

- **Step 4: Extract `parse_unit_value`** from `vegalite::parse_with_limits` without changing unit behavior, register the new frontend module, and add regression assertions for special unit parsers (boxplot, errorbar/errorband, geoshape, image).
- **Step 5: Implement recursive preflight and expansion** in `frontend/vegalite_composition.rs`. Data objects replace inherited data; layer encodings merge by channel with the closest mapping winning; concat nodes inherit data but reject their own `encoding`. Track stable JSON paths and reject unsupported properties in non-strict mode too.
- **Step 6: Run focused tests to verify expansion and guard boundaries**

Run: `cargo test -p fulgur-chart composition_`

Expected: PASS; inherited effective specs, explicit unsupported errors, and depth/view/reused-row bounds match the assertions. The final parsed point/primitive budget is completed with recursive ChartSpec validation in Task 4.

- **Step 7: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite_composition.rs crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/src/frontend/mod.rs crates/fulgur-chart/src/guard.rs
git commit -m "feat(vegalite): expand nested composition specs"
```

### Task 3: Shared scale and guide resolver

**Files:**

- Extend `crates/fulgur-chart/src/frontend/vegalite_composition.rs` with raw-domain resolution and final leaf parsing.
- Modify `crates/fulgur-chart/src/frontend/vegalite.rs` to accept private per-leaf scale overrides.
- Modify `crates/fulgur-chart/src/ir.rs` for `VegaScaleDomain` and the resolved per-leaf scale context defined in Task 1.
- Add private resolver tests to `frontend/vegalite_composition.rs`.

**Interfaces:**

- Add `fn resolve_raw_color_size_scales(node: &ExpandedCompositionNode) -> Result<ResolvedScaleInput, String>`; it reads effective raw records and encoding bindings before unit parsing, recursively applying nearest resolution overrides and unioning shared color/size domains.
- Add `fn parse_resolved_composition(node: ExpandedCompositionNode, scales: ResolvedScaleInput, strict: bool, limits: &InputLimits) -> Result<VegaCompositionNode, String>`; each unit leaf is parsed through the existing parser with `VegaUnitScaleOverrides`, then positional domains are derived from the parsed leaf IR (including error range and boxplot summaries) and unioned where x/y scales are shared.
- `VegaUnitScaleOverrides` carries shared category domains for consistent series/color mapping, numeric color extents for rect marks, and numeric size extents for point/square marks. Resolved x/y positional domains are retained on `VegaCompositionLeaf` for layout. The parser still performs current mark-specific validation and errors.
- Shared categories keep first-seen child and record order. Numeric domains union finite extrema; temporal domains union milliseconds. Channel compatibility is checked from effective field bindings before layout, while actual positional extrema are derived from parsed leaf IR so aggregate marks contribute their rendered ranges. Errors include the owning node path; independent domains remain leaf-local.
- Effective scale and guide modes are stored in composition nodes. Layout guide ownership is deterministic: one shared guide per shared node, or a guide on each independent child.

- **Step 1: Add failing raw-domain tests** `composition_shared_scale_unions_categories_in_first_seen_order`, `composition_shared_scale_unions_numeric_and_temporal_extents`, `composition_shared_color_and_size_use_union_domains`, `composition_shared_scale_rejects_incompatible_channel_types`, `composition_independent_scales_keep_per_child_domains`, and `composition_resolve_inherits_per_channel`. Assert exact resolved values, palette/category indices, point sizes, and path-qualified type errors.
- **Step 2: Run resolver tests to verify failure**

Run: `cargo test -p fulgur-chart composition_`

Expected: FAIL because raw scale domain resolution and per-leaf overrides do not exist.

- **Step 3: Implement raw color/size domain resolution** before converting effective leaf JSON into `ChartSpec`. Resolve defaults by node kind, apply nearest per-channel explicit settings, reject unsupported scale/guide combinations, validate channel types, and preserve first-seen category order.
- **Step 4: Add per-leaf color/size overrides to the unit parser** for shared categorical colors, continuous rect color extents, and size mappings. Keep existing standalone unit calls on the current defaults.
- **Step 5: Parse effective leaves and derive positional domains from the parsed IR.** Union compatible x/y domains after range marks and boxplot summaries have been normalized, then retain leaf-local and shared contexts in `VegaCompositionNode`. Prefix unit parser errors with the leaf path.
- **Step 6: Run resolver tests to verify exact domains and mappings**

Run: `cargo test -p fulgur-chart composition_`

Expected: PASS; domains union as specified, category colors and size encodings use the shared domain, incompatible channel types return a path-qualified error, and independent domains remain distinct.

- **Step 7: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite_composition.rs crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/src/ir.rs
git commit -m "feat(vegalite): resolve composition scales"
```

### Task 4: Layer and concat Scene layout

**Files:**

- Create or extend `crates/fulgur-chart/src/layout/vega_composition.rs`.
- Modify `crates/fulgur-chart/src/ir.rs`, `frontend/vegalite.rs`, `frontend/vegalite_composition.rs`, `guard.rs`, and `model.rs` to integrate the composition root and recursive checks.
- Modify `crates/fulgur-chart/src/layout/mod.rs`, `crates/fulgur-chart/src/layout/common.rs`, and Cartesian leaf builders in `layout/bar.rs`, `layout/line.rs`, `layout/scatter.rs`, `layout/vega_rect.rs`, `layout/error_mark.rs`, and `layout/vega_boxplot.rs`.
- Modify `crates/fulgur-chart/src/raster_direct.rs` and recursive image guard handling for nested image-kind detection.
- Test `crates/fulgur-chart/tests/frontend_vegalite_composition.rs` and `crates/fulgur-chart/tests/render_vegalite_composition.rs`.

**Interfaces:**

- Add `ChartKind::VegaComposition(Box<VegaCompositionNode>)`, public root dispatch in `vegalite::parse_with_limits`, and `model::chart_type_name` support. The composition parser runs preflight, expansion, raw-domain resolution, and leaf parsing before returning the root `ChartSpec`.
- Add recursive `guard::validate_vega_composition` that applies existing leaf validation and accumulates point/primitive estimates across all leaves. Extend image validation to recurse through the node tree so SVG preserves image references and raster output rejects them before rendering.
- Add `VegaCartesianContext { width, height, x_domain, y_domain, category_domains, plot_rect, guide_ownership }` and `VegaViewParts { marks, axes, legends, plot_rect }`. A `build_vega_parts(spec, m, context)` entry point in each Cartesian leaf module returns separable primitives; existing standalone `build` signatures and behavior remain intact.
- Add `fn build_node(node: &VegaCompositionNode, m: &TextMeasurer, limits: &InputLimits) -> Result<VegaNodeLayout, String>`; the result carries dimensions plus content/guide primitives so parents can merge shared legends and independently place child axes without rendering duplicate guides.
- Add `pub(crate) fn build_checked(spec: &ChartSpec, m: &TextMeasurer, limits: &InputLimits) -> Result<Scene, String>` to `layout/vega_composition.rs` and dispatch `ChartKind::VegaComposition` from `layout/mod.rs`. Layer children contribute marks against one shared plot rectangle in source order; guide ownership is resolved once per node. Independent layer y-axes use ordered left/right gutters and x-axes use bottom/top gutters, with each mark group retaining the shared clip.
- HConcat/VConcat place child content and independent axes with `Prim::Group` translations, merge shared non-position legends once, and preserve nested dimensions, spacing, titles, and root background.

- **Step 1: Add failing public parser and layout tests** `vegalite_composition_inherits_and_overrides_data_and_encoding`, `vegalite_composition_rejects_unsupported_nodes_in_both_modes`, `composition_guard_counts_reused_data_and_primitives_across_leaves`, `layer_bar_line_keeps_paint_order_and_shared_frame`, `layer_independent_axes_get_separate_gutters_and_keep_marks_clipped`, `concat_nodes_report_derived_dimensions_and_order`, `nested_concat_contains_layer_and_vconcat_groups`, `composition_titles_and_background_render_once`, and `composition_with_image_keeps_svg_reference_and_rejects_raster`. Assert final leaf IR, exact error paths, aggregate limits, child primitive order, axis/legend count, clip geometry, translation offsets, scene dimensions, title/background count, and PNG/WebP errors.
- **Step 2: Run the focused tests to verify failure**

Run: `cargo test -p fulgur-chart --test frontend_vegalite_composition --test render_vegalite_composition`

Expected: FAIL because composition root dispatch, recursive guards, and Scene lowering are absent.

- **Step 3: Integrate the public parser and recursive guards.** Add the root ChartKind and update exhaustive matches together, finalize the expanded tree with resolved scales, prefix leaf parser/guard failures with JSON paths, and enforce aggregate point/primitive limits before layout.
- **Step 4: Add the composition-aware Cartesian context and `build_vega_parts` entry points** to the listed mark layout modules. Use resolved domains and guide ownership while preserving each module's existing standalone `build` path.
- **Step 5: Implement layer Scene construction** with one common plot frame, shared/independent axis and legend ownership, per-child clipped marks, and source array paint order. Reject unsupported mark/frame combinations with their node paths.
- **Step 6: Implement recursive hconcat/vconcat layout** with `Prim::Group` translations, 20 px default spacing, inherited per-view dimensions, derived output dimensions, one title per composition node, and one root background.
- **Step 7: Propagate nested Vega-Lite image presence to raster validation** so SVG keeps `<image>` and PNG/WebP fail before rendering with the existing unsupported-image message.
- **Step 8: Run composition tests and existing renderer regressions**

Run: `cargo test -p fulgur-chart --test render_vegalite_composition`

Expected: PASS; all composition layout assertions hold.

Run: `cargo test -p fulgur-chart --test frontend_vegalite_composition --test render_vegalite_image --test render_vegalite_geoshape --test render_vegalite_error_mark --test render_vegalite_boxplot`

Expected: PASS; existing leaf rendering and raster-image behavior remain intact.

- **Step 9: Commit**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/src/frontend/vegalite_composition.rs crates/fulgur-chart/src/guard.rs crates/fulgur-chart/src/model.rs crates/fulgur-chart/src/layout/vega_composition.rs crates/fulgur-chart/src/layout/mod.rs crates/fulgur-chart/src/layout/common.rs crates/fulgur-chart/src/layout/bar.rs crates/fulgur-chart/src/layout/line.rs crates/fulgur-chart/src/layout/scatter.rs crates/fulgur-chart/src/layout/vega_rect.rs crates/fulgur-chart/src/layout/error_mark.rs crates/fulgur-chart/src/layout/vega_boxplot.rs crates/fulgur-chart/src/raster_direct.rs crates/fulgur-chart/tests/frontend_vegalite_composition.rs crates/fulgur-chart/tests/render_vegalite_composition.rs
git commit -m "feat(vegalite): render layer and concat compositions"
```

### Task 5: Examples, docs, schema parity, and native/WASM goldens

**Files:**

- Create `examples/specs/vegalite-layer.json` and `examples/specs/vegalite-nested-concat.json`.
- Modify `README.md`.
- Modify `crates/fulgur-chart/tests/render_vegalite_composition.rs`, `tests/wasm_runtime.rs`, and `tests/golden_png.rs`.
- Generate `crates/bindings/wasm/src/vegalite-schema.json` using `crates/bindings/wasm/examples/regenerate_schemas.rs`.
- Add `crates/fulgur-chart/tests/golden/vegalite-layer.png` and `vegalite-nested-concat.png`.

**Interfaces:**

- `vegalite-layer.json` uses inherited inline data and a bar+line layer with shared x/y/color.
- `vegalite-nested-concat.json` uses nested hconcat/vconcat with an inner layer and independent positional axes.
- `wasm_runtime.rs` parses both examples through `vegalite::parse` and renders each through the same SVG/PNG helpers used by native and `wasm_bindgen_test`.
- `golden_png.rs::NAMES` includes both exact example basenames; SVG snapshots are stored by `render_vegalite_composition.rs`.

- **Step 1: Add end-to-end fixture and schema parity tests** `vegalite_composition_examples_parse_in_native_and_wasm`, `vegalite_composition_svg_is_deterministic`, and `vegalite_composition_png_is_valid_on_native_and_wasm`. Assert expected nested chart types, stable SVG bytes, valid PNG signature/dimensions, and recursive unsupported errors.
- **Step 2: Run focused integration tests to verify failure**

Run: `cargo test -p fulgur-chart --test wasm_runtime vegalite_composition`

Expected: FAIL because examples and composition integration are not registered.

- **Step 3: Add the two JSON examples and document the supported operators, inheritance, resolution defaults, errors, and example links** in README's Vega-Lite section.
- **Step 4: Regenerate and verify the embedded WASM schema**

Run: `cargo run --manifest-path crates/bindings/wasm/Cargo.toml --example regenerate_schemas`

Run: `cargo test --manifest-path crates/bindings/wasm/Cargo.toml`

Expected: PASS; the checked-in `vegalite-schema.json` exactly matches the recursive Rust schema.

- **Step 5: Add both example basenames to `golden_png.rs::NAMES`, generate each PNG separately, and review the output**

Run: `UPDATE_GOLDEN=vegalite-layer cargo test -p fulgur-chart --test golden_png`

Run: `UPDATE_GOLDEN=vegalite-nested-concat cargo test -p fulgur-chart --test golden_png`

Expected: each command updates only its corresponding golden PNG; default comparison passes afterward.

- **Step 6: Run native/WASM integration tests and the workspace suite**

Run: `cargo test -p fulgur-chart --test frontend_vegalite_composition --test render_vegalite_composition --test wasm_runtime --test golden_png`

Expected: PASS, including native/WASM-shared tests and both new goldens.

Run: `wasm-pack test --node crates/fulgur-chart --test wasm_runtime`

Expected: PASS on wasm32; composition SVG and PNG runtime checks succeed and unsupported raster image composition remains an explicit error.

Run: `cargo test --workspace --exclude chart-server --locked --offline`

Expected: PASS for the complete workspace.

- **Step 7: Commit**

```bash
git add README.md examples/specs/vegalite-layer.json examples/specs/vegalite-nested-concat.json crates/fulgur-chart/tests/render_vegalite_composition.rs crates/fulgur-chart/tests/wasm_runtime.rs crates/fulgur-chart/tests/golden_png.rs crates/fulgur-chart/tests/golden/vegalite-layer.png crates/fulgur-chart/tests/golden/vegalite-nested-concat.png crates/bindings/wasm/src/vegalite-schema.json
git commit -m "docs(vegalite): add layer and concat examples"
```

---

## Plan Self-Review

- **Spec coverage:** schema recursion (Task 1); data/encoding inheritance, explicit unsupported input, paths, limits, and errors (Task 2); scale/axis/legend resolution (Tasks 3–4); layer order, independent guides, clips, concat geometry, titles/background, nested images (Task 4); examples, README, schema parity, native/WASM, and golden coverage (Task 5).
- **Step granularity:** each task starts with assertions, runs the focused test before implementation, reruns it after implementation, and commits its files. Scene layout is separated from parser/domain resolution so each has its own input and rendering assertions.
- **Type consistency:** Task 2 produces `ExpandedCompositionNode`; Task 3 resolves raw color/size domains, parses leaves, derives positional domains from parsed IR, and produces `VegaCompositionNode`; Task 4 stores that tree under the root `ChartKind::VegaComposition` and passes it to `layout::vega_composition` with a `VegaCartesianContext`.
- **Review focus coverage:** all five focus inputs are pinned in Tasks 2–4: inheritance/override, strict and non-strict rejection, incompatible shared domains, independent axis gutters/clipping, and aggregate guards.
- **Proportion:** five tasks map to schema/IR, parsing/guards, scale resolution, Scene layout, and end-to-end delivery; they do not prescribe function bodies beyond interfaces fixed by the spec.
