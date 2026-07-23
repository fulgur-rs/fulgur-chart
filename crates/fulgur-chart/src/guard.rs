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

use crate::ir::{ChartKind, ChartSpec, XPositions};
use crate::num::fmt_num;

// --- デフォルト上限定数 ---

/// wordcloud の単語数上限 (DoS 対策)。
const MAX_WORDCLOUD_WORDS: usize = 500;
/// wordcloud の 1 語あたりバイト長上限 (SVG サイズ攻撃対策)。
const MAX_WORDCLOUD_WORD_BYTES: usize = 200;

/// sankey のリンク数上限 (DoS 対策)。
/// 各リンクは 1 本のリボン(SVG path プリミティブ)になるため、リンク数がほぼ
/// プリミティブ数・パースコストに直結する。10,000 本 ≈ 出力規模・処理量ともに実用上限。
pub const MAX_SANKEY_LINKS: usize = 10_000;

/// sankey のユニークノード数上限 (スタックオーバーフロー/DoS 対策)。
///
/// レイアウト(`layout/sankey.rs`)の `process_from`/`process_to`、および
/// `calculate_x` 内の `get_all_keys_forward` は、連鎖に沿ってノード 1 つあたり
/// スタックフレーム 1 つを消費する再帰で実装されている。線形連鎖(`n0→n1→…`)では
/// 再帰深さがノード数とほぼ等しくなるため、ノード数がそのままスタック消費の上限になる。
///
/// テストスレッドの既定スタックは約 2 MB(本番メインスレッドは約 8 MB)と小さい。
/// treemap がツリー深さを `DEFAULT_MAX_TREE_DEPTH = 50` で抑えてスタックを有界化する
/// のと同じ理由で、sankey もノード数を抑えて最悪ケースの再帰深さを安全圏に保つ。
/// この値は 2 MB スタックで線形連鎖をレンダリングしてもオーバーフローしないことを
/// 経験的に検証して決めている(`tests/render_sankey.rs` のスタック安全テスト参照)。
/// 線形連鎖は支配的な再帰(`process_*`/`get_all_keys_forward`)の深さを最大化する
/// 一方、各ノードの辺が 1 本なので `sort_by_node_count` 経由の `node_count` 再帰は
/// 発火しない。分岐グラフでは `process_*` の深さに `node_count` の再帰深さが加算され
/// うるが、本番メインスレッドのスタックは約 8 MB(測定に使った 2 MB の約 4 倍)あり、
/// 線形連鎖で確保した約 3 倍のマージンと合わせて、この加算分を吸収できる範囲に収まる。
pub const MAX_SANKEY_NODES: usize = 2_000;

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

    if let XPositions::Temporal { unix_millis } = &spec.x_positions {
        if unix_millis.len() != spec.categories.len() {
            return Err(format!(
                "temporal x position count {} does not match category count {}",
                unix_millis.len(),
                spec.categories.len()
            ));
        }
        if spec
            .series
            .iter()
            .any(|series| series.values.len() != unix_millis.len())
        {
            return Err("temporal x position count does not match every line series".to_string());
        }
        if unix_millis.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("temporal x positions must be strictly increasing".to_string());
        }
        if !matches!(spec.kind, ChartKind::Line) {
            return Err("temporal x positions are only supported for line charts".to_string());
        }
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

    // --- VegaRect ラベル・グリッドセル数 ---
    // VegaRect のラベルは spec.categories/spec.series を経由せず ChartKind に直接
    // 保持されるため、既存の max_label_bytes 検証(全カテゴリ・全系列名を走査)を
    // すり抜ける。加えて layout/build_rect は x_labels.len() × y_labels.len() の
    // dense grid を確保するため、sparse rect input(distinct ラベルは膨大だが
    // 観測ペアは少数)でも積が primitive 上限を超えうる。ここで両者を明示的に押さえる。
    if let crate::ir::ChartKind::VegaRect {
        x_labels,
        y_labels,
        cells,
    } = &spec.kind
    {
        for label in x_labels.iter().chain(y_labels.iter()) {
            if label.len() > limits.max_label_bytes {
                return Err(format!(
                    "VegaRect のラベル長 {} バイトが上限 {} を超えています",
                    label.len(),
                    limits.max_label_bytes,
                ));
            }
        }
        // rect ラベル数を bar/line 系と同じく max_categories で個別に押さえる。
        // (spec.categories は空なので既存ループはバイパスされる。) 1M × 1 のような
        // 非対称グリッドが primitive cap 単独では通過してしまうケースを塞ぐ。
        if x_labels.len() > limits.max_categories {
            return Err(format!(
                "VegaRect の x_labels 数 {} が max_categories 上限 {} を超えています",
                x_labels.len(),
                limits.max_categories,
            ));
        }
        if y_labels.len() > limits.max_categories {
            return Err(format!(
                "VegaRect の y_labels 数 {} が max_categories 上限 {} を超えています",
                y_labels.len(),
                limits.max_categories,
            ));
        }
        // 直接 ChartSpec を組む callers (bindings 等) 向けに cells 形状を検証する。
        // フロントエンドは build_rect で shape を保証しているが、guard は external
        // callers を守る境界。renderer 側は debug_assert のみで release では黙って
        // ミスレンダーするため、ここで明示 Err にする。
        if cells.len() != y_labels.len() {
            return Err(format!(
                "VegaRect cells 行数 {} と y_labels 数 {} が不一致",
                cells.len(),
                y_labels.len(),
            ));
        }
        for (row_i, row) in cells.iter().enumerate() {
            if row.len() != x_labels.len() {
                return Err(format!(
                    "VegaRect cells 行 {} の列数 {} と x_labels 数 {} が不一致",
                    row_i,
                    row.len(),
                    x_labels.len(),
                ));
            }
        }
        let grid = x_labels.len().saturating_mul(y_labels.len());
        if grid > limits.max_categorical_primitives {
            return Err(format!(
                "VegaRect の grid セル数 {} (x_labels {} × y_labels {}) が上限 {} を超えています",
                grid,
                x_labels.len(),
                y_labels.len(),
                limits.max_categorical_primitives,
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

        // テンプレートはスライスごとに展開・計測・SVG へ保持されるため、単体の
        // outlabel.text がラベル上限内でも「スライス数 × 展開後テキスト量」が
        // 大きい入力は拒否する。カテゴリ名と値の実データを使って、レンダリングで
        // 生成されるテキスト量の保守的な上限を見積もる。
        let expanded_text_bytes = estimate_outlabel_expanded_bytes(spec, &outlabel.text);
        if expanded_text_bytes > limits.max_categorical_primitives {
            return Err(format!(
                "outlabeledPie: 展開後ラベル文字列 {} バイトが上限 {} を超えます",
                expanded_text_bytes, limits.max_categorical_primitives,
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

    // sankey のリンクもデータ点として合算する(各リンク=1 リボン)。これにより
    // グローバルな max_total_data_points 上限が sankey にも効く。
    let link_points: usize = spec.series.iter().map(|s| s.links.len()).sum();

    let total_points: usize = spec
        .series
        .iter()
        .map(|s| s.values.len().max(s.points.len()).max(s.box_points.len()))
        .sum::<usize>()
        + tree_points
        + link_points;
    if total_points > limits.max_total_data_points {
        return Err(format!(
            "全系列のデータ点数合計 {} が上限 {} を超えています",
            total_points, limits.max_total_data_points,
        ));
    }

    // --- 文字列長 ---
    if let Some(title) = &spec.title
        && title.len() > limits.max_label_bytes
    {
        return Err(format!(
            "タイトルの長さ {} バイトが上限 {} を超えています",
            title.len(),
            limits.max_label_bytes,
        ));
    }
    if let Some(title) = &spec.legend_title
        && title.len() > limits.max_label_bytes
    {
        return Err(format!(
            "legend title length {} bytes exceeds limit {}",
            title.len(),
            limits.max_label_bytes,
        ));
    }
    for (axis, title) in [
        ("x", spec.x_axis.title.as_ref()),
        ("y", spec.y_axis.title.as_ref()),
    ] {
        if let Some(title) = title
            && title.text.len() > limits.max_label_bytes
        {
            return Err(format!(
                "{axis}-axis title length {} bytes exceeds limit {}",
                title.text.len(),
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

    // --- wordcloud 単語数・ラベル長・パラメータ ---
    if let crate::ir::ChartKind::WordCloud {
        entries,
        min_rotation,
        max_rotation,
        padding,
        ..
    } = &spec.kind
    {
        if entries.len() > MAX_WORDCLOUD_WORDS {
            return Err(format!(
                "wordcloud の単語数 {} が上限 {} を超えています",
                entries.len(),
                MAX_WORDCLOUD_WORDS,
            ));
        }
        if !padding.is_finite() || *padding < 0.0 {
            return Err(format!(
                "wordcloud: padding は 0 以上の有限値でなければなりません: {padding}"
            ));
        }
        if !min_rotation.is_finite() || !max_rotation.is_finite() {
            return Err("wordcloud: 回転角度は有限値でなければなりません".to_string());
        }
        for e in entries {
            if e.text.len() > MAX_WORDCLOUD_WORD_BYTES {
                return Err(format!(
                    "wordcloud: 単語が長すぎます ({}バイト > 上限 {}バイト)",
                    e.text.len(),
                    MAX_WORDCLOUD_WORD_BYTES,
                ));
            }
            if !e.size.is_finite() || e.size <= 0.0 {
                return Err(format!(
                    "wordcloud: 単語サイズは正の有限値でなければなりません: {}",
                    e.size
                ));
            }
        }
    }

    // --- sankey リンク数・ノード数・ノードラベル長 ---
    // リンク数はリボン(プリミティブ)数・パースコストを、ノード数はレイアウトの
    // 再帰深さ(process_from/process_to/get_all_keys_forward)を抑える。MAX_SANKEY_NODES の
    // 再帰スタック根拠は定数の doc コメント参照(treemap の深さ上限と同趣旨)。
    if let crate::ir::ChartKind::Sankey {
        labels, columns, ..
    } = &spec.kind
    {
        let links = spec.series.first().map(|s| s.links.len()).unwrap_or(0);
        if links > MAX_SANKEY_LINKS {
            return Err(format!(
                "sankey link count {} exceeds limit {}",
                links, MAX_SANKEY_LINKS,
            ));
        }
        // ユニークノード集合 + 各ノードキーのバイト長 + 全フロー合計を一度に走査する。
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut total_flow = 0.0_f64;
        if let Some(s) = spec.series.first() {
            for link in &s.links {
                for key in [link.from.as_str(), link.to.as_str()] {
                    if seen.insert(key) && key.len() > limits.max_label_bytes {
                        return Err(format!(
                            "sankey node label length {} bytes exceeds limit {}",
                            key.len(),
                            limits.max_label_bytes,
                        ));
                    }
                }
                total_flow += link.flow;
            }
        }
        if seen.len() > MAX_SANKEY_NODES {
            return Err(format!(
                "sankey node count {} exceeds limit {}",
                seen.len(),
                MAX_SANKEY_NODES,
            ));
        }
        // 個々の flow が有限でも、合算で全フロー合計が ∞ に overflow すると、layout の
        // 列内サイズ合計や max_y も ∞ になり py(inf)=0 で幾何が潰れる。逆に合計が有限なら、
        // 各列のサイズ合計 ≈ 合計(保存則)も max_y も有限で、padding も(node_padding は上限済み)
        // 有限に収まる。よって「合計が有限であること」だけを検証すればよい(per-node を包含)。
        if !total_flow.is_finite() {
            return Err(format!(
                "sankey total flow is non-finite (values too large): {total_flow}"
            ));
        }
        // labels 上書き値も描画される(幅測定 + SVG 出力)ため max_label_bytes で検証する。
        for (key, label) in labels {
            if label.len() > limits.max_label_bytes {
                return Err(format!(
                    "sankey label override ({}) length {} bytes exceeds limit {}",
                    key,
                    label.len(),
                    limits.max_label_bytes,
                ));
            }
        }
        // 手動 column の巨大値は max_x を膨張させ fix_top / calculate_y_using_priority の
        // `0..=max_x` ループを暴走させる(DoS)。参照ノードの column を MAX_SANKEY_NODES 未満に制限する。
        for (key, &col) in columns {
            if seen.contains(key.as_str()) && col >= MAX_SANKEY_NODES {
                return Err(format!(
                    "sankey manual column ({})={} must be below limit {}",
                    key, col, MAX_SANKEY_NODES,
                ));
            }
        }
    }

    Ok(())
}

fn estimate_outlabel_expanded_bytes(spec: &ChartSpec, template: &str) -> usize {
    let Some(series) = spec.series.first() else {
        return 0;
    };

    // テンプレートを1回だけ解析してプレースホルダー数とリテラル長を取得する。
    // これにより O(N × T) の二重ループを O(T + N) に削減し、ガード自体が
    // DoS ベクターにならないようにする。
    let mut num_l = 0usize;
    let mut num_v = 0usize;
    let mut num_p = 0usize;
    let mut other_len = 0usize;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some(&'l') => {
                    chars.next();
                    num_l = num_l.saturating_add(1);
                }
                Some(&'v') => {
                    chars.next();
                    num_v = num_v.saturating_add(1);
                }
                Some(&'p') => {
                    chars.next();
                    num_p = num_p.saturating_add(1);
                }
                _ => {
                    other_len = other_len.saturating_add(c.len_utf8());
                }
            }
        } else {
            other_len = other_len.saturating_add(c.len_utf8());
        }
    }

    series
        .values
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_finite() && **v > 0.0)
        .map(|(idx, value)| {
            let label_len = spec.categories.get(idx).map(|s| s.len()).unwrap_or(0);
            let value_len = fmt_num(*value).len();
            // pct は `(frac * 100.0).round() as i64` で frac ∈ (0, 1] なので
            // pct ∈ [0, 100] → i64::to_string() の最大長は 3 バイト ("100")。
            const PCT_LEN_BOUND: usize = 3;
            other_len
                .saturating_add(num_l.saturating_mul(label_len))
                .saturating_add(num_v.saturating_mul(value_len))
                .saturating_add(num_p.saturating_mul(PCT_LEN_BOUND))
        })
        .fold(0usize, usize::saturating_add)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::chartjs;
    use crate::ir::XPositions;

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
    fn temporal_positions_must_match_categories() {
        let mut spec = base_spec();
        spec.categories = vec!["a".into(), "b".into()];
        spec.x_positions = XPositions::Temporal {
            unix_millis: vec![1],
        };
        let err = validate_spec(&spec, &default_limits()).unwrap_err();
        assert!(err.contains("temporal x position count"));
    }

    #[test]
    fn temporal_positions_must_be_strictly_increasing() {
        let mut spec = base_spec();
        spec.categories = vec!["a".into(), "b".into()];
        spec.series[0].values = vec![1.0, 2.0];
        spec.x_positions = XPositions::Temporal {
            unix_millis: vec![2, 2],
        };
        let err = validate_spec(&spec, &default_limits()).unwrap_err();
        assert!(err.contains("strictly increasing"));
    }

    #[test]
    fn temporal_positions_must_match_every_series() {
        let mut spec = base_spec();
        spec.categories = vec!["a".into(), "b".into()];
        spec.x_positions = XPositions::Temporal {
            unix_millis: vec![1, 2],
        };
        let err = validate_spec(&spec, &default_limits()).unwrap_err();
        assert!(err.contains("every line series"));
    }

    #[test]
    fn temporal_positions_require_line_chart() {
        let mut spec = base_spec();
        spec.x_positions = XPositions::Temporal {
            unix_millis: vec![1],
        };
        let err = validate_spec(&spec, &default_limits()).unwrap_err();
        assert!(err.contains("only supported for line charts"));
    }

    #[test]
    fn valid_spec_passes() {
        assert!(validate_spec(&base_spec(), &default_limits()).is_ok());
    }

    #[test]
    fn axis_and_legend_titles_accept_exact_label_limit() {
        use crate::ir::{AxisTitle, AxisTitleAlign};

        let limits = InputLimits {
            max_label_bytes: 4,
            ..default_limits()
        };
        let mut spec = base_spec();
        spec.legend_title = Some("xxxx".into());
        spec.x_axis.title = Some(AxisTitle {
            text: "xxxx".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        spec.y_axis.title = Some(AxisTitle {
            text: "xxxx".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        });
        assert!(validate_spec(&spec, &limits).is_ok());
    }

    #[test]
    fn axis_and_legend_titles_reject_over_label_limit() {
        use crate::ir::{AxisTitle, AxisTitleAlign};

        let limits = InputLimits {
            max_label_bytes: 4,
            ..default_limits()
        };
        let title = AxisTitle {
            text: "xxxxx".into(),
            color: None,
            font_size: None,
            align: AxisTitleAlign::Center,
        };
        let mut legend_spec = base_spec();
        legend_spec.legend_title = Some("xxxxx".into());
        assert!(validate_spec(&legend_spec, &limits).is_err());

        let mut x_spec = base_spec();
        x_spec.x_axis.title = Some(title.clone());
        assert!(validate_spec(&x_spec, &limits).is_err());

        let mut y_spec = base_spec();
        y_spec.y_axis.title = Some(title);
        assert!(validate_spec(&y_spec, &limits).is_err());
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
            interpolation: crate::ir::LineInterpolation::Linear,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
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
            interpolation: crate::ir::LineInterpolation::Linear,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
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
            interpolation: crate::ir::LineInterpolation::Linear,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
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
            interpolation: crate::ir::LineInterpolation::Linear,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
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
    fn outlabeled_pie_rejects_amplified_outlabel_text() {
        use crate::ir::OutlabelConfig;
        let mut spec = chartjs::parse(
            r#"{"type":"outlabeledPie","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        let outlabel = OutlabelConfig {
            text: "x".repeat(DEFAULT_MAX_LABEL_BYTES),
            ..OutlabelConfig::default()
        };
        spec.kind = crate::ir::ChartKind::OutlabeledPie {
            donut_ratio: 0.0,
            outlabel,
        };
        // 1,000 スライス × 4,096 バイト = 約 4 MiB。プリミティブ数は
        // 4,000 と上限内だが、保持・計測・シリアライズされる文字列量が過大。
        spec.categories = vec!["A".to_string(); 1_000];
        spec.series[0].values = vec![1.0; 1_000];
        let result = validate_spec(&spec, &default_limits());
        assert!(
            result.is_err(),
            "must reject aggregate outlabel text amplification"
        );
    }

    #[test]
    fn outlabeled_pie_allows_default_template_many_slices() {
        // デフォルトテンプレート "%l\n%p%" × 100,000 スライスは合法。
        // PCT_LEN_BOUND を過大に設定すると誤拒否されていたケースの回帰テスト。
        // pct ∈ [0, 100] なので最大 3 バイト。
        // 推定: 100,000 × (2 + 1 + 3) = 600,000 bytes ≤ 1,000,000
        let mut spec = chartjs::parse(
            r#"{"type":"outlabeledPie","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        // OutlabelConfig::default().text == "%l\n%p%"
        spec.kind = crate::ir::ChartKind::OutlabeledPie {
            donut_ratio: 0.0,
            outlabel: crate::ir::OutlabelConfig::default(),
        };
        spec.categories = vec!["A".to_string(); 100_000];
        spec.series[0].values = vec![1.0; 100_000];
        assert!(validate_spec(&spec, &default_limits()).is_ok());
    }

    #[test]
    fn outlabeled_pie_allows_small_template_many_slices() {
        use crate::ir::OutlabelConfig;
        let mut spec = chartjs::parse(
            r#"{"type":"outlabeledPie","data":{"labels":["A"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        let outlabel = OutlabelConfig {
            text: "%l".to_string(),
            ..OutlabelConfig::default()
        };
        spec.kind = crate::ir::ChartKind::OutlabeledPie {
            donut_ratio: 0.0,
            outlabel,
        };
        // プリミティブ数・展開後文字列量の両方がデフォルト上限内。
        // %l は 1 バイトラベルに展開されるため 100,000 × 1 = 100,000 bytes < 1,000,000。
        spec.categories = vec!["A".to_string(); 100_000];
        spec.series[0].values = vec![1.0; 100_000];
        assert!(validate_spec(&spec, &default_limits()).is_ok());
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

    #[test]
    fn wordcloud_too_many_words() {
        use crate::ir::{ChartKind, WordEntry};
        let entries: Vec<WordEntry> = (0..=500)
            .map(|i| WordEntry {
                text: format!("word{i}"),
                size: 12.0,
                color: None,
            })
            .collect();
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries,
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn wordcloud_label_too_long() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![WordEntry {
                    text: "a".repeat(201),
                    size: 12.0,
                    color: None,
                }],
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn wordcloud_valid_passes_guard() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![
                    WordEntry {
                        text: "Rust".to_string(),
                        size: 80.0,
                        color: None,
                    },
                    WordEntry {
                        text: "SVG".to_string(),
                        size: 60.0,
                        color: None,
                    },
                ],
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_ok());
    }

    #[test]
    fn wordcloud_negative_padding_rejected() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![WordEntry {
                    text: "A".to_string(),
                    size: 12.0,
                    color: None,
                }],
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: -1.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn wordcloud_nonfinite_padding_rejected() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![WordEntry {
                    text: "A".to_string(),
                    size: 12.0,
                    color: None,
                }],
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: f64::NAN,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn wordcloud_nonfinite_rotation_rejected() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![WordEntry {
                    text: "A".to_string(),
                    size: 12.0,
                    color: None,
                }],
                min_rotation: f64::INFINITY,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    #[test]
    fn wordcloud_zero_size_rejected() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![WordEntry {
                    text: "A".to_string(),
                    size: 0.0,
                    color: None,
                }],
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }

    // ── sankey ガード ──

    /// 最小の sankey spec を作り、リンク配列を差し替えるヘルパ。
    fn sankey_spec_with_links(links: Vec<crate::ir::SankeyLink>) -> ChartSpec {
        let mut s = chartjs::parse(
            r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]}}"#,
            false,
        )
        .unwrap();
        s.series[0].links = links;
        s
    }

    fn link(from: &str, to: &str) -> crate::ir::SankeyLink {
        crate::ir::SankeyLink {
            from: from.to_string(),
            to: to.to_string(),
            flow: 1.0,
            color_from: None,
            color_to: None,
        }
    }

    #[test]
    fn sankey_link_count_over_limit_rejected() {
        // A→B を MAX_SANKEY_LINKS+1 本。ノードは 2 つだけなので link 数チェックを単離する。
        let links = vec![link("A", "B"); MAX_SANKEY_LINKS + 1];
        let spec = sankey_spec_with_links(links);
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn sankey_node_count_over_limit_rejected() {
        // 自己ループ n_i→n_i を MAX_SANKEY_NODES+1 本。各リンク 1 ユニークキーで
        // ノード数 = MAX_SANKEY_NODES+1、リンク数は MAX_SANKEY_LINKS 未満に収め node 数チェックを単離する。
        let links: Vec<_> = (0..=MAX_SANKEY_NODES)
            .map(|i| {
                let key = format!("n{i}");
                link(&key, &key)
            })
            .collect();
        let spec = sankey_spec_with_links(links);
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn sankey_within_limits_ok() {
        let links = vec![
            link("A", "B"),
            link("A", "C"),
            link("B", "D"),
            link("C", "D"),
        ];
        let spec = sankey_spec_with_links(links);
        assert!(validate_spec(&spec, &default_limits()).is_ok());
    }

    #[test]
    fn sankey_node_label_too_long_rejected() {
        let long = "x".repeat(DEFAULT_MAX_LABEL_BYTES + 1);
        let spec = sankey_spec_with_links(vec![link(&long, "B")]);
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn sankey_links_count_toward_total_points() {
        let spec = sankey_spec_with_links(vec![link("A", "B"), link("B", "C"), link("C", "D")]);
        let mut limits = default_limits();
        limits.max_total_data_points = 2; // 3 リンク > 2
        assert!(validate_spec(&spec, &limits).is_err());
        limits.max_total_data_points = 3;
        assert!(validate_spec(&spec, &limits).is_ok());
    }

    #[test]
    fn sankey_huge_manual_column_rejected() {
        // 参照ノードの巨大 column は max_x を膨張させ DoS になるため拒否。
        let mut spec = sankey_spec_with_links(vec![link("A", "B")]);
        if let crate::ir::ChartKind::Sankey { columns, .. } = &mut spec.kind {
            columns.insert("B".to_string(), MAX_SANKEY_NODES);
        }
        assert!(validate_spec(&spec, &default_limits()).is_err());
        // 上限未満は OK。
        let mut ok = sankey_spec_with_links(vec![link("A", "B")]);
        if let crate::ir::ChartKind::Sankey { columns, .. } = &mut ok.kind {
            columns.insert("B".to_string(), MAX_SANKEY_NODES - 1);
        }
        assert!(validate_spec(&ok, &default_limits()).is_ok());
    }

    #[test]
    fn sankey_oversized_label_override_rejected() {
        // labels 上書き値も描画されるため node キー同様にバイト長を検証する。
        let long = "x".repeat(DEFAULT_MAX_LABEL_BYTES + 1);
        let mut spec = sankey_spec_with_links(vec![link("A", "B")]);
        if let crate::ir::ChartKind::Sankey { labels, .. } = &mut spec.kind {
            labels.insert("A".to_string(), long);
        }
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    fn flow_link(from: &str, to: &str, flow: f64) -> crate::ir::SankeyLink {
        crate::ir::SankeyLink {
            from: from.into(),
            to: to.into(),
            flow,
            color_from: None,
            color_to: None,
        }
    }

    #[test]
    fn sankey_non_finite_flow_total_rejected() {
        // 同一ソースからの 2 本の 1e308 で合計が +inf に overflow → 拒否。
        let spec =
            sankey_spec_with_links(vec![flow_link("A", "B", 1e308), flow_link("A", "C", 1e308)]);
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn sankey_aggregate_flow_overflow_rejected() {
        // 別ソースの 2 本(各 1e308)でも合計が ∞ に overflow → 全フロー合計の有限性で弾く。
        let spec =
            sankey_spec_with_links(vec![flow_link("A", "B", 1e308), flow_link("C", "D", 1e308)]);
        assert!(validate_spec(&spec, &default_limits()).is_err());
        // 単一の大きい有限 flow は合計も有限なので受理される(下流も保存則で有限に収まる)。
        let ok = sankey_spec_with_links(vec![flow_link("A", "B", 1e308)]);
        assert!(validate_spec(&ok, &default_limits()).is_ok());
    }

    // ── VegaRect ガード ──

    /// 最小の有効な VegaRect spec を vegalite フロントエンド経由で作るヘルパ。
    fn rect_spec_2x2() -> ChartSpec {
        crate::frontend::vegalite::parse(
            r#"{
                "mark": "rect",
                "data": {"values": [
                    {"x":"A","y":"X","v":1},
                    {"x":"B","y":"X","v":2},
                    {"x":"A","y":"Y","v":3},
                    {"x":"B","y":"Y","v":4}
                ]},
                "encoding": {
                    "x": {"field":"x"},
                    "y": {"field":"y"},
                    "color": {"field":"v","type":"quantitative"}
                }
            }"#,
            false,
        )
        .unwrap()
    }

    #[test]
    fn vega_rect_oversized_label_is_rejected() {
        // VegaRect のラベルは spec.categories 経由ではなく ChartKind::VegaRect の
        // x_labels/y_labels に直接持たれる。既存 max_label_bytes 検証を通り抜けないよう
        // guard が明示的に走査することを pin する。
        let mut spec = rect_spec_2x2();
        if let crate::ir::ChartKind::VegaRect { x_labels, .. } = &mut spec.kind {
            x_labels[0] = "x".repeat(DEFAULT_MAX_LABEL_BYTES + 1);
        }
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn vega_rect_grid_over_primitive_cap_is_rejected() {
        // x_labels.len() × y_labels.len() が categorical primitive 上限を超える
        // sparse rect(観測ペアはわずかだが distinct ラベルが膨大)を拒否する。
        // build_rect が dense に grid を確保するため、この積を先取りで押さえる。
        let mut spec = rect_spec_2x2();
        if let crate::ir::ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } = &mut spec.kind
        {
            *x_labels = (0..1001).map(|i| format!("x{i}")).collect();
            *y_labels = (0..1001).map(|i| format!("y{i}")).collect();
            // cells の形状不変条件は layout が debug_assert! で検証するが、
            // guard 側は積さえ確認できればよいので dummy 形状で OK。
            *cells = vec![vec![None; x_labels.len()]; y_labels.len()];
        }
        // 1001 × 1001 = 1,002,001 > 1,000,000
        assert!(validate_spec(&spec, &default_limits()).is_err());
    }

    #[test]
    fn vega_rect_within_grid_cap_is_accepted() {
        // 通常の小さい rect は validate_spec を通る。上のテストが誤って全 VegaRect を
        // 拒否していないことを確認する回帰ガード。
        let spec = rect_spec_2x2();
        assert!(validate_spec(&spec, &default_limits()).is_ok());
    }

    #[test]
    fn vega_rect_asymmetric_x_over_max_categories_is_rejected() {
        // x_labels = max_categories + 1, y_labels = 1 の非対称グリッドは
        // primitive cap (1M) 単体だと通過してしまう (100k+1 × 1 < 1M) が、
        // max_categories 単独ガードで拒否されるべき。
        let limits = default_limits();
        let n = limits.max_categories + 1;
        let mut spec = rect_spec_2x2();
        if let crate::ir::ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } = &mut spec.kind
        {
            *x_labels = (0..n).map(|i| format!("x{i}")).collect();
            *y_labels = vec!["y".to_string()];
            *cells = vec![vec![None; n]];
        }
        let err = validate_spec(&spec, &limits).unwrap_err();
        assert!(
            err.contains("x_labels"),
            "expected x_labels error, got: {err}"
        );
    }

    #[test]
    fn vega_rect_asymmetric_y_over_max_categories_is_rejected() {
        // 逆方向 (y のみ肥大化) も同じく個別ガードで拒否されること。
        let limits = default_limits();
        let n = limits.max_categories + 1;
        let mut spec = rect_spec_2x2();
        if let crate::ir::ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } = &mut spec.kind
        {
            *x_labels = vec!["x".to_string()];
            *y_labels = (0..n).map(|i| format!("y{i}")).collect();
            *cells = vec![vec![None]; n];
        }
        let err = validate_spec(&spec, &limits).unwrap_err();
        assert!(
            err.contains("y_labels"),
            "expected y_labels error, got: {err}"
        );
    }

    #[test]
    fn vega_rect_cells_shape_mismatch_is_rejected() {
        // 直接 ChartSpec を組む callers 向けの境界。cells 行数が y_labels 数と
        // 不一致 (または各行の列数が x_labels 数と不一致) の場合、renderer の
        // debug_assert に依存せず guard で明示 Err にする。
        let mut spec = rect_spec_2x2();
        if let crate::ir::ChartKind::VegaRect {
            x_labels,
            y_labels,
            cells,
        } = &mut spec.kind
        {
            *x_labels = vec!["A".to_string(), "B".to_string()];
            *y_labels = vec!["X".to_string()];
            // 意図的な不整合: 行は 1 だが列が 1 (x_labels は 2)。
            *cells = vec![vec![None]];
        }
        let err = validate_spec(&spec, &default_limits()).unwrap_err();
        assert!(
            err.contains("cells") || err.contains("不一致"),
            "expected shape mismatch error, got: {err}"
        );
    }

    #[test]
    fn wordcloud_negative_size_rejected() {
        use crate::ir::{ChartKind, WordEntry};
        let s = ChartSpec {
            kind: ChartKind::WordCloud {
                entries: vec![WordEntry {
                    text: "A".to_string(),
                    size: -5.0,
                    color: None,
                }],
                min_rotation: -90.0,
                max_rotation: 0.0,
                rotation_steps: 2,
                padding: 2.0,
            },
            series: vec![],
            categories: vec![],
            ..base_spec()
        };
        assert!(validate_spec(&s, &default_limits()).is_err());
    }
}
