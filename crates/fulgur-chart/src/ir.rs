//! IR: フロントエンド(DSL) と描画コアの安定境界。

/// 解決済みの色（不透明 RGB + アルファ）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32, // 0.0–1.0
}

/// 散布図(scatter)/バブル(bubble)の点データ。`x`/`y` は各軸の scale で写像される数値、
/// `r` は任意の半径。
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

/// Geometry and optional side colors for a filled line area.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaFill {
    pub target: AreaFillTarget,
    pub above: Option<Color>,
    pub below: Option<Color>,
}

/// Destination for a line area's closed polygon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AreaFillTarget {
    Origin,
    Start,
    End,
    Value(f64),
    /// Index into `ChartSpec.series` after mixed dataset order has been applied.
    Dataset(usize),
    Stack,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum XPositions {
    #[default]
    Category,
    Temporal {
        unix_millis: Vec<i64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LineInterpolation {
    #[default]
    Linear,
    CatmullRom {
        tension: f64,
    },
    Monotone,
}

/// Line segment stepping direction, independent from input-schema terminology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMode {
    Before,
    After,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SizeMode {
    #[default]
    Canvas,
    PlotArea,
}

/// Optional Chart.js per-dataset bar geometry overrides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BarGeometryOptions {
    pub category_percentage: Option<f64>,
    pub bar_percentage: Option<f64>,
    pub bar_thickness: Option<BarThickness>,
    pub max_bar_thickness: Option<f64>,
    pub min_bar_length: Option<f64>,
    pub border_radius: Option<BarBorderRadius>,
}

impl BarGeometryOptions {
    /// Whether these options change category placement or bar dimensions.
    /// Styling-only fields such as `border_radius` must not opt charts out of legacy geometry.
    pub(crate) fn has_geometry_controls(self) -> bool {
        self.category_percentage.is_some()
            || self.bar_percentage.is_some()
            || self.bar_thickness.is_some()
            || self.max_bar_thickness.is_some()
            || self.min_bar_length.is_some()
    }
}

/// Chart.js per-dataset bar corner radius in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarBorderRadius {
    Uniform(f64),
    Corners {
        top_left: Option<f64>,
        top_right: Option<f64>,
        bottom_left: Option<f64>,
        bottom_right: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarThickness {
    Pixels(f64),
    Flex,
}

/// Chart.js pie/doughnut inner-radius setting. Pixel values are absolute;
/// percentage values are relative to the chart's outer radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PieCutout {
    Pixels(f64),
    Percent(f64),
}

impl PieCutout {
    /// Resolve the inner radius and clamp it to a finite range within the outer radius.
    pub fn inner_radius(self, outer_radius: f64) -> f64 {
        let outer_radius = if outer_radius.is_finite() {
            outer_radius.max(0.0)
        } else {
            0.0
        };
        let requested = match self {
            Self::Pixels(pixels) => pixels,
            Self::Percent(percent) => outer_radius * (percent / 100.0),
        };
        if requested.is_finite() {
            requested.clamp(0.0, outer_radius)
        } else if requested.is_sign_positive() {
            outer_radius
        } else {
            0.0
        }
    }

    pub fn is_doughnut(self) -> bool {
        match self {
            Self::Pixels(value) | Self::Percent(value) => value > 0.0,
        }
    }
}

/// Per-dataset Chart.js arc spacing, displacement, and corner radii.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PieGeometryOptions {
    pub spacing: f64,
    /// Scalar values are stored as a one-item vector; empty means the default zero offset.
    pub offsets: Vec<f64>,
    /// Scalar values are stored as a one-item vector; empty means square corners.
    pub border_radii: Vec<ArcBorderRadius>,
}

impl PieGeometryOptions {
    pub fn offset_at(&self, index: usize) -> f64 {
        if self.offsets.is_empty() {
            0.0
        } else {
            self.offsets[index % self.offsets.len()]
        }
    }

    pub fn border_radius_at(&self, index: usize) -> ArcBorderRadius {
        if self.border_radii.is_empty() {
            ArcBorderRadius::Uniform(0.0)
        } else {
            self.border_radii[index % self.border_radii.len()]
        }
    }
}

