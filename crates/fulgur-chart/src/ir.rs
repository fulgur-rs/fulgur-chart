//! IR: フロントエンド(DSL) と描画コアの安定境界。

/// 解決済みの色（不透明 RGB + アルファ）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32, // 0.0–1.0
}

/// 散布図(scatter)/バブル(bubble)の点データ。`x`/`y` は線形座標、`r` は任意の半径。
/// カテゴリ系チャート(bar/line/pie)はこれを使わず `values` を使う。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub r: Option<f64>,
}

/// BoxPlot の5数要約。[min, q1, median, q3, max]。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxPoint {
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
}

/// ワードクラウドの 1 単語エントリ。
#[derive(Clone, Debug, PartialEq)]
pub struct WordEntry {
    /// 表示テキスト。
    pub text: String,
    /// フォントサイズ (px)。入力 data[] の値をそのまま使う。
    pub size: f64,
    /// 塗り色。None のときはパレット巡回。
    pub color: Option<Color>,
}

/// treemap の階層ノード。リーフは children 空・value はリーフ値。
/// グループは value=子の合算・children=サブノード。任意の深さにネストできる。
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode {
    pub label: String,
    pub value: f64,
    pub children: Vec<TreeNode>,
}

/// sankey のリンク(フロー)。ノード間のフロー量を表す。from/to はノードID(文字列)。
/// per-link 色上書き: chartjs-chart-sankey の data 要素 `color`/`colorFrom`/`colorTo` に対応。
/// None なら dataset レベル(`ChartKind::Sankey.color_from` / `color_to`)にフォールバック。
/// - `color_from`: from 側 stop 上書き
/// - `color_to`: to 側 stop 上書き
/// - `color` は parse 時に解決(color_from/color_to が個別未指定なら両方に流し込む)ため IR には持たない。
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLink {
    pub from: String,
    pub to: String,
    pub flow: f64,
    pub color_from: Option<Color>,
    pub color_to: Option<Color>,
}

/// 系列ごとの描画種別。混合チャート(bar+line)で dataset 別 type を表す。
/// 単一種別チャートでは全系列が同じ値になる(描画に影響しない既定は Bar)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeriesType {
    Bar,
    Line,
}

/// 色は**データ点ごと**に持てる（pie のスライス別色が標準形のため）。
/// 長さ 1 のときは全点へブロードキャストする。`fill_at`/`stroke_at` で安全に参照する。
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    /// scatter/bubble の点データ。カテゴリ系チャートでは空。
    pub points: Vec<Point>,
    pub fill: Vec<Color>,   // len==1 でブロードキャスト、または点ごと
    pub stroke: Vec<Color>, // 同上
    pub stroke_width: f64,
    pub area: bool,   // line のとき塗りつぶすか
    pub tension: f64, // 0.0 = 直線
    /// 描画種別。混合チャートでのみ意味を持つ(単一種別では未使用)。
    pub series_type: SeriesType,
    /// scatter のマーカー半径(chart.js pointRadius)。None なら既定値。
    /// bubble では point.r を優先し、欠落時のフォールバックに使う。
    pub point_radius: Option<f64>,
    /// boxplot の5数要約データ。boxplot 種別のみ使用、他は空。
    pub box_points: Vec<BoxPoint>,
    /// treemap の階層データ (トップレベルノードの forest)。treemap 種別のみ使用、他は空。
    pub tree: Vec<TreeNode>,
    /// sankey のリンク(フロー)配列。sankey 種別のみ使用、他は空。
    pub links: Vec<SankeyLink>,
}

impl Series {
    /// i 番目のデータ点の塗り色。空なら黒、len==1 ならブロードキャスト。
    pub fn fill_at(&self, i: usize) -> Color {
        color_at(&self.fill, i)
    }
    pub fn stroke_at(&self, i: usize) -> Color {
        color_at(&self.stroke, i)
    }
}

/// 要素番号 i から色を解決する共有ルール(空なら黒、len==1 ならブロードキャスト、
/// それ以外は i % len)。レンダラ(`fill_at`/`stroke_at` 経由)と意味モデル
/// (`model::colors_to_strings`)が同一経路を使い、モデルと描画の差異を防ぐ。
pub fn color_at(colors: &[Color], i: usize) -> Color {
    match colors.len() {
        0 => Color {
            r: 0,
            g: 0,
            b: 0,
            a: 1.0,
        },
        1 => colors[0],
        _ => colors[i % colors.len()],
    }
}

/// 軸タイトルの配置位置。chart.js の `title.align` に対応。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum AxisTitleAlign {
    Start,
    #[default]
    Center,
    End,
}

