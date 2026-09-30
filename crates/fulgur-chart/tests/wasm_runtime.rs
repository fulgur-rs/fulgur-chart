//! wasm32-unknown-unknown ランタイム検証。
//!
//! 目的: 「ビルドが通る」だけでなく、実際に wasm 上で SVG/PNG を生成し、
//! フォントロードとラスタライズが wasm ランタイムで panic せず動くこと、
//! および決定論が保たれることを確認する。同一テストを native(`#[test]`)と
//! wasm(`#[wasm_bindgen_test]`)の両方で走らせる。
//!
//! ## このプロジェクトの決定論の前提（重要）
//! - SVG は cross-platform で byte 決定的。`render_*` のスナップショット(insta)が全 OS の
//!   CI マトリクスで exact 一致して green なのが根拠。SVG は全プラットフォーム共通で exact
//!   比較してよい。
//! - PNG(tiny-skia ラスタライズ)は浮動小数/AA のプラットフォーム差があり、native 間でも byte
//!   一致しない(`golden_png.rs` がピクセル許容差で比較している理由)。よって PNG の exact byte
//!   比較は同一プラットフォーム間でしか成立しない。全 OS 共通テストでは「有効な PNG・期待寸法」
//!   までを検証し、exact 比較は wasm 限定テストに隔離する(CI の wasm ジョブは
//!   ubuntu=linux-x86_64 なので ubuntu native と byte 一致する)。OS 跨ぎの視覚一致は
//!   `golden_png.rs` の許容差比較が担保する。
//!
//! 実行:
//!   native: cargo test -p fulgur-chart --test wasm_runtime
//!   wasm:   wasm-pack test --node crates/fulgur-chart --test wasm_runtime

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::{chartjs, vegalite};
use fulgur_chart::ir::VegaCompositionNode;
use fulgur_chart::raster_direct::{render_chart_to_png_default, render_chart_to_webp};
use fulgur_chart::render::render_chart;

/// 依存なしの決定論的ハッシュ(FNV-1a 64bit)。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// 軸・ラベル・棒(=テキストグリフ輪郭 + 塗り)を含む非自明な spec。
/// bar チャートは超越関数(sin/cos)を経由しないため SVG は f64 演算 + 文字列化のみで
/// 決定的になり、cross-platform で byte 一致する。
fn sample_spec() -> fulgur_chart::ir::ChartSpec {
    let json = r#"{
        "type": "bar",
        "data": {
            "labels": ["Mon", "Tue", "Wed", "Thu"],
            "datasets": [{
                "label": "Sales",
                "backgroundColor": "rgba(54, 162, 235, 0.7)",
                "borderColor": "rgb(54, 162, 235)",
                "data": [12, 19, 7, 15]
            }]
        }
    }"#;
    chartjs::parse(json, false).expect("spec parses")
}

fn sample_geoshape_spec() -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(
        include_str!("../../../examples/specs/vegalite_geoshape.json"),
        true,
    )
    .expect("geoshape fixture parses")
}

fn sample_boxplot_spec() -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(
        include_str!("../../../examples/specs/vegalite-boxplot.json"),
        true,
    )
    .expect("boxplot fixture parses")
}

fn sample_image_spec() -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(
        include_str!("../../../examples/specs/vegalite-image.json"),
        true,
    )
    .expect("image fixture parses")
}

fn composition_examples() -> [(&'static str, &'static str); 2] {
    [
        (
            "vegalite-layer",
            include_str!("../../../examples/specs/vegalite-layer.json"),
        ),
        (
            "vegalite-nested-concat",
            include_str!("../../../examples/specs/vegalite-nested-concat.json"),
        ),
    ]
}

fn composition_leaf_count(node: &VegaCompositionNode) -> usize {
    match node {
        VegaCompositionNode::Unit(_) => 1,
        VegaCompositionNode::Layer(layer) => {
            layer.children.iter().map(composition_leaf_count).sum()
        }
        VegaCompositionNode::HConcat(concat) | VegaCompositionNode::VConcat(concat) => {
            concat.children.iter().map(composition_leaf_count).sum()
        }
    }
}

