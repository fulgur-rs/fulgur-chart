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

use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png_default;
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

const PNG_SCALE: f32 = 2.0;
const PNG_SIGNATURE: &[u8; 8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
// デフォルト 800x450 を PNG_SCALE 倍した寸法。
const PNG_WIDTH: u32 = 1600;
const PNG_HEIGHT: u32 = 900;

// SVG は cross-platform 決定的なので全プラットフォーム共通の期待値。
// (native の linux-x86_64 で観測。SVG の決定論は insta スナップショットが全 OS で実証。)
const SVG_LEN: usize = 3483;
const SVG_HASH: u64 = 0x3af5_841b_b6bb_3b8e;

// PNG の exact 期待値は「同一プラットフォーム(linux-x86_64)」専用。wasm 限定テストでのみ使う。
// native ビルドでは未使用になるため cfg で除外する(dead_code 警告回避)。
// 既定圧縮 Balanced(fdeflate + 適応フィルタ)での値。Fast/High に既定を変えた場合は再生成すること。
#[cfg(target_arch = "wasm32")]
const PNG_LEN_LINUX_X86: usize = 42630;
#[cfg(target_arch = "wasm32")]
const PNG_HASH_LINUX_X86: u64 = 0x6ee4_ac9e_6d16_b2e2;

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

/// PNG: wasm の出力が同一プラットフォーム(linux-x86_64)の native と byte 一致することを
/// 検証する。CI の wasm ジョブは ubuntu で走るため、ubuntu native と同一ビットになる。
/// tiny-skia の浮動小数差は OS 跨ぎで出るため、この exact 比較は wasm(=ubuntu) 限定。
/// OS 跨ぎの視覚一致は `golden_png.rs` の許容差比較が担保する。
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
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