/// 軸タイトル。`text` は必須で、色/フォントサイズ/配置は任意。
#[derive(Clone, Debug, PartialEq)]
pub struct AxisTitle {
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f64>,
    pub align: AxisTitleAlign,
}

/// 軸のグリッド線設定。chart.js `scales.*.grid` に対応。
#[derive(Clone, Debug, PartialEq)]
pub struct AxisGrid {
    pub display: bool,
    pub color: Option<Color>,
    pub line_width: f64,
    pub draw_ticks: bool,
}

impl Default for AxisGrid {
    fn default() -> Self {
        Self {
            display: true,
            color: None,
            line_width: 1.0,
            draw_ticks: true,
        }
    }
}

/// 軸のボーダー(基線)設定。chart.js `scales.*.border` に対応。
#[derive(Clone, Debug, PartialEq)]
pub struct AxisBorder {
    pub display: bool,
    pub color: Option<Color>,
    pub width: f64,
    pub dash: Vec<f64>,
}

impl Default for AxisBorder {
    fn default() -> Self {
        Self {
            display: true,
            color: None,
            width: 1.0,
            dash: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisSpec {
    pub title: Option<AxisTitle>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub suggested_min: Option<f64>,
    pub suggested_max: Option<f64>,
    pub begin_at_zero: bool,
    /// chart.js category スケールの offset。true でカテゴリを band 中心へ寄せる
    /// (bar の既定挙動)。false は edge-to-edge(line の既定)。現状 line レイアウトの
    /// x 軸のみが消費する(y は line の値軸なので無描画)。
    pub offset: bool,
    pub grid: AxisGrid,
    pub border: AxisBorder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LegendPos {
    Top,
    Bottom,
    Left,
    Right,
    None,
}

/// outlabeledPie / outlabeledDoughnut の引き出しラベル設定。
#[derive(Clone, Debug, PartialEq)]
pub struct OutlabelConfig {
    /// ラベルテキストテンプレート。%l=カテゴリ名, %v=値, %p=パーセント。
    pub text: String,
    /// ラベル文字色。
    pub color: Color,
    /// ラベル背景色。None = スライス色を使用。
    pub background: Option<Color>,
    /// 引き出し線の長さ(px)。外周からこの距離だけ外側へ伸びる。
    pub stretch: f64,
}

impl Default for OutlabelConfig {
    fn default() -> Self {
        OutlabelConfig {
            text: "%l\n%p%".to_string(),
            color: Color {
                r: 255,
                g: 255,
                b: 255,
                a: 1.0,
            },
            background: None,
            stretch: 40.0,
        }
    }
}

/// sankey リンクの配色モード。chartjs-chart-sankey の colorMode に対応。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SankeyColorMode {
    From,
    To,
    Gradient,
}

/// sankey の x 方向レイアウトモード。chartjs の modeX に対応。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SankeyModeX {
    Edge,
    Even,
}

/// sankey のノードサイズ算出方式。chartjs の size に対応(max=既定)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SankeySize {
    Min,
    Max,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChartKind {
    Bar {
        horizontal: bool,
        /// index 軸 stacked: 配置(同スロット vs dodge)
        placement_stacked: bool,
        /// 値軸 stacked: 値累積・値域計算
        value_stacked: bool,
    },
    Line, // area/tension は Series 側
    Pie {
        donut_ratio: f64,
    }, // 0.0 = pie, >0 = doughnut
    Scatter, // 線形 x × 線形 y。点データ(Series.points)を使う
    Bubble, // scatter と同じ枠組み。半径は point.r(第3次元)を使う
    Radar, // 極座標。カテゴリ=スポーク、系列ごとに多角形を重ねる
    Mixed, // 共有カテゴリ x・線形 y に bar+line を重ねる。種別は Series.series_type
    Matrix {
        color_lo: Color, // min 値のセル色（白固定）
        color_hi: Color, // max 値のセル色（backgroundColor 由来）
    },
    /// Vega-Lite `mark: "rect"` (ヒートマップ)。
    /// scale 解決済み色を per-cell で持つ純粋 grid。`None` セルは描画スキップ(透過)。
    /// x_labels/y_labels は categories/series 経由ではなくここに直接持ち、layout 側は
    /// この variant の情報だけで描画する(既存 ChartKind::Matrix パスを触らないため)。
    VegaRect {
        /// 列ラベル(横軸カテゴリ)、first-seen 順。
        x_labels: Vec<String>,
        /// 行ラベル(縦軸カテゴリ)、first-seen 順。
        y_labels: Vec<String>,
        /// cells[row][col] = 解決済み Color または None(欠損/skip)。
        /// row: y_labels の index、col: x_labels の index。
        cells: Vec<Vec<Option<Color>>>,
    },
    /// QuickChart 互換の progress バー。軸なし水平バー。
    /// series[0].values=各バーの値、series.get(1).values=per-bar max(省略時100)。
    Progress,
    /// QuickChart 互換の boxplot。カテゴリ×5数要約(min/q1/median/q3/max)。
    BoxPlot,
    /// QuickChart 互換のスパークライン。軸・ラベル・凡例なしのミニマル折れ線。
    Sparkline,
    /// Chart.js v4 polarArea: 角度等分・半径が値に比例する極座標チャート。
    PolarArea,
    /// QuickChart radialGauge: 全円。値まで塗りつぶす弧 + トラック + 中央値テキスト。
    /// series[0].values[0]=値、series[0].fill[0]=塗り色。スカラ構造値はここに持つ。
    RadialGauge {
        min: f64,
        max: f64,
        track: Color,
        inner_ratio: f64, // centerPercentage/100
        rounded: bool,
        display_text: bool,
        /// centerArea.fontSize の上書き(px)。None なら内径比で自動算出。
        center_font_size: Option<f64>,
    },
    /// QuickChart gauge: 半円。color zone(series[0].values=累積閾値, series[0].fill=ゾーン色)
    /// + 針 + 値ラベル。value=針値、min=下端(max は閾値末尾)。
    Gauge {
        value: f64,
        min: f64,
        needle: Color,
        label: bool,        // valueLabel.display
        label_color: Color, // valueLabel.color
        label_bg: Color,    // valueLabel.backgroundColor
    },
    /// QuickChart 互換の outlabeledPie / outlabeledDoughnut。
    /// 各スライスから円外側へ引き出し線を描き、ラベルを外に配置する。
    OutlabeledPie {
        donut_ratio: f64,
        outlabel: OutlabelConfig,
    },
    /// QuickChart / chartjs-chart-treemap 互換の treemap。階層データを squarified で
    /// ネストした矩形に分割し、深さに応じた色で塗る。データは series[0].tree に持つ。
    Treemap,
    /// QuickChart / chartjs-chart-wordcloud 互換のワードクラウド。
    /// 単語の重要度をフォントサイズで表現し、アルキメデス螺旋で非重複配置する。
    WordCloud {
        entries: Vec<WordEntry>,
        /// 最小回転角度 (度)。デフォルト: -90.0
        min_rotation: f64,
        /// 最大回転角度 (度)。デフォルト: 0.0
        max_rotation: f64,
        /// 離散回転ステップ数。デフォルト: 2
        rotation_steps: u32,
        /// 各単語の周囲パディング (px)。デフォルト: 2.0
        padding: f64,
    },
    /// QuickChart / chartjs-chart-sankey 互換の sankey。ノード間フロー量を帯幅で表す。
    /// データは series[0].links に持つ。設定値は kind に保持(Gauge 同様)。
    Sankey {
        color_from: Color,
        color_to: Color,
        color_mode: SankeyColorMode,
        /// リンク塗りの不透明度(0.0–1.0)。chartjs default 0.5。
        alpha: f32,
        node_width: f64,
        node_padding: f64,
        mode_x: SankeyModeX,
        size: SankeySize,
        border: Color,
        border_width: f64,
        label_color: Color,
        /// ノードID→表示ラベル上書き。未登録は ID をそのまま表示。
        labels: std::collections::HashMap<String, String>,
        /// ノードID→priority(列内ソートキー)。空なら priority レイアウト無効。
        priority: std::collections::HashMap<String, f64>,
        /// ノードID→列番号(手動 x 指定)。
        columns: std::collections::HashMap<String, usize>,
    },
}

/// 視覚トークンのテーマ。`options.theme` で上書きできる解決済みの値。
/// `Default` は現行の描画定数と**完全一致**する（テーマ未指定時の byte 一致を保証）。
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// 系列/スライスの自動配色に使う巡回パレット。
    pub palette: Vec<Color>,
    /// カスタムパレットが指定されているかどうか。
    pub is_custom_palette: bool,
    /// グリッド線の色。
    pub grid_color: Color,
    /// テキスト/インクの色。
    pub text_color: Color,
    /// キャンバス背景色。None は背景なし(現行挙動)。
    pub background: Option<Color>,
    /// ラベル基準フォントサイズ(px)。タイトルは固定。
    pub font_size: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            palette: crate::palette::PALETTE.to_vec(),
            is_custom_palette: false,
            grid_color: Color {
                r: 224,
                g: 224,
                b: 224,
                a: 1.0,
            },
            text_color: Color {
                r: 102,
                g: 102,
                b: 102,
                a: 1.0,
            },
            background: None,
            font_size: 12.0,
        }
    }
}