const PNG_SCALE: f32 = 2.0;
const PNG_SIGNATURE: &[u8; 8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
// デフォルト 800x450 を PNG_SCALE 倍した寸法。
const PNG_WIDTH: u32 = 1600;
const PNG_HEIGHT: u32 = 900;

/// Recursive composition parsing accepts both checked-in examples through the native/WASM API.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn vegalite_composition_examples_parse_in_native_and_wasm() {
    for (name, json) in composition_examples() {
        let spec = vegalite::parse(json, true)
            .unwrap_or_else(|error| panic!("{name} composition example did not parse: {error}"));
        let fulgur_chart::ir::ChartKind::VegaComposition(root) = &spec.kind else {
            panic!("{name} must parse to recursive composition IR");
        };
        let expected_leaves = if name == "vegalite-layer" { 2 } else { 4 };
        assert_eq!(composition_leaf_count(root), expected_leaves, "{name}");
        if name == "vegalite-nested-concat" {
            assert!(matches!(root.as_ref(), VegaCompositionNode::VConcat(_)));
            let VegaCompositionNode::VConcat(vconcat) = root.as_ref() else {
                unreachable!()
            };
            assert!(matches!(
                vconcat.children[0],
                VegaCompositionNode::HConcat(_)
            ));
            assert!(matches!(vconcat.children[1], VegaCompositionNode::Unit(_)));
        }
    }
}

/// Composition examples use the same deterministic SVG path on native and WASM.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn vegalite_composition_svg_is_deterministic() {
    for (name, json) in composition_examples() {
        let spec = vegalite::parse(json, true)
            .unwrap_or_else(|error| panic!("{name} composition example did not parse: {error}"));
        let svg = render_chart(&spec);
        assert_eq!(
            svg,
            render_chart(&spec),
            "{name} SVG should be deterministic"
        );
        let expected = match name {
            "vegalite-layer" => (3_863, 0xedd1_3312_bdc6_573f),
            "vegalite-nested-concat" => (10_233, 0xce15_09f8_b918_aed8),
            _ => unreachable!("composition fixture list is fixed"),
        };
        assert_eq!(
            (svg.len(), fnv1a(svg.as_bytes())),
            expected,
            "{name} SVG must match the checked native/WASM reference bytes"
        );
        assert!(svg.starts_with("<svg"), "{name} did not render SVG");
        assert!(
            svg.contains("<path") || svg.contains("<rect"),
            "{name} has no marks"
        );
        let lower = svg.to_ascii_lowercase();
        assert!(
            !lower.contains("nan") && !lower.contains("inf"),
            "{name}: {svg}"
        );
    }
}

/// Composition PNGs decode with the dimensions reported by their recursively composed scenes.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn vegalite_composition_png_is_valid_on_native_and_wasm() {
    for (name, json) in composition_examples() {
        let spec = vegalite::parse(json, true)
            .unwrap_or_else(|error| panic!("{name} composition example did not parse: {error}"));
        let png = render_chart_to_png_default(&spec, 1.0)
            .unwrap_or_else(|error| panic!("{name} PNG render failed: {error}"));
        assert_eq!(&png[..8], PNG_SIGNATURE, "{name} PNG signature is invalid");
        let pixmap = tiny_skia::Pixmap::decode_png(&png)
            .unwrap_or_else(|error| panic!("{name} PNG failed to decode: {error}"));
        assert_eq!(
            (pixmap.width(), pixmap.height()),
            (spec.width as u32, spec.height as u32),
            "{name} PNG dimensions must match the composed view"
        );
    }
}

/// Unsupported descendants keep stable paths in both strict and permissive parser modes.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn vegalite_composition_errors_are_path_qualified_on_native_and_wasm() {
    let unsupported = r#"{
      "vconcat":[{"layer":[{"mark":"bar","transform":[{"filter":"datum.x > 0"}]}]}]
    }"#;
    for strict in [false, true] {
        let error = vegalite::parse(unsupported, strict).unwrap_err();
        assert!(
            error.contains("vconcat[0].layer[0].transform"),
            "strict={strict}: {error}"
        );
    }

    let image = vegalite::parse(
        r#"{
          "hconcat":[
            {"mark":{"type":"image","width":12,"height":8},"data":{"values":[{"x":1,"y":2,"src":"https://example.test/a.png"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"y","type":"quantitative"},"url":{"field":"src"}}}
          ]
        }"#,
        true,
    )
    .expect("composed image spec parses");
    let error = render_chart_to_png_default(&image, 1.0).unwrap_err();
    assert!(
        error.contains("image marks") && error.contains("SVG"),
        "{error}"
    );
}