/// Resolved Chart.js arc corner radii in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArcBorderRadius {
    Uniform(f64),
    Corners {
        outer_start: f64,
        outer_end: f64,
        inner_start: f64,
        inner_end: f64,
    },
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
    pub area: bool, // line のとき塗りつぶすか
    /// Chart.js-specific target and colors. `None` preserves other frontends' legacy area rules.
    pub area_fill: Option<AreaFill>,
    pub interpolation: LineInterpolation,
    /// Whether a line connects across missing data points.
    pub span_gaps: bool,
    /// Optional stepped-line mode. Layout applies it in preference to interpolation.
    pub step_mode: Option<StepMode>,
    /// 描画種別。混合チャートでのみ意味を持つ(単一種別では未使用)。
    pub series_type: SeriesType,
    /// Optional stack group id. Chart.js datasets normalize omitted ids to their type default;
    /// other frontends leave this unset to retain their historical single-stack behavior.
    pub stack: Option<String>,
    /// Chart.js bar geometry controls, when supplied by that frontend.
    pub bar_geometry: Option<BarGeometryOptions>,
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
///
/// `draw_ticks` の既定は fulgur では `false`(Chart.js は `true`)。
/// これは v0 以来 fulgur が軸目盛の短線を描画してこなかった経緯を尊重し、
/// 既存 fixture のスナップショットが Task 9 で回帰しないようにするための
/// **意図的な乖離**。Chart.js の既定に合わせたいユーザは
/// `"grid": {"drawTicks": true}` で明示的にオプトインする。
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
            // fulgur の後方互換: Chart.js の既定 (true) からの意図的乖離。
            // 既存チャートに tick 短線が突然生えるのを避けるための選択。
            draw_ticks: false,
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

