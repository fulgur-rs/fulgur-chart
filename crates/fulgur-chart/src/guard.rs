//! 入力安全性: 信頼できない JSON spec から DoS を防ぐ上限定数と検証。
//!
//! ## 設計方針
//! - 検証は CLI の信頼境界(render_one)でのみ行う。内部の描画コアは上限検証を持たない。
//! - 入力を truncate・変形せず、超過時は Err を返して拒否する。
//! - 上限はデフォルト定数で定義し、`InputLimits` 構造体でカスタマイズできる。
//!
//! ## 対象外
//! - **タイムアウト**: 入力上限が有効であれば処理量は有界になるため不要。
//!   wall-clock タイムアウトは出力を非決定的にするので採用しない。
//! - **stdin サイズ制限**: OS/シェルレベルで対処すべき領域。
//! - **フォントファイルサイズ**: --font はユーザ自身が渡す。不正フォントは
//!   ttf_parser::Face::parse が Err を返すので別途処理済み。

use crate::ir::ChartSpec;

// --- デフォルト上限定数 ---

/// 全系列の合計データ点数の上限(scatter/bubble 向け)。
/// 合計で抑えることで series × points の積による爆発を防ぐ。
pub const DEFAULT_MAX_TOTAL_DATA_POINTS: usize = 1_000_000;

/// 系列(dataset)の上限。
pub const DEFAULT_MAX_SERIES: usize = 1_000;

/// カテゴリ(labels)の上限。
pub const DEFAULT_MAX_CATEGORIES: usize = 100_000;

/// series × categories の積の上限(棒グラフ/折れ線 向け)。
/// bar チャートでは各セルが SVG 要素 1 つになるため積がプリミティブ数に直結する。
/// 1M 要素 × ~150 B ≈ 150 MB SVG が実用的な上限の目安。
pub const DEFAULT_MAX_CATEGORICAL_PRIMITIVES: usize = 1_000_000;

/// ラベル・タイトル文字列の上限(バイト)。
pub const DEFAULT_MAX_LABEL_BYTES: usize = 4_096;

/// treemap のツリー深さの上限。スタックオーバーフロー/DoS 対策。parser の groups 上限に揃える。
pub const DEFAULT_MAX_TREE_DEPTH: usize = 50;

/// spec の width/height 上限(px)。
/// Chrome のブラウザ上限(32,767 px)に合わせた値。
/// PNG 面積の独立した入口を塞ぐ目的もある。実際の PNG メモリは raster の面積チェックで保護する。
pub const DEFAULT_MAX_DIMENSION_PX: f64 = 32_768.0;

/// spec の width/height 下限(px)。
/// ゼロ・負値はレイアウトで除算異常を起こし得るため拒否する。
pub const DEFAULT_MIN_DIMENSION_PX: f64 = 1.0;

// --- 設定構造体 ---

/// 入力検証の上限設定。各フィールドはデフォルト値から変更できる。
///
/// # 例
/// ```
/// use fulgur_chart::guard::InputLimits;
///
/// // デフォルト上限を使う
/// let limits = InputLimits::default();
///
/// // 上限を緩める(信頼済みの大規模データ向け)
/// let relaxed = InputLimits {
///     max_series: 5_000,
///     max_categorical_primitives: 10_000_000,
///     ..InputLimits::default()
/// };
///
/// // 上限を厳しくする(公開 API 向け)
/// let strict = InputLimits {
///     max_series: 20,
///     max_categories: 500,
///     max_categorical_primitives: 10_000,
///     ..InputLimits::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct InputLimits {
    /// 全系列の合計データ点数の上限(scatter/bubble 向け)。
    pub max_total_data_points: usize,
    /// 系列(dataset)の上限。
    pub max_series: usize,
    /// カテゴリ(labels)の上限。
    pub max_categories: usize,
    /// series × categories の積の上限(bar/line チャートの SVG プリミティブ数を抑える)。
    pub max_categorical_primitives: usize,
    /// ラベル・タイトル文字列の上限(バイト)。
    pub max_label_bytes: usize,
    /// width/height の上限(px)。
    pub max_dimension_px: f64,
    /// width/height の下限(px)。
    pub min_dimension_px: f64,
}