// SVG は cross-platform 決定的なので全プラットフォーム共通の期待値。
// (native の linux-x86_64 で観測。SVG の決定論は insta スナップショットが全 OS で実証。)
const SVG_LEN: usize = 3638;
const SVG_HASH: u64 = 0x9745_8add_b99c_4293;

// PNG の exact 期待値は wasm32 と linux-x86_64 native の同一レンダリング環境用。
// それ以外の native では cfg で除外する
// (dead_code 警告回避)。
// 既定圧縮 Balanced(fdeflate + 適応フィルタ)での値。Fast/High に既定を変えた場合は再生成すること。
#[cfg(any(
    target_arch = "wasm32",
    all(target_os = "linux", target_arch = "x86_64")
))]
const PNG_LEN_LINUX_X86: usize = 44422;
#[cfg(any(
    target_arch = "wasm32",
    all(target_os = "linux", target_arch = "x86_64")
))]
const PNG_HASH_LINUX_X86: u64 = 0x13ba_b2c2_d628_27a9;

/// SVG: 全プラットフォーム共通で exact byte 一致を検証(cross-platform 決定的)。
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn svg_is_byte_identical_across_platforms() {
    let svg = render_chart(&sample_spec());
    assert!(svg.starts_with("<svg"), "出力が SVG ではない");
    assert_eq!(svg.len(), SVG_LEN, "SVG 長が期待値と不一致");
    assert_eq!(
        fnv1a(svg.as_bytes()),
        SVG_HASH,
        "SVG byte が期待値と不一致(cross-platform 決定論の破れ)"
    );
}

/// PNG: 全プラットフォーム共通の不変条件。wasm ランタイムでラスタライズが panic せず
/// 完走し、有効な PNG と期待寸法を返すこと。浮動小数差を主張しないので OS 跨ぎでも安全。
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn png_renders_validly_on_every_platform() {
    let png = render_chart_to_png_default(&sample_spec(), PNG_SCALE).expect("PNG 生成成功");
    assert_eq!(&png[..8], PNG_SIGNATURE, "PNG シグネチャ不正");
    let pix = tiny_skia::Pixmap::decode_png(&png).expect("生成 PNG がデコード可能");
    assert_eq!(
        (pix.width(), pix.height()),
        (PNG_WIDTH, PNG_HEIGHT),
        "PNG 寸法が期待値と不一致"
    );
}

/// Vega-Lite GeoJSON uses the shared native/WASM projection, path and raster pipelines.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn geoshape_svg_and_png_render_validly_and_deterministically() {
    let spec = sample_geoshape_spec();
    let svg = render_chart(&spec);
    let svg_again = render_chart(&spec);
    assert_eq!(svg, svg_again, "geoshape SVG should be deterministic");
    assert!(svg.contains("<path") && svg.contains("d=\"M "));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));

    let png = render_chart_to_png_default(&spec, 1.0).expect("geoshape PNG 生成成功");
    let png_again = render_chart_to_png_default(&spec, 1.0).expect("geoshape PNG 生成成功(2回目)");
    assert_eq!(png, png_again, "geoshape PNG should be deterministic");
    assert_eq!(&png[..8], PNG_SIGNATURE);
    let image = tiny_skia::Pixmap::decode_png(&png).expect("生成 PNG がデコード可能");
    assert_eq!((image.width(), image.height()), (480, 280));
}

/// Image URLs pass through the same synchronous SVG path on native and WASM; raster output is rejected.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn image_mark_emits_svg_references_and_rejects_raster_output() {
    let spec = sample_image_spec();
    let svg = render_chart(&spec);
    assert_eq!(svg.matches("<image ").count(), 2);
    assert!(svg.contains("href=\"data:image/svg+xml,"));

    let png_error = render_chart_to_png_default(&spec, 1.0).unwrap_err();
    assert!(png_error.contains("image marks") && png_error.contains("SVG"));
    let webp_error = render_chart_to_webp(&spec, 1.0, DEFAULT_FONT).unwrap_err();
    assert!(webp_error.contains("image marks") && webp_error.contains("SVG"));
}

