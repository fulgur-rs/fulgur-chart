# Vega-Lite geoshape 実装計画

> **実装者向け:** `superpowers:executing-plans` を使い、task ごとに実装する。各 step は checkbox で管理する。

**Goal:** inline GeoJSON の `mark: "geoshape"` を全 Geometry 種別・全 Vega-Lite v6 projection で描画し、choropleth を native/WASM 共通で動作させる。

**Architecture:** GeoJSON と projection 設定を専用 IR payload に保持する。純 Rust の d3-geo port で projection/clip/resample し、Fulgur の `Prim::Path`/`Prim::Circle` に変換する。Vega-Lite parser が record/Feature/FeatureCollection を解決し、機能に固有の上限を path 生成前に検査する。

**Tech Stack:** Rust 2024 / serde_json / `geo` 0.28 / `geo-types` 0.7.19 / `d3_geo_rs` 3.0.0 (`default-features = false`) / existing SVG and tiny-skia renderers。

**Spec:** [`docs/superpowers/specs/2026-09-27-fulgur-chart-312-geoshape.md`](../specs/2026-09-27-fulgur-chart-312-geoshape.md)

## Global Constraints

- 受理する Geometry は Point, MultiPoint, LineString, MultiLineString, Polygon, MultiPolygon, GeometryCollection。
- Feature / FeatureCollection / GeometryCollection を source order のまま扱い、Feature の `geometry: null` は空形状として受理する。
- inline record、Feature 配列、Feature、FeatureCollection のみを受理し、URL / TopoJSON は明確な error にする。
- projection type は Vega-Lite v6 schema の16種すべて。既定は `equalEarth`、名前は case-insensitive。
- `center`, `rotate`, `clipAngle`, `clipExtent`, `parallels`, `pointRadius`, `precision`, `scale`, `translate` と identity の `reflectX` / `reflectY` を解釈する。
- 自動 fit は plot area 内側8 pxを使い、aspect ratio を保つ。指定された projection transform 値は優先する。
- path adapter は空白区切りの `M x y`, `L x y`, `Z` のみを生成し、SVG と raster の両方に渡す。
- native/WASM は同じ parse、guard、projection、layout、scene path を通る。
- feature 上限は100,000、総座標数と出力 primitive 上限はそれぞれ1,000,000。`parse_with_limits` の caller 設定値を尊重し、projected path を確保する前に検査する。

## Review Focus

- FeatureCollection 内の null geometry と GeometryCollection の深い入れ子で panic せず、有効 geometry だけを描く。Task 1 の geometry parser test で固定する。
- 180度子午線と投影 horizon で切断される形状が非有限 path を作らない。Task 2 の projection/clip test で固定する。
- 外周・穴の ring 方向が任意でも穴が塗られない。Task 2 の polygon-hole test で固定する。
- 巨大 FeatureCollection / 座標列が projection path 確保前に拒否される。Task 1 と Task 4 の preflight test で固定する。
- 同名 color field が record と Feature properties の両方にあると record 値が優先され、欠損色は feature を塗らず stroke を残す。Task 3 の choropleth test で固定する。

---

