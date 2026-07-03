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
    /// 水平リニアグラデーションで塗る任意パス。sankey のリボンに使う。
    /// グラデーションは userSpace の x0→x1 で stop0→stop1 に補間する(y 方向は一定)。
    /// d は `Prim::Path` と同じく fmt_num 整形済みトークンのみを含むこと。
    GradientPath {
        d: String,
        /// グラデーション開始 x(stop0 の位置、ユーザ座標)。
        x0: f64,
        /// グラデーション終了 x(stop1 の位置、ユーザ座標)。
        x1: f64,
        stop0: Color,
        stop1: Color,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Color,
        stroke: Color,
        stroke_width: f64,
    },
    Text {
        x: f64,
        y: f64,
        size: f64,
        anchor: Anchor,
        fill: Color,
        content: String,
        rotate_deg: Option<f64>, // Some(deg) → SVG transform="rotate(deg,x,y)"
    },
}

/// 1枚のチャート画像。
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Prim>,
}

impl Scene {
    /// 最背面(items[0])が canvas 全面を覆う不透明 Rect のとき true。
    ///
    /// `build_scene` は `theme.background` 指定時に全面矩形を index 0 へ挿入するため、
    /// これは「不透明背景が敷かれている」ことと一致する。背景なし・半透明背景・部分被覆の
    /// 先頭矩形では false（＝最適化を適用せず安全側）。PNG/WebP エンコードで
    /// demultiply スキャンを省ける（全画素 α==255 を前提にできる）ための **必要条件**。
    /// 十分条件は encode 時に scale 依存の device 被覆判定と合成する。
    pub fn has_opaque_background(&self) -> bool {
        matches!(
            self.items.first(),
            Some(Prim::Rect { x, y, w, h, fill })
                if *x <= 0.0
                    && *y <= 0.0
                    && *x + *w >= self.width
                    && *y + *h >= self.height
                    && fill.a >= 1.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Color;

    fn full_rect(w: f64, h: f64, a: f32) -> Prim {
        Prim::Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
            fill: Color {
                r: 10,
                g: 20,
                b: 30,
                a,
            },
        }
    }

    #[test]
    fn opaque_full_canvas_rect_is_opaque_background() {
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![full_rect(100.0, 50.0, 1.0)],
        };
        assert!(s.has_opaque_background());
    }

    #[test]
    fn semi_transparent_bg_is_not_opaque() {
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![full_rect(100.0, 50.0, 0.5)],
        };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn empty_scene_is_not_opaque() {
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![],
        };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn partial_coverage_first_rect_is_not_opaque() {
        // 全幅に満たない先頭矩形は背景として扱わない。
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![full_rect(80.0, 50.0, 1.0)],
        };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn positive_offset_rect_is_not_opaque() {
        // x=10,y=10 は左上端を覆わない → *x<=0.0 / *y<=0.0 節を固定する。
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Rect {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 50.0,
                fill: Color {
                    r: 10,
                    g: 20,
                    b: 30,
                    a: 1.0,
                },
            }],
        };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn short_height_rect_is_not_opaque() {
        // h=40 は下端まで届かない → *y + *h >= self.height 節を固定する。
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 40.0,
                fill: Color {
                    r: 10,
                    g: 20,
                    b: 30,
                    a: 1.0,
                },
            }],
        };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn non_rect_first_item_is_not_opaque() {
        let s = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Line {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
                stroke: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 1.0,
                },
                stroke_width: 1.0,
            }],
        };
        assert!(!s.has_opaque_background());
    }
}