/// Projection failures travel back as render errors in native and WASM builds.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn geoshape_projection_errors_are_returned_on_every_platform() {
    let spec = vegalite::parse(
        r#"{
            "mark":"geoshape",
            "data":{"values":[{"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[2,0]}}]},
            "encoding":{},
            "projection":{"type":"identity","scale":1.7976931348623157e308}
        }"#,
        true,
    )
    .expect("geoshape spec parses");
    let error = fulgur_chart::render::render_chart_with_limits(
        &spec,
        &fulgur_chart::guard::InputLimits::default(),
    )
    .expect_err("non-finite projected coordinates should be a render error");
    assert!(error.contains("non-finite"), "{error}");
}

fn error_mark_fixtures() -> [(&'static str, &'static str); 4] {
    [
        (
            "vegalite-errorbar-raw",
            include_str!("../../../examples/specs/vegalite-errorbar-raw.json"),
        ),
        (
            "vegalite-errorbar-preaggregated",
            include_str!("../../../examples/specs/vegalite-errorbar-preaggregated.json"),
        ),
        (
            "vegalite-errorband-raw",
            include_str!("../../../examples/specs/vegalite-errorband-raw.json"),
        ),
        (
            "vegalite-errorband-preaggregated",
            include_str!("../../../examples/specs/vegalite-errorband-preaggregated.json"),
        ),
    ]
}

/// Error mark examples parse and use the same Scene geometry on native and wasm32.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn error_mark_examples_render_svg_and_png_deterministically() {
    for (name, json) in error_mark_fixtures() {
        let spec = vegalite::parse(json, true)
            .unwrap_or_else(|error| panic!("{name} error mark example did not parse: {error}"));
        let svg = render_chart(&spec);
        let svg_again = render_chart(&spec);
        assert_eq!(svg, svg_again, "{name} SVG should be deterministic");
        assert!(svg.starts_with("<svg"), "{name} did not render SVG");
        let lower_svg = svg.to_ascii_lowercase();
        assert!(
            !lower_svg.contains("nan") && !lower_svg.contains("inf"),
            "{name} SVG contains a non-finite coordinate"
        );
        if name.contains("errorbar") {
            assert!(
                svg.contains("<line"),
                "{name} has no errorbar line geometry"
            );
        } else {
            assert!(
                svg.contains("d=\"M "),
                "{name} has no errorband path geometry"
            );
        }

        let png = render_chart_to_png_default(&spec, 1.0)
            .unwrap_or_else(|error| panic!("{name} PNG render failed: {error}"));
        let png_again = render_chart_to_png_default(&spec, 1.0)
            .unwrap_or_else(|error| panic!("{name} PNG rerender failed: {error}"));
        assert_eq!(png, png_again, "{name} PNG should be deterministic");
        assert_eq!(&png[..8], PNG_SIGNATURE, "{name} PNG signature is invalid");
        let pixmap = tiny_skia::Pixmap::decode_png(&png)
            .unwrap_or_else(|error| panic!("{name} PNG failed to decode: {error}"));
        assert_eq!(
            (pixmap.width(), pixmap.height()),
            (480, 280),
            "{name} PNG dimensions are invalid"
        );
    }
}