/// デシメーションアルゴリズム（Chart.js 互換）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimationAlgorithm {
    MinMax,
    Lttb,
}

/// options.plugins.decimation の解決済み設定。
/// 既定は自動オン（enabled=true）。Chart.js（false）からの意図的乖離。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decimation {
    pub enabled: bool,
    pub algorithm: DecimationAlgorithm,
    /// lttb の目標サンプル数。None なら論理プロット幅px。
    pub samples: Option<f64>,
    /// 間引き発動の点数しきい値。None なら論理プロット幅px × 4。
    pub threshold: Option<f64>,
}

impl Default for Decimation {
    fn default() -> Self {
        Decimation {
            enabled: true,
            algorithm: DecimationAlgorithm::MinMax,
            samples: None,
            threshold: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub series: Vec<Series>,
    pub categories: Vec<String>,
    pub x_axis: AxisSpec,
    pub y_axis: AxisSpec,
    pub legend: LegendPos,
    pub title: Option<String>,
    pub width: f64,
    pub height: f64,
    /// データラベルを描画するか(frontend で解決済み)。
    pub data_labels: bool,
    /// 視覚トークンのテーマ(frontend で解決済み)。
    pub theme: Theme,
    /// line/area 用デシメーション設定(frontend で解決済み)。
    pub decimation: Decimation,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    #[test]
    fn fill_at_broadcasts_single_color() {
        let s = Series {
            name: "x".into(),
            values: vec![1.0, 2.0, 3.0],
            points: vec![],
            fill: vec![c(1, 2, 3)],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
        };
        assert_eq!(s.fill_at(0), c(1, 2, 3));
        assert_eq!(s.fill_at(2), c(1, 2, 3)); // ブロードキャスト
    }

    #[test]
    fn fill_at_indexes_per_point_colors() {
        let s = Series {
            name: "x".into(),
            values: vec![1.0, 2.0],
            points: vec![],
            fill: vec![c(10, 0, 0), c(0, 20, 0)],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
        };
        assert_eq!(s.fill_at(0), c(10, 0, 0));
        assert_eq!(s.fill_at(1), c(0, 20, 0));
        assert_eq!(s.fill_at(2), c(10, 0, 0)); // 巡回
    }

    #[test]
    fn stroke_at_empty_is_black() {
        let s = Series {
            name: "x".into(),
            values: vec![1.0],
            points: vec![],
            fill: vec![],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
        };
        assert_eq!(s.stroke_at(0), c(0, 0, 0));
    }

    #[test]
    fn theme_default_palette_is_not_custom() {
        let t = Theme::default();
        assert!(!t.is_custom_palette);
    }

    #[test]
    fn box_point_fields_accessible() {
        let bp = BoxPoint {
            min: 1.0,
            q1: 2.0,
            median: 3.0,
            q3: 4.0,
            max: 5.0,
        };
        assert_eq!(bp.median, 3.0);
        assert_eq!(bp.max - bp.min, 4.0);
    }

    #[test]
    fn outlabel_config_default_values() {
        let c = OutlabelConfig::default();
        assert_eq!(c.text, "%l\n%p%");
        assert!((c.stretch - 40.0).abs() < 1e-9);
        assert!(c.background.is_none());
        assert_eq!(c.color.r, 255);
        assert_eq!(c.color.a, 1.0);
    }

    #[test]
    fn tree_node_is_recursive() {
        let leaf = TreeNode {
            label: "a".into(),
            value: 3.0,
            children: vec![],
        };
        let group = TreeNode {
            label: "g".into(),
            value: 3.0,
            children: vec![leaf.clone()],
        };
        assert_eq!(group.children.len(), 1);
        assert_eq!(group.children[0].value, 3.0);
        assert!(leaf.children.is_empty());
    }

    #[test]
    fn axis_grid_default_is_chartjs_shape() {
        let g = AxisGrid::default();
        assert!(g.display);
        assert!((g.line_width - 1.0).abs() < 1e-9);
        assert!(g.draw_ticks);
        assert!(g.color.is_none());
    }

    #[test]
    fn axis_border_default_is_chartjs_shape() {
        let b = AxisBorder::default();
        assert!(b.display);
        assert!((b.width - 1.0).abs() < 1e-9);
        assert!(b.color.is_none());
        assert!(b.dash.is_empty());
    }

    #[test]
    fn axis_title_align_default_is_center() {
        let a: AxisTitleAlign = Default::default();
        assert_eq!(a, AxisTitleAlign::Center);
    }
}
