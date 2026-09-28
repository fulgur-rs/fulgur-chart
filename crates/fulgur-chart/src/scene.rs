//! 描画プリミティブの中間表現。幾何 + スタイルのみを持ち、解釈は含まない。

use crate::ir::Color;

/// テキストの水平アンカー。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// User-space rectangle used to clip a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
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
        /// 破線パターン(空なら実線)。SVG `stroke-dasharray` と tiny-skia `StrokeDash` に渡す。
        /// 空 `Vec` = 実線(後方互換のデフォルト)。
        dash: Vec<f64>,
    },
    /// 折れ線（塗りなし）。
    Polyline {
        points: Vec<(f64, f64)>,
        stroke: Color,
        stroke_width: f64,
    },
    /// Dashed version of a polyline used for dataset line styling.
    StyledPolyline {
        points: Vec<(f64, f64)>,
        stroke: Color,
        stroke_width: f64,
        dash: Vec<f64>,
        dash_offset: f64,
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
    /// Path whose fill and stroke are clipped to a user-space rectangle.
    ClippedPath {
        /// SVG path data using the same restricted commands as Path.
        d: String,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f64,
        /// Boxed so this rare primitive does not increase the size of every Prim.
        clip: Box<ClipRect>,
    },
    /// Dashed version of a path used for dataset line styling.
    StyledPath {
        /// SVG path data using the same restricted commands as `Prim::Path`.
        d: String,
        stroke: Color,
        stroke_width: f64,
        dash: Vec<f64>,
        dash_offset: f64,
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
    /// Circle whose fill and stroke are clipped to a user-space rectangle.
    ClippedCircle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Color,
        stroke: Color,
        stroke_width: f64,
        clip: Box<ClipRect>,
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
    /// Text with explicit SVG font attributes. Kept behind a `Box` so this rare variant does not
    /// enlarge every `Prim` stored in a scene. Raster output uses the selected font face and
    /// approximates weight/style where possible.
    StyledText(Box<StyledText>),
    /// Ordered child primitives rendered with a user-space translation and optional clip.
    /// The clip rectangle is expressed in this group's local coordinate system.
    Group {
        translate_x: f64,
        translate_y: f64,
        clip: Option<Box<ClipRect>>,
        children: Vec<Prim>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyledText {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub anchor: Anchor,
    pub fill: Color,
    pub content: String,
    pub rotate_deg: Option<f64>,
    pub font_family: Option<String>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
}

/// 1枚のチャート画像。
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Prim>,
}

/// Visit each primitive in depth-first input order with the translation applied to its local
/// coordinates. Group nodes are visited too, using their own effective translation.
pub(crate) fn visit_prims<'a>(
    items: &'a [Prim],
    parent_x: f64,
    parent_y: f64,
    visitor: &mut impl FnMut(&'a Prim, f64, f64),
) {
    for prim in items {
        let (translate_x, translate_y) = match prim {
            Prim::Group {
                translate_x,
                translate_y,
                ..
            } => (parent_x + translate_x, parent_y + translate_y),
            _ => (parent_x, parent_y),
        };
        visitor(prim, translate_x, translate_y);
        if let Prim::Group { children, .. } = prim {
            visit_prims(children, translate_x, translate_y, visitor);
        }
    }
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
                dash: Vec::new(),
            }],
        };
        assert!(!s.has_opaque_background());
    }

    #[test]
    fn translated_group_visits_primitives_in_depth_first_input_order() {
        let scene = Scene {
            width: 100.0,
            height: 80.0,
            items: vec![
                Prim::Line {
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
                    dash: Vec::new(),
                },
                Prim::Group {
                    translate_x: 13.0,
                    translate_y: 7.0,
                    clip: None,
                    children: vec![
                        Prim::Rect {
                            x: 1.0,
                            y: 2.0,
                            w: 3.0,
                            h: 4.0,
                            fill: Color {
                                r: 1,
                                g: 2,
                                b: 3,
                                a: 1.0,
                            },
                        },
                        Prim::Group {
                            translate_x: 2.0,
                            translate_y: 3.0,
                            clip: None,
                            children: vec![Prim::Circle {
                                cx: 1.0,
                                cy: 1.0,
                                r: 1.0,
                                fill: Color {
                                    r: 4,
                                    g: 5,
                                    b: 6,
                                    a: 1.0,
                                },
                                stroke: Color {
                                    r: 0,
                                    g: 0,
                                    b: 0,
                                    a: 1.0,
                                },
                                stroke_width: 0.0,
                            }],
                        },
                    ],
                },
            ],
        };

        let mut visits = Vec::new();
        visit_prims(&scene.items, 0.0, 0.0, &mut |prim, x, y| {
            if matches!(
                prim,
                Prim::Line { .. } | Prim::Rect { .. } | Prim::Circle { .. }
            ) {
                visits.push((
                    match prim {
                        Prim::Line { .. } => "line",
                        Prim::Rect { .. } => "rect",
                        Prim::Circle { .. } => "circle",
                        _ => unreachable!(),
                    },
                    x,
                    y,
                ));
            }
        });
        assert_eq!(
            visits,
            [
                ("line", 0.0, 0.0),
                ("rect", 13.0, 7.0),
                ("circle", 15.0, 10.0)
            ]
        );
    }
}