/// Vega-Lite boxplot example uses the shared native/WASM statistics and Scene path.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn vegalite_boxplot_example_renders_deterministic_svg_and_png() {
    let spec = sample_boxplot_spec();
    let svg = render_chart(&spec);
    let svg_again = render_chart(&spec);
    assert_eq!(svg, svg_again, "boxplot SVG should be deterministic");
    assert!(svg.starts_with("<svg"), "boxplot did not render SVG");
    assert!(svg.contains("<rect"), "boxplot has no box geometry");
    assert!(svg.contains("<line"), "boxplot has no whisker geometry");
    assert!(svg.contains("<circle"), "boxplot has no outlier geometry");
    let lower_svg = svg.to_ascii_lowercase();
    assert!(
        !lower_svg.contains("nan") && !lower_svg.contains("inf"),
        "boxplot SVG contains a non-finite coordinate"
    );

    let png = render_chart_to_png_default(&spec, 1.0).expect("boxplot PNG 生成成功");
    let png_again = render_chart_to_png_default(&spec, 1.0).expect("boxplot PNG 生成成功(2回目)");
    assert_eq!(png, png_again, "boxplot PNG should be deterministic");
    assert_eq!(&png[..8], PNG_SIGNATURE, "boxplot PNG signature is invalid");
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("boxplot PNG decodes");
    assert_eq!(
        (pixmap.width(), pixmap.height()),
        (480, 280),
        "boxplot example dimensions are invalid"
    );
}

/// PNG: wasm32 と linux-x86_64 native で、linux-x86_64 の期待 byte と一致することを検証する。
/// CI の wasm ジョブは ubuntu で走るため、ubuntu native と同一ビットになる。
/// tiny-skia の浮動小数差は OS 跨ぎで出るため、この exact 比較は上記対象に限定する。
/// OS 跨ぎの視覚一致は `golden_png.rs` の許容差比較が担保する。
#[cfg(any(
    target_arch = "wasm32",
    all(target_os = "linux", target_arch = "x86_64")
))]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn png_is_byte_identical_to_linux_x86_native() {
    let png = render_chart_to_png_default(&sample_spec(), PNG_SCALE).expect("PNG 生成成功");
    assert_eq!(
        png.len(),
        PNG_LEN_LINUX_X86,
        "PNG 長が linux-x86 native と不一致"
    );
    assert_eq!(
        fnv1a(&png),
        PNG_HASH_LINUX_X86,
        "PNG byte が linux-x86 native と不一致"
    );
}