impl Default for InputLimits {
    fn default() -> Self {
        Self {
            max_total_data_points: DEFAULT_MAX_TOTAL_DATA_POINTS,
            max_series: DEFAULT_MAX_SERIES,
            max_categories: DEFAULT_MAX_CATEGORIES,
            max_categorical_primitives: DEFAULT_MAX_CATEGORICAL_PRIMITIVES,
            max_label_bytes: DEFAULT_MAX_LABEL_BYTES,
            max_dimension_px: DEFAULT_MAX_DIMENSION_PX,
            min_dimension_px: DEFAULT_MIN_DIMENSION_PX,
        }
    }
}

// --- 検証 ---

/// treemap のツリーを再帰走査し、ノード総数を返す。各ノードのラベル長を
/// `max_label_bytes` で検証し、深さが `DEFAULT_MAX_TREE_DEPTH` を超えたら Err。
/// 深さチェックはノード処理の直前に行う。空の `children` への再帰では発火しないため、
/// 深さちょうど `DEFAULT_MAX_TREE_DEPTH` の葉を持つ有効なツリーは受理される。
/// 実ノードが深さ上限超で初めて Err になるため、本関数自身の再帰も有界。
fn validate_tree(
    nodes: &[crate::ir::TreeNode],
    depth: usize,
    limits: &InputLimits,
) -> Result<usize, String> {
    let mut count = 0usize;
    for n in nodes {
        if depth > DEFAULT_MAX_TREE_DEPTH {
            return Err(format!(
                "treemap のツリー深さが上限 {} を超えています",
                DEFAULT_MAX_TREE_DEPTH,
            ));
        }
        if n.label.len() > limits.max_label_bytes {
            return Err(format!(
                "treemap ラベルの長さ {} バイトが上限 {} を超えています",
                n.label.len(),
                limits.max_label_bytes,
            ));
        }
        count += 1 + validate_tree(&n.children, depth + 1, limits)?;
    }
    Ok(count)
}

