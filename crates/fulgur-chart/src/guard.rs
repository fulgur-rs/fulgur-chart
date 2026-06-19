//! 入力安全性: 信頼できない JSON spec から DoS を防ぐ上限定数と検証。
//!
//! ## 設計方針
//! - 検証は CLI の信頼境界(render_one)でのみ行う。内部の描画コアは上限検証を持たない。
//! - 入力を truncate・変形せず、超過時は Err を返して拒否する。
//! - 定数はそれぞれ 1 つの脅威ベクタに対応し、コメントで根拠を示す。
//!
//! ## 対象外
//! - **タイムアウト**: 入力上限が有効であれば処理量は有界になるため不要。
//!   wall-clock タイムアウトは出力を非決定的にするので採用しない。
//! - **stdin サイズ制限**: OS/シェルレベルで対処すべき領域。
//! - **フォントファイルサイズ**: --font はユーザ自身が渡す。不正フォントは
//!   ttf_parser::Face::parse が Err を返すので別途処理済み。

use crate::ir::ChartSpec;

// --- 上限定数 ---

/// 全系列の合計データ点数の上限。
/// 1 系列あたりではなく合計で抑えることで series × points の積による爆発を防ぐ。
/// 1M 点: 各点が SVG 1 要素(~100 B)になるとして ~100 MB 出力。
pub const MAX_TOTAL_DATA_POINTS: usize = 1_000_000;

/// 系列(dataset)の上限。凡例・色テーブルの線形探索コストを抑える。
pub const MAX_SERIES: usize = 1_000;

/// カテゴリ(labels)の上限。bar/line の x 軸ラベルが無制限に増えるのを防ぐ。
pub const MAX_CATEGORIES: usize = 100_000;

/// ラベル・タイトル文字列の上限(バイト)。
/// 巨大文字列が SVG テキストノードとして複数コピーされるのを防ぐ。
pub const MAX_LABEL_BYTES: usize = 4_096;

/// spec の width/height 上限(px)。
/// PNG 面積の独立した入口を塞ぐ。実際の PNG メモリは raster の面積チェックで保護する。
pub const MAX_DIMENSION_PX: f64 = 32_768.0;

/// spec の width/height 下限(px)。
/// ゼロ・負値はレイアウトで除算異常を起こし得るため拒否する。
pub const MIN_DIMENSION_PX: f64 = 1.0;

// --- 検証 ---

/// ChartSpec が入力上限内にあることを検証する。
///
/// CLI は `--width`/`--height` オーバーライドを適用した後にこの関数を呼ぶ。
/// 超過した場合は `Err(日本語メッセージ)` を返す。
pub fn validate_spec(spec: &ChartSpec) -> Result<(), String> {
    // --- 寸法 ---
    if !spec.width.is_finite() || spec.width < MIN_DIMENSION_PX || spec.width > MAX_DIMENSION_PX {
        return Err(format!(
            "width {:.0} は有効範囲 [{:.0}–{:.0}] を外れています",
            spec.width, MIN_DIMENSION_PX, MAX_DIMENSION_PX,
        ));
    }
    if !spec.height.is_finite() || spec.height < MIN_DIMENSION_PX || spec.height > MAX_DIMENSION_PX
    {
        return Err(format!(
            "height {:.0} は有効範囲 [{:.0}–{:.0}] を外れています",
            spec.height, MIN_DIMENSION_PX, MAX_DIMENSION_PX,
        ));
    }

    // --- 系列数 ---
    if spec.series.len() > MAX_SERIES {
        return Err(format!(
            "系列数 {} が上限 {} を超えています",
            spec.series.len(),
            MAX_SERIES,
        ));
    }

    // --- カテゴリ数 ---
    if spec.categories.len() > MAX_CATEGORIES {
        return Err(format!(
            "カテゴリ数 {} が上限 {} を超えています",
            spec.categories.len(),
            MAX_CATEGORIES,
        ));
    }

    // --- 全データ点数(合計) ---
    // values と points の大きい方を各系列のコストとして合算する。
    // 積(series × per-series)ではなく合計で抑えることで多次元の爆発を防ぐ。
    let total_points: usize = spec
        .series
        .iter()
        .map(|s| s.values.len().max(s.points.len()))
        .sum();
    if total_points > MAX_TOTAL_DATA_POINTS {
        return Err(format!(
            "全系列のデータ点数合計 {} が上限 {} を超えています",
            total_points, MAX_TOTAL_DATA_POINTS,
        ));
    }

    // --- 文字列長 ---
    if let Some(title) = &spec.title {
        if title.len() > MAX_LABEL_BYTES {
            return Err(format!(
                "タイトルの長さ {} バイトが上限 {} を超えています",
                title.len(),
                MAX_LABEL_BYTES,
            ));
        }
    }
    for cat in &spec.categories {
        if cat.len() > MAX_LABEL_BYTES {
            return Err(format!(
                "カテゴリラベルの長さ {} バイトが上限 {} を超えています",
                cat.len(),
                MAX_LABEL_BYTES,
            ));
        }
    }
    for ser in &spec.series {
        if ser.name.len() > MAX_LABEL_BYTES {
            return Err(format!(
                "系列名の長さ {} バイトが上限 {} を超えています",
                ser.name.len(),
                MAX_LABEL_BYTES,
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::chartjs;

    fn base_spec() -> ChartSpec {
        chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap()
    }

    #[test]
    fn valid_spec_passes() {
        assert!(validate_spec(&base_spec()).is_ok());
    }

    #[test]
    fn zero_width_is_rejected() {
        let mut s = base_spec();
        s.width = 0.0;
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn negative_height_is_rejected() {
        let mut s = base_spec();
        s.height = -1.0;
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn max_dimension_is_accepted() {
        let mut s = base_spec();
        s.width = MAX_DIMENSION_PX;
        s.height = MAX_DIMENSION_PX;
        assert!(validate_spec(&s).is_ok());
    }

    #[test]
    fn over_max_dimension_is_rejected() {
        let mut s = base_spec();
        s.width = MAX_DIMENSION_PX + 1.0;
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn nan_dimension_is_rejected() {
        let mut s = base_spec();
        s.width = f64::NAN;
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn too_many_series_is_rejected() {
        use crate::ir::{Color, Series, SeriesType};
        let mut s = base_spec();
        let dummy = Series {
            name: String::new(),
            values: vec![1.0],
            points: vec![],
            fill: vec![Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            }],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
        };
        s.series = vec![dummy; MAX_SERIES + 1];
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn too_many_categories_is_rejected() {
        let mut s = base_spec();
        s.categories = vec!["x".to_string(); MAX_CATEGORIES + 1];
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn too_many_total_data_points_is_rejected() {
        use crate::ir::{Color, Series, SeriesType};
        let mut s = base_spec();
        let big_series = Series {
            name: String::new(),
            values: vec![1.0; MAX_TOTAL_DATA_POINTS + 1],
            points: vec![],
            fill: vec![Color {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            }],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
        };
        s.series = vec![big_series];
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn long_label_is_rejected() {
        let mut s = base_spec();
        s.categories = vec!["x".repeat(MAX_LABEL_BYTES + 1)];
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn long_series_name_is_rejected() {
        let mut s = base_spec();
        s.series[0].name = "x".repeat(MAX_LABEL_BYTES + 1);
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn long_title_is_rejected() {
        let mut s = base_spec();
        s.title = Some("x".repeat(MAX_LABEL_BYTES + 1));
        assert!(validate_spec(&s).is_err());
    }
}
