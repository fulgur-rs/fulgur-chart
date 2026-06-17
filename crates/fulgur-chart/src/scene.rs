//! 描画プリミティブの中間表現。幾何 + スタイルのみを持ち、解釈は含まない。

use crate::ir::Color;

/// テキストの水平アンカー。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// 描画プリミティブ。SVG要素に1対1で対応する。
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Color,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: Color,
        stroke_width: f64,
    },
    /// 折れ線（塗りなし）。
    Polyline {
        points: Vec<(f64, f64)>,
        stroke: Color,
        stroke_width: f64,
    },
    /// 任意パス。area塗り・pie扇形・曲線に使う。fill/strokeは任意。
    Path {
        /// SVG path data。`fmt_num` 整形済みのトークンとパスコマンドのみを含むこと。
        /// 生のユーザ文字列(系列名・ラベル等)を補間してはならない(無エスケープで出力される)。
        d: String,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f64,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Color,
    },
    Text {
        x: f64,
        y: f64,
        size: f64,
        anchor: Anchor,
        fill: Color,
        content: String,
    },
}

/// 1枚のチャート画像。
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Prim>,
}