/// cartesian 軸のスケール種別。カテゴリ軸(chart種別で暗黙決定)には適用しない。
/// 数値軸(value axis)のみが Linear/Logarithmic を切り替える。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ScaleKind {
    #[default]
    Linear,
    Logarithmic,
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
    /// 数値軸の目盛スケール種別。カテゴリ軸(XPositions::Category が支配する軸)では
    /// 意味を持たないが、AxisSpec は x/y 共通型のため常に存在する。`Bar{..}` / `Line`
    /// の値軸と `Scatter` / `Bubble` の数値軸が Logarithmic を消費する。
    pub scale_kind: ScaleKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LegendPos {
    Top,
    Bottom,
    Left,
    Right,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LegendAlign {
    Start,
    #[default]
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendPointStyle {
    Circle,
    Cross,
    CrossRot,
    Dash,
    Line,
    Rect,
    RectRounded,
    RectRot,
    Star,
    Triangle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegendTitlePadding {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// Resolved Chart.js legend styling. `None` values retain the renderer defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegendOptions {
    pub align: LegendAlign,
    pub reverse: bool,
    pub labels_color: Option<Color>,
    pub labels_font_size: Option<f64>,
    pub labels_font_family: Option<String>,
    pub labels_font_weight: Option<String>,
    pub labels_font_style: Option<String>,
    pub labels_padding: Option<f64>,
    pub labels_box_width: Option<f64>,
    pub labels_box_height: Option<f64>,
    pub labels_use_point_style: bool,
    pub labels_point_style: Option<LegendPointStyle>,
    pub title_display: bool,
    pub title_color: Option<Color>,
    pub title_font_size: Option<f64>,
    pub title_font_family: Option<String>,
    pub title_font_weight: Option<String>,
    pub title_font_style: Option<String>,
    pub title_padding: LegendTitlePadding,
}

/// Radar / polarArea の r スケール。既存の `AxisSpec` は cartesian 向けに
/// title/offset/grid を含むため再利用しない。cartesian の
/// `suggestedMin/suggestedMax/beginAtZero` と同じセマンティクス:
/// - `min` / `max`: hard override (データ範囲外でも従う)
/// - `suggested_min` / `suggested_max`: expand-only (データ範囲を広げる方向のみ)
/// - `begin_at_zero`: true でドメインに 0 を含める
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialAxis {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub suggested_min: Option<f64>,
    pub suggested_max: Option<f64>,
    pub begin_at_zero: bool,
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
    Line {
        /// 積み上げ area。Vega-Lite の mark:"area" + color channel で既定 true
        /// (encoding.y.stack: null で false)。Chart.js フロントエンドでは値軸の
        /// `scales.<axis>.stacked` がこのフラグを制御する。
        stacked: bool,
        /// 積み上げ時に欠損データをその系列の line geometry で gap として扱う。
        /// Chart.js は true、欠損を0値として帯を保つ Vega-Lite stacked area は false。
        stacked_missing_values_are_gaps: bool,
    }, // area/tension は Series 側
    Pie {
        cutout: PieCutout,
        dataset_options: Vec<PieGeometryOptions>,
    }, // zero cutout = pie, positive cutout = doughnut
    Scatter, // 線形 x × 線形 y。点データ(Series.points)を使う
    Bubble,  // scatter と同じ枠組み。半径は point.r(第3次元)を使う
    Radar,   // 極座標。カテゴリ=スポーク、系列ごとに多角形を重ねる
    Mixed,   // 共有カテゴリ x・線形 y に bar+line を重ねる。種別は Series.series_type
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
    pub x_positions: XPositions,
    pub x_axis: AxisSpec,
    pub y_axis: AxisSpec,
    pub legend: LegendPos,
    pub legend_options: LegendOptions,
    pub legend_title: Option<String>,
    pub title: Option<String>,
    pub width: f64,
    pub height: f64,
    pub size_mode: SizeMode,
    /// データラベルを描画するか(frontend で解決済み)。
    pub data_labels: bool,
    /// 視覚トークンのテーマ(frontend で解決済み)。
    pub theme: Theme,
    /// line/area 用デシメーション設定(frontend で解決済み)。
    pub decimation: Decimation,
    /// Radar / polarArea 専用の r スケール。他 kind では常に None。
    pub radial_axis: Option<RadialAxis>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    #[test]
    fn new_line_contracts_have_backward_compatible_defaults() {
        assert_eq!(XPositions::default(), XPositions::Category);
        assert_eq!(LineInterpolation::default(), LineInterpolation::Linear);
        assert_eq!(SizeMode::default(), SizeMode::Canvas);
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
            area_fill: None,
            interpolation: LineInterpolation::Linear,
            span_gaps: false,
            step_mode: None,
            stack: None,
            bar_geometry: None,
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
            area_fill: None,
            interpolation: LineInterpolation::Linear,
            span_gaps: false,
            step_mode: None,
            stack: None,
            bar_geometry: None,
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
    fn pie_cutout_inner_radius_uses_units_and_clamps() {
        assert_eq!(PieCutout::Pixels(12.0).inner_radius(100.0), 12.0);
        assert_eq!(PieCutout::Percent(25.0).inner_radius(100.0), 25.0);
        assert_eq!(PieCutout::Pixels(-12.0).inner_radius(100.0), 0.0);
        assert_eq!(PieCutout::Pixels(120.0).inner_radius(100.0), 100.0);
        assert_eq!(PieCutout::Pixels(f64::INFINITY).inner_radius(100.0), 100.0);
        assert_eq!(
            PieCutout::Pixels(f64::NEG_INFINITY).inner_radius(100.0),
            0.0
        );
        assert_eq!(PieCutout::Percent(50.0).inner_radius(f64::NAN), 0.0);
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
            area_fill: None,
            interpolation: LineInterpolation::Linear,
            span_gaps: false,
            step_mode: None,
            stack: None,
            bar_geometry: None,
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
    fn axis_grid_default_matches_fulgur_backward_compat() {
        // fulgur の既定は Chart.js と一部乖離: draw_ticks=false。
        // v0 以来 tick 短線を描いてこなかったため、既存スナップショットを保護する
        // 選択的な既定値の反転。ユーザは drawTicks:true で明示オプトインする。
        let g = AxisGrid::default();
        assert!(g.display);
        assert!((g.line_width - 1.0).abs() < 1e-9);
        assert!(!g.draw_ticks);
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

#[cfg(test)]
mod radial_axis_tests {
    use super::*;

    /// ChartSpec を最小構成で組んで `radial_axis` フィールドの既定を検証する。
    /// `ChartSpec` は `Default` を実装していないので、リテラル構築し `radial_axis: None`
    /// を明示せずに初期化パスをすべて通ることを確認する。
    fn minimal_spec() -> ChartSpec {
        ChartSpec {
            kind: ChartKind::Line {
                stacked: false,
                stacked_missing_values_are_gaps: false,
            },
            series: vec![],
            categories: vec![],
            x_positions: XPositions::default(),
            x_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: false,
                offset: false,
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
                scale_kind: ScaleKind::Linear,
            },
            y_axis: AxisSpec {
                title: None,
                min: None,
                max: None,
                suggested_min: None,
                suggested_max: None,
                begin_at_zero: false,
                offset: false,
                grid: AxisGrid::default(),
                border: AxisBorder::default(),
                scale_kind: ScaleKind::Linear,
            },
            legend: LegendPos::None,
            legend_options: LegendOptions::default(),
            legend_title: None,
            title: None,
            width: 600.0,
            height: 400.0,
            size_mode: SizeMode::default(),
            data_labels: false,
            theme: Theme::default(),
            decimation: Decimation::default(),
            radial_axis: None,
        }
    }

    #[test]
    fn radial_axis_default_is_none_on_chart_spec() {
        let spec = minimal_spec();
        assert!(spec.radial_axis.is_none());
    }

    #[test]
    fn radial_axis_stores_all_five_knobs() {
        let a = RadialAxis {
            min: Some(0.0),
            max: Some(100.0),
            suggested_min: Some(-5.0),
            suggested_max: Some(120.0),
            begin_at_zero: true,
        };
        assert_eq!(a.min, Some(0.0));
        assert_eq!(a.max, Some(100.0));
        assert_eq!(a.suggested_min, Some(-5.0));
        assert_eq!(a.suggested_max, Some(120.0));
        assert!(a.begin_at_zero);
    }
}
