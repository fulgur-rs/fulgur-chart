//! golden PNG 比較テスト。代表 spec を PNG にラスタライズし、コミット済みの
//! golden PNG とピクセル許容差で比較する。tiny-skia の AA・浮動小数の
//! プラットフォーム差を吸収しつつ、実害のある視覚回帰は検出する。
//!
//! golden の再生成は環境変数 `UPDATE_GOLDEN`（任意の値）で行う:
//!   UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png
//!
//! レンダラ変更時は意図的に UPDATE_GOLDEN=1 で再生成してから commit する。

use std::path::PathBuf;

use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png;
use tiny_skia::Pixmap;

/// 比較対象の代表 spec 名（examples/specs/<name>.json）。
const NAMES: &[&str] = &[
    "bar",
    "line",
    "area",
    "pie",
    "line_decimated",
    "line_decimated_lttb",
    "line_with_null",
    "bar_with_null",
    "boxplot_with_null",
];

/// 1 チャンネルあたりの絶対差がこの値を超えたら「差分ピクセル」と数える。
const CHANNEL_TOLERANCE: i16 = 4;

/// 全ピクセルに占める差分ピクセルの許容割合（0.5%）。
const MAX_DIFF_FRAC: f64 = 0.005;

/// spec JSON のパス。CARGO_MANIFEST_DIR は crates/fulgur-chart なので
/// ../../ でリポジトリルートへ戻り examples/specs を指す。
fn spec_path(name: &str) -> String {
    format!(
        "{}/../../examples/specs/{}.json",
        env!("CARGO_MANIFEST_DIR"),
        name
    )
}

/// golden PNG のパス（crates/fulgur-chart/tests/golden/<name>.png）。
fn golden_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("golden");
    p.push(format!("{name}.png"));
    p
}

/// spec を読み込み・parse・render し、scale 1.0 で PNG バイト列を返す。
fn render_to_png(name: &str) -> Vec<u8> {
    let path = spec_path(name);
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("spec 読み込み失敗 {path}: {e}"));
    let spec =
        chartjs::parse(&json, false).unwrap_or_else(|e| panic!("spec parse 失敗 {name}: {e}"));
    render_chart_to_png(&spec, 1.0, fulgur_chart::font::DEFAULT_FONT)
        .unwrap_or_else(|e| panic!("ラスタライズ失敗 {name}: {e}"))
}

#[test]
fn golden_png_matches() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();

    for &name in NAMES {
        let actual_png = render_to_png(name);
        let golden = golden_path(name);

        if update {
            // 再生成モード: golden を書き出して比較はスキップ。
            if let Some(dir) = golden.parent() {
                std::fs::create_dir_all(dir)
                    .unwrap_or_else(|e| panic!("golden ディレクトリ作成失敗 {dir:?}: {e}"));
            }
            std::fs::write(&golden, &actual_png)
                .unwrap_or_else(|e| panic!("golden 書き込み失敗 {golden:?}: {e}"));
            continue;
        }

        // 比較モード。
        let golden_bytes = std::fs::read(&golden).unwrap_or_else(|_| {
            panic!(
                "golden が見つかりません {golden:?}。意図した初回生成なら \
                 UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png で再生成してください"
            )
        });

        let golden_pix = Pixmap::decode_png(&golden_bytes)
            .unwrap_or_else(|e| panic!("golden PNG デコード失敗 {name}: {e}"));
        let actual_pix = Pixmap::decode_png(&actual_png)
            .unwrap_or_else(|e| panic!("actual PNG デコード失敗 {name}: {e}"));

        // 寸法は完全一致が前提（data の zip で取りこぼさないよう先に確認）。
        assert_eq!(
            (golden_pix.width(), golden_pix.height()),
            (actual_pix.width(), actual_pix.height()),
            "寸法が一致しません {name}: golden {}x{} vs actual {}x{}",
            golden_pix.width(),
            golden_pix.height(),
            actual_pix.width(),
            actual_pix.height(),
        );

        // RGBA 4 バイト単位で走査し、いずれかのチャンネル絶対差が許容を超えた
        // ピクセルを数える。u8 同士の減算は debug でアンダーフロー panic するため i16 にキャスト。
        let diff_pixels = golden_pix
            .data()
            .chunks_exact(4)
            .zip(actual_pix.data().chunks_exact(4))
            .filter(|(g, a)| {
                g.iter()
                    .zip(a.iter())
                    .any(|(gc, ac)| (*gc as i16 - *ac as i16).abs() > CHANNEL_TOLERANCE)
            })
            .count();

        let total_pixels = (golden_pix.width() as u64 * golden_pix.height() as u64) as f64;
        let diff_frac = diff_pixels as f64 / total_pixels;

        assert!(
            diff_frac <= MAX_DIFF_FRAC,
            "視覚差分が大きすぎます name={name} diff_frac={diff_frac:.6} (許容 {MAX_DIFF_FRAC}). \
             差分が大きい場合は意図した変更なら UPDATE_GOLDEN=1 で再生成、そうでなければ視覚回帰",
        );
    }
}