### Task 1: GeoJSON payload と preflight parser

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`
- Create: `crates/fulgur-chart/src/geoshape.rs`
- Modify: `crates/fulgur-chart/Cargo.toml`
- Update: root `Cargo.lock` と `crates/bindings/wasm/Cargo.lock`

**Interfaces:**
- `ir::GeoShape` は `features: Vec<GeoFeature>`, `projection: GeoProjection`, `style: GeoShapeStyle` を持つ。
- `ir::GeoGeometry` は7 Geometry 種を表し、`GeoFeature` は `geometry: Option<GeoGeometry>` と解決済み `fill: Option<Color>` を持つ。
- `GeoProjectionType` は16 projection 名、`GeoProjection` は型と common projection properties、`GeoShapeStyle` は定数 fill/stroke と線幅を保持する。これらの IR 型は Task 1 で定義し、初期値は Task 3 の parser が設定する。
- `InputLimits` に `max_geo_features=100_000`, `max_geo_vertices=1_000_000`, `max_geo_primitives=1_000_000` を追加し、GeometryCollection の nesting は64までとする。
- `geoshape::parse_geojson(data: &Value, shape_field: Option<&str>, limits: &InputLimits) -> Result<Vec<RawGeoFeature>, String>` は形状と source record / Feature properties を保持する。
- `RawGeoFeature` は frontend が choropleth 色を解決するまで `geometry`, `record`, `properties` を保持する。

- [ ] **Step 1: 失敗する GeoJSON parser test を追加する**

`geoshape::tests::parses_every_geometry_and_geojson_wrapper` で全7 Geometry、Feature、FeatureCollection、GeometryCollection、Feature array、record の shape field、null geometry を検証する。

```rust
assert_eq!(parsed_geometry_kinds, ALL_SEVEN_GEOMETRY_KINDS);
assert!(null_feature.geometry.is_none());
assert_eq!(feature_collection_order, ["first", "second"]);
```

- [ ] **Step 2: parser test が失敗することを確認する**

Run: `cargo test -p fulgur-chart parses_every_geometry_and_geojson_wrapper --lib`

Expected: `GeoGeometry` / `parse_geojson` が未実装で FAIL。

- [ ] **Step 3: GeoJSON types と preflight/parser を実装する**

`ir.rs` に GeoShape payload と Geometry enum を追加する。`geoshape.rs` は source の種類、geometry nesting、position、Feature properties を検査し、feature / vertex / subgeometry 数を数えて limits を確認してから typed geometry vectors を作る。Position は2個以上の有限 ordinates を要求し、3個目以降は無視する。ring は4点以上を要求する。

依存を追加する前に `/tmp/fulgur-geoshape-dep-probe-3` の小さな crate で `d3_geo_rs = { version = "=3.0.0", default-features = false }`, `geo = { version = "~0.28", default-features = false }`, `geo-types = "=0.7.19"` の WASM build を試す。probe は `wasm32-unknown-unknown` で成功済み。`geo-types` 0.7.19 を固定し、0.7.20 が導入する thiserror 範囲と既存 workspace dependency の衝突を避ける。

- [ ] **Step 4: GeoJSON parser test を通す**

Run: `cargo test -p fulgur-chart parses_every_geometry_and_geojson_wrapper --lib`

Expected: PASS。加えて `cargo check --workspace --locked --target wasm32-unknown-unknown --exclude chart-server` が通る。

- [ ] **Step 5: malformed input と preflight の test を追加する**

`geoshape::tests::rejects_malformed_coordinates_with_path` と `geoshape::tests::preflight_rejects_feature_vertex_and_part_limits` を追加し、欠損 ordinate、壊れた nesting、未閉鎖または4点未満の ring、深さ上限64を超える GeometryCollection、Feature/vertex/part 上限超過が path-aware error になることを検証する。GeoShape 用の3上限は `InputLimits` に追加し、既定値を feature 100,000 / vertex 1,000,000 / primitive 1,000,000 とする。

- [ ] **Step 6: parser tests を通し、Task 1 を commit する**

Run: `cargo test -p fulgur-chart geoshape::tests --lib`

Expected: PASS。Commit: `feat: add guarded GeoJSON geoshape model`。

### Task 2: 全 projection と renderer-compatible path adapter

**Files:**
- Modify: `crates/fulgur-chart/src/geoshape.rs`
- Create: `crates/fulgur-chart/src/layout/geoshape.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Modify: `crates/fulgur-chart/src/model.rs`
- Modify: `crates/fulgur-chart/src/guard.rs`

**Interfaces:**
- `ir::ChartKind::GeoShape { data: Box<ir::GeoShape> }` を追加し、layout/model/guard の dispatch に登録する。
- `GeoProjectionType` は Vega-Lite v6 の16名を列挙する。
- `GeoProjection` は共通 projection properties と identity reflection を保持する。
- `geoshape::project_features(shape: &GeoShape, viewport: ClipRect) -> Result<Vec<ProjectedFeature>, String>` は全 Feature 共通の auto-fit を解決して path/point geometry を返し、source order と feature fill を維持する。
- `layout::geoshape::build(spec: &ChartSpec, m: &TextMeasurer) -> Scene` は既存 Scene primitives のみを生成する。

- [ ] **Step 1: projection test を追加する**

`geoshape::tests::all_vl_projection_types_return_finite_coordinates` で16名を表にし、通常の投影は赤道近傍、`albersUsa` は米国内形状を使う。各出力 coordinate が有限であることを確認する。