/// stamp cache 経路(>=128 均一マーカー)を通す非自明な scatter spec(200 点)。
/// per-point 色/半径を持たないため全マーカーが stamp 化される(run=200 >= 閾値 128)。
fn sample_stamp_spec() -> fulgur_chart::ir::ChartSpec {
    let pts: Vec<String> = (0..200)
        .map(|i| format!(r#"{{"x":{},"y":{}}}"#, i, (i * 37 + 13) % 100))
        .collect();
    let json = format!(
        r#"{{"type":"scatter","data":{{"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        pts.join(",")
    );
    chartjs::parse(&json, false).expect("stamp spec parses")
}

/// stamp 経路の PNG: 全プラットフォーム共通の不変条件。wasm でラスタライズが panic せず
/// 完走し、有効な PNG・期待寸法を返し、かつ同一入力で 2 回 byte 一致(決定的)であること。
/// stamp build は既存 fill_path と同エンジン、blit は整数演算なので決定論は保たれる。
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn stamp_png_renders_validly_and_deterministically() {
    let spec = sample_stamp_spec();
    let png = render_chart_to_png_default(&spec, PNG_SCALE).expect("stamp PNG 生成成功");
    let png2 = render_chart_to_png_default(&spec, PNG_SCALE).expect("stamp PNG 生成成功(2回目)");
    assert_eq!(
        png, png2,
        "stamp 経路 PNG が決定的でない(同一入力で byte 不一致)"
    );
    assert_eq!(&png[..8], PNG_SIGNATURE, "PNG シグネチャ不正");
    let pix = tiny_skia::Pixmap::decode_png(&png).expect("生成 PNG がデコード可能");
    assert_eq!(
        (pix.width(), pix.height()),
        (PNG_WIDTH, PNG_HEIGHT),
        "stamp PNG 寸法が期待値と不一致"
    );
}

// stamp 経路 PNG の linux-x86 native 期待値(既存 PNG_*_LINUX_X86 と同じ理由で wasm 限定)。
// linux-x86_64 native で生成。既定圧縮 Balanced。圧縮設定や stamp 既定(B/閾値)を変えたら再生成。
#[cfg(target_arch = "wasm32")]
const STAMP_PNG_LEN_LINUX_X86: usize = 133447;
#[cfg(target_arch = "wasm32")]
const STAMP_PNG_HASH_LINUX_X86: u64 = 0x96f2_1e67_6ffb_146d;

/// stamp 経路の PNG が linux-x86 native と byte 一致(wasm=ubuntu と native の決定論)。
/// stamp build(fill_path) + 整数 blit が wasm でも native と同一ビットを生むことを担保する。
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn stamp_png_byte_identical_to_linux_x86_native() {
    let png =
        render_chart_to_png_default(&sample_stamp_spec(), PNG_SCALE).expect("stamp PNG 生成成功");
    assert_eq!(
        png.len(),
        STAMP_PNG_LEN_LINUX_X86,
        "stamp PNG 長が linux-x86 native と不一致"
    );
    assert_eq!(
        fnv1a(&png),
        STAMP_PNG_HASH_LINUX_X86,
        "stamp PNG byte が linux-x86 native と不一致"
    );
}

/// stamp 経路は WebP でも踏まれる(`render_chart_to_webp` も `scene_to_pixmap` を共有)。
/// WebP の stamp 出力が wasm で panic せず完走し、有効な WebP を返し、かつ同一入力で
/// 2 回 byte 一致(決定的)であることを検証する。WebP の cross-platform byte 一致は
/// PNG 同様 OS 跨ぎでは成立しない(別途 golden が担保)ため、ここでは同一プラットフォーム
/// 上の決定性に絞る。
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn stamp_webp_renders_validly_and_deterministically() {
    let spec = sample_stamp_spec();
    let a = render_chart_to_webp(&spec, PNG_SCALE, DEFAULT_FONT).expect("stamp WebP 生成成功");
    let b =
        render_chart_to_webp(&spec, PNG_SCALE, DEFAULT_FONT).expect("stamp WebP 生成成功(2回目)");
    assert_eq!(
        a, b,
        "stamp 経路 WebP が決定的でない(同一入力で byte 不一致)"
    );
    // RIFF コンテナ + WEBP fourcc は予備チェック。
    assert!(a.len() > 12, "WebP が短すぎる");
    assert_eq!(&a[0..4], b"RIFF", "WebP RIFF ヘッダ不正");
    assert_eq!(&a[8..12], b"WEBP", "WebP fourcc 不正");
    // 実デコードまで行い、ヘッダだけ正しい壊れた WebP を弾く
    // (png_renders_validly_on_every_platform と同様の round-trip 検証)。
    let img = image::load_from_memory_with_format(&a, image::ImageFormat::WebP)
        .expect("生成 WebP がデコード可能")
        .to_rgba8();
    assert_eq!(
        (img.width(), img.height()),
        (PNG_WIDTH, PNG_HEIGHT),
        "WebP 寸法が期待値と不一致"
    );
}

/// Chart.js title/subtitle shared layout survives the wasm32 SVG and direct-raster paths.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn chartjs_title_subtitle_render_through_wasm() {
    let json = r##"{
        "type":"bar",
        "data":{"labels":["A","B","C"],"datasets":[{"data":[3,2,1]}]},
        "options":{"plugins":{
            "title":{"display":true,"text":["WASM title line one","WASM title line two"],"color":"#123456","font":{"size":20,"weight":"700"},"padding":0},
            "subtitle":{"display":true,"text":"WASM subtitle","align":"end","position":"bottom","color":"#654321","font":{"size":12},"padding":0}
        }}
    }"##;
    let spec = chartjs::parse(json, false).expect("WASM title spec parses");

    let svg = render_chart(&spec);
    assert!(svg.contains("WASM title line one"));
    assert!(svg.contains("WASM title line two"));
    assert!(svg.contains("WASM subtitle"));
    assert!(svg.contains("fill=\"#123456\""));
    assert!(svg.contains("fill=\"#654321\""));

    let png = render_chart_to_png_default(&spec, 1.0).expect("WASM title PNG renders");
    assert_eq!(
        &png[..8],
        PNG_SIGNATURE,
        "WASM title PNG signature is invalid"
    );
    let pixmap = tiny_skia::Pixmap::decode_png(&png).expect("WASM title PNG decodes");
    assert_eq!(
        (pixmap.width(), pixmap.height()),
        (800, 450),
        "WASM title PNG dimensions are unchanged for Canvas sizing"
    );
}
