//! IR: フロントエンド(DSL) と描画コアの安定境界。

/// 解決済みの色（不透明 RGB + アルファ）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32, // 0.0–1.0
}

/// 色は**データ点ごと**に持てる（pie のスライス別色が標準形のため）。
/// 長さ 1 のときは全点へブロードキャストする。`fill_at`/`stroke_at` で安全に参照する。
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    pub fill: Vec<Color>,   // len==1 でブロードキャスト、または点ごと
    pub stroke: Vec<Color>, // 同上
    pub stroke_width: f64,
    pub area: bool,   // line のとき塗りつぶすか
    pub tension: f64, // 0.0 = 直線
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

fn color_at(colors: &[Color], i: usize) -> Color {
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

#[derive(Clone, Debug, PartialEq)]
pub struct AxisSpec {
    pub title: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub begin_at_zero: bool,
    pub grid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LegendPos {
    Top,
    Bottom,
    Left,
    Right,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChartKind {
    Bar { horizontal: bool },
    Line,                     // area/tension は Series 側
    Pie { donut_ratio: f64 }, // 0.0 = pie, >0 = doughnut
}

/// 視覚トークンのテーマ。`options.theme` で上書きできる解決済みの値。
/// `Default` は現行の描画定数と**完全一致**する（テーマ未指定時の byte 一致を保証）。
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// 系列/スライスの自動配色に使う巡回パレット。
    pub palette: Vec<Color>,
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
            fill: vec![c(1, 2, 3)],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
        };
        assert_eq!(s.fill_at(0), c(1, 2, 3));
        assert_eq!(s.fill_at(2), c(1, 2, 3)); // ブロードキャスト
    }

    #[test]
    fn fill_at_indexes_per_point_colors() {
        let s = Series {
            name: "x".into(),
            values: vec![1.0, 2.0],
            fill: vec![c(10, 0, 0), c(0, 20, 0)],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
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
            fill: vec![],
            stroke: vec![],
            stroke_width: 1.0,
            area: false,
            tension: 0.0,
        };
        assert_eq!(s.stroke_at(0), c(0, 0, 0));
    }
}
