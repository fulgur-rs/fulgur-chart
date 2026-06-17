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