```rust
assert_eq!(PROJECTION_TYPES.len(), 16);
assert!(projected.iter().flatten().all(|(x, y)| x.is_finite() && y.is_finite()));
```

- [ ] **Step 2: projection test が失敗することを確認する**

Run: `cargo test -p fulgur-chart all_vl_projection_types_return_finite_coordinates --lib`

Expected: projection dispatcher が未実装で FAIL。

- [ ] **Step 3: projection, clipping, fit, path endpoint を実装する**

`d3_geo_rs` の streaming pipeline を使い、独自 path endpoint から Fulgur の空白区切り数値 token を直接出す。16 type を明示 dispatch し、backend にない raw formula はローカル projector として追加する。Identity は planar coordinates、scale/translate、reflectX/reflectY を適用する。scale と translate がどちらも未指定の場合に自動 fit し、指定された値はそのまま優先する。自動 fit は全可視 feature の bounds を使い8 px marginを確保する。projection result に非有限値があれば path 作成前に error にする。

- [ ] **Step 4: projection test を通す**

Run: `cargo test -p fulgur-chart all_vl_projection_types_return_finite_coordinates --lib`

Expected: PASS。

- [ ] **Step 5: clipping / fit / ring test を追加する**

`layout::geoshape::tests::auto_fit_contains_all_features_with_margin`, `geoshape::tests::clips_projection_horizon_without_non_finite_path`, `layout::geoshape::tests::normalizes_polygon_holes_and_preserves_feature_order`, `geoshape::tests::precision_zero_is_bounded_by_the_projected_vertex_limit` を追加する。Hole は外周と反対 winding、source feature 順は出力順、fit bounds は viewport から各辺8 px以上内側を assert する。`precision: 0` は resampler の最悪出力を事前見積もりし、上限超過なら投影前に拒否する。

- [ ] **Step 6: direct IR guard test を追加する**

`guard::tests::geoshape_guard_rejects_feature_vertex_and_primitive_over_limits` は手作り `ChartKind::GeoShape` を `validate_spec(&spec, &limits)` に渡し、feature count、座標数、primitive count の各 cap を個別に超えるケースが対応するエラーで拒否されることを確認する。

- [ ] **Step 7: Scene layout と projection tests を通して commit する**

`layout::geoshape::build` は no-axis view bounds を作り、title、`Prim::Path`、`Prim::Circle` を追加する。Open line に fill を付けず、polygon holes は nonzero winding に正規化する。`model.rs` は type `geoshape` と feature count を返す。`guard.rs` は ChartKind payload の整合性・feature/vertex/primitive caps を再検査する。

Run: `cargo test -p fulgur-chart geoshape --lib`

Expected: PASS。Commit: `feat: project and render GeoJSON geoshapes`。

### Task 3: Vega-Lite schema/parser と choropleth 色

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`
- Modify: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- `VlGeoshapeSpec` は geoshape mark、geo data、encoding、projection、通常の title/size/background fields を定義する。
- `VlGeoShapeEncoding` は optional `shape` geojson field と optional `color` channel を持つ。
- `parse_geoshape_kind` は `RawGeoFeature` と color type/palette を受け、`ChartKind::GeoShape` を返す。
- quantitative fill は existing VegaRect white-to-high interpolation、nominal/ordinal fill は `VEGALITE_PALETTE` の first-seen mapping を使う。

- [ ] **Step 1: typed schema と parser の失敗 test を追加する**

`tests/frontend_vegalite.rs` に `geoshape_schema_accepts_geojson_encoding_and_all_projection_names`, `geoshape_parses_record_and_feature_collection_inputs`, `geoshape_choropleth_uses_record_before_feature_properties`, `geoshape_rejects_url_topojson_and_unknown_projection` を追加する。`shape.type="geojson"`、16 type、center/rotate/clipAngle/clipExtent/parallels/pointRadius/precision/scale/translate と identity reflect 設定、quantitative/nominal colors、constant fill precedence、strict/non-strict のエラーを assert する。

- [ ] **Step 2: parser tests が失敗することを確認する**

Run: `cargo test -p fulgur-chart --test frontend_vegalite geoshape_`

Expected: geoshape mark が未対応で FAIL。

- [ ] **Step 3: typed schema variant と strict parser を追加する**

`VegaLiteSpec::GeoShape`、`VlGeoShapeSpec`, `VlGeoData`, `VlGeoShapeEncoding`, `VlGeoShapeChannel`, color type、16-name projection enum を追加する。`parse_mark`, `check_unknown_keys`, parser dispatch で top-level `projection`, `encoding.shape`, mark style, inline Feature/FeatureCollection を受理する。URL と `data.format.type="topojson"` は詳細な error にする。

- [ ] **Step 4: record/Feature properties と choropleth 色を実装する**

`parse_geoshape_kind` で row field を優先し、未解決なら Feature properties field を参照する。Quantitative は data domain で continuous fill、nominal/ordinal は first-seen palette index とし、欠損色は color field が指定された場合に fill を省いて geometry/stroke を保つ。色 encoding がない場合は mark fill、次に mark color、最後に Vega-Lite geoshape の既定色 `#4682b4` を使う。色 channel の `value` は定数 fill として受理し、同一 channel の `field` と併記された場合はエラーにする。`mark.stroke` は独立した stroke とする。