/// ChartSpec が `limits` の入力上限内にあることを検証する。
///
/// CLI は `--width`/`--height` オーバーライドを適用した後にこの関数を呼ぶ。
/// 超過した場合は `Err(説明メッセージ)` を返す。
pub fn validate_spec(spec: &ChartSpec, limits: &InputLimits) -> Result<(), String> {
    // --- 寸法 ---
    if !spec.width.is_finite()
        || spec.width < limits.min_dimension_px
        || spec.width > limits.max_dimension_px
    {
        return Err(format!(
            "width {:.0} は有効範囲 [{:.0}–{:.0}] を外れています",
            spec.width, limits.min_dimension_px, limits.max_dimension_px,
        ));
    }
    if !spec.height.is_finite()
        || spec.height < limits.min_dimension_px
        || spec.height > limits.max_dimension_px
    {
        return Err(format!(
            "height {:.0} は有効範囲 [{:.0}–{:.0}] を外れています",
            spec.height, limits.min_dimension_px, limits.max_dimension_px,
        ));
    }

    // --- 系列数 ---
    if spec.series.len() > limits.max_series {
        return Err(format!(
            "系列数 {} が上限 {} を超えています",
            spec.series.len(),
            limits.max_series,
        ));
    }

    // --- カテゴリ数 ---
    if spec.categories.len() > limits.max_categories {
        return Err(format!(
            "カテゴリ数 {} が上限 {} を超えています",
            spec.categories.len(),
            limits.max_categories,
        ));
    }

    // --- series × categories の積(bar/line チャートのプリミティブ数) ---
    // MAX_SERIES と MAX_CATEGORIES を独立に設定すると積が膨大になるため、
    // 積を直接バウンドする。例: 1,000 series × 100,000 categories = 1億要素。
    let categorical_primitives = spec.series.len().saturating_mul(spec.categories.len());
    if categorical_primitives > limits.max_categorical_primitives {
        return Err(format!(
            "系列数 {} × カテゴリ数 {} = {} が上限 {} を超えています",
            spec.series.len(),
            spec.categories.len(),
            categorical_primitives,
            limits.max_categorical_primitives,
        ));
    }

    // --- progress バー数(プリミティブ数) ---
    // progress は series[0].values の各要素が 1 本のバーになり、バーごとに
    // トラック・前景・バー名・%ラベルで最大 4 プリミティブを生む。カテゴリ(labels)が
    // 空だと series×categories=0 で categorical 上限を素通りするため、バー数 ×
    // バーあたり最大プリミティブ数を categorical 上限で個別に検証する。
    if matches!(spec.kind, crate::ir::ChartKind::Progress) {
        const PRIMS_PER_BAR: usize = 4; // トラック + 前景 + バー名 + %ラベル(最大)
        let bars = spec.series.first().map(|s| s.values.len()).unwrap_or(0);
        let progress_primitives = bars.saturating_mul(PRIMS_PER_BAR);
        if progress_primitives > limits.max_categorical_primitives {
            return Err(format!(
                "progress バー数 {} (プリミティブ {}) が上限 {} を超えています",
                bars, progress_primitives, limits.max_categorical_primitives,
            ));
        }
    }

    // --- boxplot プリミティブ数 ---
    // 1 ボックスあたり rect×1 + 枠線×4 + 中央値×1 + ヒゲ×2 + キャップ×2 = 10 プリミティブ。
    // categories/series 上限は箱数を間接的に制限するが、primitive cap を直接チェックする。
    if matches!(spec.kind, crate::ir::ChartKind::BoxPlot) {
        const PRIMS_PER_BOX: usize = 10;
        let boxes = spec
            .series
            .iter()
            .flat_map(|s| s.box_points.iter())
            .filter(|p| {
                p.min.is_finite()
                    && p.q1.is_finite()
                    && p.median.is_finite()
                    && p.q3.is_finite()
                    && p.max.is_finite()
            })
            .count();
        let boxplot_primitives = boxes.saturating_mul(PRIMS_PER_BOX);
        if boxplot_primitives > limits.max_categorical_primitives {
            return Err(format!(
                "boxplot ボックス数 {} (プリミティブ {}) が上限 {} を超えています",
                boxes, boxplot_primitives, limits.max_categorical_primitives,
            ));
        }
    }

    // --- outlabeledPie バリデーション ---
    if let crate::ir::ChartKind::OutlabeledPie { ref outlabel, .. } = spec.kind {
        // Left/Right 凡例は未サポート（pie.rs と挙動を揃えるため明示エラー）。
        if matches!(
            spec.legend,
            crate::ir::LegendPos::Left | crate::ir::LegendPos::Right
        ) {
            return Err("outlabeledPie では Left/Right 凡例はサポートされていません".to_string());
        }

        // テンプレート文字列長の上限チェック。
        if outlabel.text.len() > limits.max_label_bytes {
            return Err(format!(
                "outlabel.text の長さ {} バイトが上限 {} を超えています",
                outlabel.text.len(),
                limits.max_label_bytes,
            ));
        }

        // プリミティブ数上限: スライスごとに Path + Polyline + Rect + Text×N。
        // ゼロ/非有限値は描画でスキップされるため、有効スライスのみカウントする。
        let n_lines = outlabel.text.split('\n').count().max(1);
        let prims_per_slice = 3 + n_lines;
        let slices = spec
            .series
            .first()
            .map(|s| {
                s.values
                    .iter()
                    .filter(|v| v.is_finite() && **v > 0.0)
                    .count()
            })
            .unwrap_or(0);
        let outlabeled_primitives = slices.saturating_mul(prims_per_slice);
        if outlabeled_primitives > limits.max_categorical_primitives {
            return Err(format!(
                "outlabeledPie: スライス数 {} × {} プリミティブ = {} が上限 {} を超えます",
                slices, prims_per_slice, outlabeled_primitives, limits.max_categorical_primitives,
            ));
        }
    }

    // --- 全データ点数の合計(scatter/bubble 向け) ---
    // values と points の大きい方を各系列のコストとして合算する。
    // treemap ツリーをノード数・深さ・ラベル長で検証する。深さ上限により本検証と
    // draw_nodes の再帰スタックが有界になり、IR 直接構築や他フロントエンド経由でも DoS を防ぐ。
    let mut tree_points = 0usize;
    for s in &spec.series {
        tree_points += validate_tree(&s.tree, 0, limits)?;
    }

    let total_points: usize = spec
        .series
        .iter()
        .map(|s| s.values.len().max(s.points.len()).max(s.box_points.len()))
        .sum::<usize>()
        + tree_points;
    if total_points > limits.max_total_data_points {
        return Err(format!(
            "全系列のデータ点数合計 {} が上限 {} を超えています",
            total_points, limits.max_total_data_points,
        ));
    }

    // --- 文字列長 ---
    if let Some(title) = &spec.title {
        if title.len() > limits.max_label_bytes {
            return Err(format!(
                "タイトルの長さ {} バイトが上限 {} を超えています",
                title.len(),
                limits.max_label_bytes,
            ));
        }
    }
    for cat in &spec.categories {
        if cat.len() > limits.max_label_bytes {
            return Err(format!(
                "カテゴリラベルの長さ {} バイトが上限 {} を超えています",
                cat.len(),
                limits.max_label_bytes,
            ));
        }
    }
    for ser in &spec.series {
        if ser.name.len() > limits.max_label_bytes {
            return Err(format!(
                "系列名の長さ {} バイトが上限 {} を超えています",
                ser.name.len(),
                limits.max_label_bytes,
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

    fn default_limits() -> InputLimits {
        InputLimits::default()
    }

    #[test]
    fn valid_spec_passes() {
        assert!(validate_spec(&base_spec(), &default_limits()).is_ok());
    }

    #[test]
    fn zero_width_is_rejected() {
        let mut s = base_spec();
        s.width = 0.0;
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn negative_height_is_rejected() {
        let mut s = base_spec();
        s.height = -1.0;
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn max_dimension_is_accepted() {
        let mut s = base_spec();
        s.width = DEFAULT_MAX_DIMENSION_PX;
        s.height = DEFAULT_MAX_DIMENSION_PX;
        assert!(validate_spec(&s, &default_limits()).is_ok());
    }

    #[test]
    fn over_max_dimension_is_rejected() {
        let mut s = base_spec();
        s.width = DEFAULT_MAX_DIMENSION_PX + 1.0;
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn nan_dimension_is_rejected() {
        let mut s = base_spec();
        s.width = f64::NAN;
        assert!(validate_spec(&s, &default_limits()).is_err());
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
            box_points: vec![],
            tree: vec![],
        };
        s.series = vec![dummy; DEFAULT_MAX_SERIES + 1];
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn too_many_categories_is_rejected() {
        let mut s = base_spec();
        s.categories = vec!["x".to_string(); DEFAULT_MAX_CATEGORIES + 1];
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn too_many_categorical_primitives_is_rejected() {
        use crate::ir::{Color, Series, SeriesType};
        // 100 series × 10,001 categories = 1,000,100 > 1,000,000
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
            box_points: vec![],
            tree: vec![],
        };
        s.series = vec![dummy; 100];
        s.categories = vec!["x".to_string(); 10_001];
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn categorical_primitives_at_limit_is_accepted() {
        use crate::ir::{Color, Series, SeriesType};
        // 100 series × 10,000 categories = 1,000,000 (exactly at limit)
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
            box_points: vec![],
            tree: vec![],
        };
        s.series = vec![dummy; 100];
        s.categories = vec!["x".to_string(); 10_000];
        assert!(validate_spec(&s, &default_limits()).is_ok());
    }

    #[test]
    fn too_many_total_data_points_is_rejected() {
        use crate::ir::{Color, Series, SeriesType};
        let mut s = base_spec();
        let big_series = Series {
            name: String::new(),
            values: vec![1.0; DEFAULT_MAX_TOTAL_DATA_POINTS + 1],
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
            box_points: vec![],
            tree: vec![],
        };
        s.series = vec![big_series];
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn too_many_progress_bars_is_rejected() {
        // progress は 1 バー = 最大 4 プリミティブ。total_data_points(1M)は通るが
        // categorical 上限(1M)を 4 倍係数で超える本数を拒否する。
        let mut s = chartjs::parse(
            r#"{"type":"progress","data":{"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        // 250,001 本 × 4 = 1,000,004 > 1,000,000（total_points 250,001 は上限内）
        s.series[0].values = vec![1.0; DEFAULT_MAX_CATEGORICAL_PRIMITIVES / 4 + 1];
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn progress_bars_within_limit_is_accepted() {
        let mut s = chartjs::parse(
            r#"{"type":"progress","data":{"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        // 250,000 本 × 4 = 1,000,000 <= 1,000,000
        s.series[0].values = vec![1.0; DEFAULT_MAX_CATEGORICAL_PRIMITIVES / 4];
        assert!(validate_spec(&s, &default_limits()).is_ok());
    }

    #[test]
    fn long_label_is_rejected() {
        let mut s = base_spec();
        s.categories = vec!["x".repeat(DEFAULT_MAX_LABEL_BYTES + 1)];
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn long_series_name_is_rejected() {
        let mut s = base_spec();
        s.series[0].name = "x".repeat(DEFAULT_MAX_LABEL_BYTES + 1);
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn long_title_is_rejected() {
        let mut s = base_spec();
        s.title = Some("x".repeat(DEFAULT_MAX_LABEL_BYTES + 1));
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn outlabeled_pie_primitive_guard_rejects_too_many_slices() {
        use crate::ir::OutlabelConfig;
        let mut spec = chartjs::parse(
            r#"{"type":"outlabeledPie","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        spec.kind = crate::ir::ChartKind::OutlabeledPie {
            donut_ratio: 0.0,
            outlabel: OutlabelConfig::default(),
        };
        // デフォルトテンプレート "%l\n%p%" は 2 行 → prims_per_slice = 3 + 2 = 5
        // 200,001 スライス × 5 プリミティブ = 1,000,005 > 1,000,000 limit
        spec.series[0].values = vec![1.0; 200_001];
        let limits = default_limits();
        let result = validate_spec(&spec, &limits);
        assert!(result.is_err(), "must reject too many outlabel primitives");
    }

    #[test]
    fn custom_limits_override_defaults() {
        let mut s = base_spec();
        // デフォルトでは通る series=1 を、カスタム上限 max_series=0 で拒否する
        let strict = InputLimits {
            max_series: 0,
            ..InputLimits::default()
        };
        assert!(validate_spec(&s, &strict).is_err());

        // デフォルトでは拒否される width=50,000 を、カスタム上限で通す
        s.width = 50_000.0;
        let relaxed = InputLimits {
            max_dimension_px: 100_000.0,
            ..InputLimits::default()
        };
        assert!(validate_spec(&s, &relaxed).is_ok());
    }

    #[test]
    fn treemap_tree_nodes_count_toward_total_points() {
        let spec = crate::frontend::chartjs::parse(
            r#"{"type":"treemap","data":{"datasets":[{"tree":[1,2,3,4,5]}]}}"#,
            false,
        )
        .unwrap();
        let mut limits = default_limits();
        limits.max_total_data_points = 4; // 5 ノード > 4
        assert!(validate_spec(&spec, &limits).is_err());
        limits.max_total_data_points = 5;
        assert!(validate_spec(&spec, &limits).is_ok());
    }

    #[test]
    fn treemap_deep_tree_is_rejected() {
        // 深さ DEFAULT_MAX_TREE_DEPTH+2 の手組みツリー
        use crate::ir::TreeNode;
        let mut node = TreeNode {
            label: "leaf".into(),
            value: 1.0,
            children: vec![],
        };
        for _ in 0..(DEFAULT_MAX_TREE_DEPTH + 2) {
            node = TreeNode {
                label: "g".into(),
                value: 1.0,
                children: vec![node],
            };
        }
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"treemap","data":{"datasets":[{"tree":[1]}]}}"#,
            false,
        )
        .unwrap();
        spec.series[0].tree = vec![node];
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn treemap_tree_at_max_depth_is_accepted() {
        // 深さちょうど DEFAULT_MAX_TREE_DEPTH の葉を持つツリーは受理される(境界)。
        use crate::ir::TreeNode;
        let mut node = TreeNode {
            label: "leaf".into(),
            value: 1.0,
            children: vec![],
        };
        for _ in 0..DEFAULT_MAX_TREE_DEPTH {
            node = TreeNode {
                label: "g".into(),
                value: 1.0,
                children: vec![node],
            };
        }
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"treemap","data":{"datasets":[{"tree":[1]}]}}"#,
            false,
        )
        .unwrap();
        spec.series[0].tree = vec![node];
        assert!(validate_spec(&spec, &default_limits()).is_ok());
    }

    #[test]
    fn treemap_oversized_label_is_rejected() {
        use crate::ir::TreeNode;
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"treemap","data":{"datasets":[{"tree":[1]}]}}"#,
            false,
        )
        .unwrap();
        spec.series[0].tree = vec![TreeNode {
            label: "x".repeat(DEFAULT_MAX_LABEL_BYTES + 1),
            value: 1.0,
            children: vec![],
        }];
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }
}