- [ ] **Step 5: parser/schema/color tests を通して commit する**

Run: `cargo test -p fulgur-chart --test frontend_vegalite geoshape_`

Expected: PASS。加えて `cargo test -p fulgur-chart --test inspect_model` で `meta.type="geoshape"` を確認する。Commit: `feat: parse Vega-Lite geoshape and choropleths`。

### Task 4: public regression fixture、golden、native/WASM 確認

**Files:**
- Create: `examples/specs/vegalite_geoshape.json`
- Modify: `README.md`
- Create: `crates/fulgur-chart/tests/render_vegalite_geoshape.rs`
- Modify: `crates/fulgur-chart/tests/golden_png.rs`
- Modify: `crates/fulgur-chart/tests/wasm_runtime.rs`
- Create: `crates/fulgur-chart/tests/snapshots/render_vegalite_geoshape__geoshape_snapshot.snap`
- Create: `crates/fulgur-chart/tests/golden/vegalite_geoshape.png`

**Interfaces:**
- Consumes: `ChartKind::GeoShape`, the common `vegalite_geoshape.json` fixture, and `vegalite::parse`.
- Produces: pinned SVG snapshot, PNG pixel golden, and native/WASM render smoke coverage for the same fixture.

- [ ] **Step 1: inline choropleth fixture と rendering test を追加する**

`tests/frontend_vegalite.rs` に `geoshape_fixture_renders_svg_and_png`、`tests/render_vegalite_geoshape.rs` に SVG snapshot test を追加する。fixture は polygon hole と quantitative Feature property を含め、SVG に finite `path`、PNG decode 成功、期待寸法を assert する。SVG は `insta::assert_snapshot!`、PNG は pixel golden で固定する。

- [ ] **Step 2: rendering test を通し、SVG/PNG goldens を生成する**

Run: `cargo test -p fulgur-chart --test frontend_vegalite geoshape_fixture_renders_svg_and_png`

Expected: PASS。SVG snapshot: `INSTA_UPDATE=always cargo test -p fulgur-chart --test render_vegalite_geoshape`。PNG golden: `UPDATE_GOLDEN=vegalite_geoshape cargo test -p fulgur-chart --test golden_png golden_png_matches`。

- [ ] **Step 3: WASM runtime の同一 fixture test と docs を追加する**

`wasm_runtime.rs` に同じ fixture の `render_chart` と `render_chart_to_png_default` smoke/determinism test を加える。README の Vega-Lite supported subset と inline GeoJSON example を更新する。

- [ ] **Step 4: 全 quality gate を実行する**

Run:
`cargo fmt --all -- --check`
`cargo test --workspace --locked`
`cargo clippy --workspace --all-targets --locked -- -D warnings`
`cargo check --workspace --locked --target wasm32-unknown-unknown --exclude chart-server`
`wasm-pack test --node crates/fulgur-chart --test wasm_runtime`

Expected: 全 command PASS。golden が変更された場合は該当 fixture のみを再生成し、差分を確認する。

- [ ] **Step 5: Task 4 を commit する**

Commit: `test: cover Vega-Lite geoshape rendering on native and wasm`。

## Handoff

各 task の commit 後、worktree の全差分を確認する。全 quality gate が通ったら PR を作成し、CI green を確認して通常 merge、merge 後に worktree を削除する。
