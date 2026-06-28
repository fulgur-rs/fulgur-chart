//! Scene → 決定的な SVG 文字列。座標・寸法・不透明度はすべて fmt_num を通す。

use crate::ir::Color;
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use std::fmt::Write;

pub fn render_svg(scene: &Scene, font_family: &str) -> String {
    let mut s = String::new();
    let w = fmt_num(scene.width);
    let h = fmt_num(scene.height);
    write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    )
    .unwrap();

    // defs: GradientPath を出現順に grad{n} で採番(userSpaceOnUse・水平)。
    let mut grad_defs = String::new();
    let mut gi = 0usize;
    for item in &scene.items {
        if let Prim::GradientPath {
            x0,
            x1,
            stop0,
            stop1,
            ..
        } = item
        {
            write_linear_gradient(&mut grad_defs, gi, *x0, *x1, stop0, stop1);
            gi += 1;
        }
    }
    if gi > 0 {
        s.push_str("<defs>");
        s.push_str(&grad_defs);
        s.push_str("</defs>");
    }

    // 各プリミティブの出力は write_prim が担う。GradientPath は出現順に grad{n} を
    // 参照する必要があるため、defs パスと同じ順序で採番するカウンタを渡す。
    let mut grad_idx = 0usize;
    for item in &scene.items {
        write_prim(&mut s, item, font_family, &mut grad_idx);
    }
    s.push_str("</svg>\n");
    s
}

fn write_linear_gradient(
    s: &mut String,
    idx: usize,
    x0: f64,
    x1: f64,
    stop0: &Color,
    stop1: &Color,
) {
    let x0f = fmt_num(x0);
    let x1f = fmt_num(x1);
    let c0 = color_hex(stop0);
    let c1 = color_hex(stop1);
    let o0 = opacity_attr("stop-opacity", stop0.a);
    let o1 = opacity_attr("stop-opacity", stop1.a);
    write!(
        s,
        r#"<linearGradient id="grad{idx}" gradientUnits="userSpaceOnUse" x1="{x0f}" y1="0" x2="{x1f}" y2="0"><stop offset="0" stop-color="{c0}"{o0}/><stop offset="1" stop-color="{c1}"{o1}/></linearGradient>"#
    )
    .unwrap();
}

/// 色を小文字 `#rrggbb` に整形する。
fn color_hex(c: &Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// alpha が 1.0 未満のとき ` {name}="{value}"` を出す。1.0 以上なら空。
fn opacity_attr(name: &str, a: f32) -> String {
    if a < 1.0 {
        let v = fmt_num(a as f64);
        format!(r#" {name}="{v}""#)
    } else {
        String::new()
    }
}

fn write_prim(s: &mut String, prim: &Prim, font_family: &str, grad_idx: &mut usize) {
    match prim {
        Prim::Rect { x, y, w, h, fill } => {
            let x = fmt_num(*x);
            let y = fmt_num(*y);
            let w = fmt_num(*w);
            let h = fmt_num(*h);
            let hex = color_hex(fill);
            let op = opacity_attr("fill-opacity", fill.a);
            write!(
                s,
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{hex}"{op}/>"#
            )
            .unwrap();
        }
        Prim::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
        } => {
            let x1 = fmt_num(*x1);
            let y1 = fmt_num(*y1);
            let x2 = fmt_num(*x2);
            let y2 = fmt_num(*y2);
            let hex = color_hex(stroke);
            let sw = fmt_num(*stroke_width);
            let op = opacity_attr("stroke-opacity", stroke.a);
            write!(
                s,
                r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{hex}" stroke-width="{sw}"{op}/>"#
            )
            .unwrap();
        }
        Prim::Polyline {
            points,
            stroke,
            stroke_width,
        } => {
            let mut pts = String::new();
            for (i, (px, py)) in points.iter().enumerate() {
                if i > 0 {
                    pts.push(' ');
                }
                let px = fmt_num(*px);
                let py = fmt_num(*py);
                write!(pts, "{px},{py}").unwrap();
            }
            let hex = color_hex(stroke);
            let sw = fmt_num(*stroke_width);
            let op = opacity_attr("stroke-opacity", stroke.a);
            write!(
                s,
                r#"<polyline points="{pts}" fill="none" stroke="{hex}" stroke-width="{sw}"{op}/>"#
            )
            .unwrap();
        }
        Prim::Path {
            d,
            fill,
            stroke,
            stroke_width,
        } => {
            let fill_attr = match fill {
                Some(c) => color_hex(c),
                None => "none".to_string(),
            };
            let stroke_attr = match stroke {
                Some(c) => color_hex(c),
                None => "none".to_string(),
            };
            let mut tail = String::new();
            if stroke.is_some() {
                let sw = fmt_num(*stroke_width);
                write!(tail, r#" stroke-width="{sw}""#).unwrap();
            }
            if let Some(c) = fill {
                tail.push_str(&opacity_attr("fill-opacity", c.a));
            }
            if let Some(c) = stroke {
                tail.push_str(&opacity_attr("stroke-opacity", c.a));
            }
            write!(
                s,
                r#"<path d="{d}" fill="{fill_attr}" stroke="{stroke_attr}"{tail}/>"#
            )
            .unwrap();
        }
        // GradientPath: render_svg の defs パスが出力した grad{n} を url() で参照する。
        // 採番は defs パスと同じ「出現順」で、grad_idx を GradientPath ごとに進める。
        Prim::GradientPath { d, .. } => {
            write!(
                s,
                r#"<path d="{d}" fill="url(#grad{grad_idx})" stroke="none"/>"#
            )
            .unwrap();
            *grad_idx += 1;
        }
        Prim::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
            stroke_width,
        } => {
            let cx = fmt_num(*cx);
            let cy = fmt_num(*cy);
            let r = fmt_num(*r);
            let fill_hex = color_hex(fill);
            write!(
                s,
                r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill_hex}""#
            )
            .unwrap();
            if *stroke_width > 0.0 {
                let stroke_hex = color_hex(stroke);
                let sw = fmt_num(*stroke_width);
                write!(s, r#" stroke="{stroke_hex}" stroke-width="{sw}""#).unwrap();
            }
            s.push_str(&opacity_attr("fill-opacity", fill.a));
            if *stroke_width > 0.0 {
                s.push_str(&opacity_attr("stroke-opacity", stroke.a));
            }
            s.push_str("/>");
        }
        Prim::Text {
            x,
            y,
            size,
            anchor,
            fill,
            content,
            rotate_deg,
        } => {
            let xv = fmt_num(*x);
            let yv = fmt_num(*y);
            let size = fmt_num(*size);
            let anchor = match anchor {
                Anchor::Start => "start",
                Anchor::Middle => "middle",
                Anchor::End => "end",
            };
            let hex = color_hex(fill);
            let op = opacity_attr("fill-opacity", fill.a);
            let escaped = xml_escape(content);
            // font_family は --font のフォント name table 由来(信頼できない)になり得る。
            // 二重引用符属性なので `"` を含む family 名による属性インジェクションを防ぐ。
            let fam = xml_escape_attr(font_family);
            let transform = rotate_deg
                .map(|d| format!(" transform=\"rotate({},{},{})\"", fmt_num(d), xv, yv))
                .unwrap_or_default();
            write!(
                s,
                r#"<text x="{xv}" y="{yv}"{transform} font-family="{fam}" font-size="{size}" text-anchor="{anchor}" fill="{hex}"{op}>{escaped}</text>"#
            )
            .unwrap();
        }
    }
}

/// XML テキストエスケープ。`&` を最初に処理してから `<`、`>` の順。
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// XML 属性値用エスケープ。テキスト用に加えて二重引用符 `"` も実体参照化し、
/// 二重引用符属性の早期終端によるインジェクションを防ぐ。
fn xml_escape_attr(s: &str) -> String {
    xml_escape(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Color;
    use crate::scene::*;

    fn black() -> Color {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 1.0,
        }
    }
    fn blue() -> Color {
        Color {
            r: 54,
            g: 162,
            b: 235,
            a: 1.0,
        }
    }

    #[test]
    fn svg_header_and_footer() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\" viewBox=\"0 0 100 50\">"
        ));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn rect_uses_fmt_num_and_hex() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Rect {
                x: 1.005,
                y: 2.0,
                w: 10.0,
                h: 20.0,
                fill: blue(),
            }],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(r##"<rect x="1" y="2" width="10" height="20" fill="#36a2eb"/>"##),
            "got: {svg}"
        );
    }

    #[test]
    fn rect_with_alpha_emits_fill_opacity() {
        let scene = Scene {
            width: 10.0,
            height: 10.0,
            items: vec![Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                fill: Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 0.5,
                },
            }],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(r##"fill="#010203" fill-opacity="0.5"/>"##),
            "got: {svg}"
        );
    }

    #[test]
    fn text_anchor_family_and_escape() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Text {
                x: 5.0,
                y: 10.0,
                size: 12.0,
                anchor: Anchor::Middle,
                fill: black(),
                content: "a<b & c>d".into(),
                rotate_deg: None,
            }],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(svg.contains(r#"font-family="Noto Sans JP, sans-serif""#));
        assert!(svg.contains(r#"font-size="12""#));
        assert!(svg.contains(r#"text-anchor="middle""#));
        assert!(svg.contains("a&lt;b &amp; c&gt;d</text>"));
    }

    #[test]
    fn text_anchor_start_and_end() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![
                Prim::Text {
                    x: 0.0,
                    y: 0.0,
                    size: 10.0,
                    anchor: Anchor::Start,
                    fill: black(),
                    content: "s".into(),
                    rotate_deg: None,
                },
                Prim::Text {
                    x: 0.0,
                    y: 0.0,
                    size: 10.0,
                    anchor: Anchor::End,
                    fill: black(),
                    content: "e".into(),
                    rotate_deg: None,
                },
            ],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(svg.contains(r#"text-anchor="start""#), "got: {svg}");
        assert!(svg.contains(r#"text-anchor="end""#), "got: {svg}");
    }

    #[test]
    fn line_and_polyline_and_circle() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![
                Prim::Line {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 10.0,
                    y2: 20.0,
                    stroke: black(),
                    stroke_width: 1.0,
                },
                Prim::Polyline {
                    points: vec![(0.0, 0.0), (5.0, 5.0), (10.0, 0.0)],
                    stroke: blue(),
                    stroke_width: 2.0,
                },
                Prim::Circle {
                    cx: 3.0,
                    cy: 4.0,
                    r: 2.5,
                    fill: blue(),
                    stroke: black(),
                    stroke_width: 0.0,
                },
            ],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(
                r##"<line x1="0" y1="0" x2="10" y2="20" stroke="#000000" stroke-width="1"/>"##
            ),
            "got: {svg}"
        );
        assert!(
            svg.contains(
                r##"<polyline points="0,0 5,5 10,0" fill="none" stroke="#36a2eb" stroke-width="2"/>"##
            ),
            "got: {svg}"
        );
        assert!(
            svg.contains(r##"<circle cx="3" cy="4" r="2.5" fill="#36a2eb"/>"##),
            "got: {svg}"
        );
    }

    #[test]
    fn polyline_with_stroke_alpha_emits_stroke_opacity() {
        let scene = Scene {
            width: 10.0,
            height: 10.0,
            items: vec![Prim::Polyline {
                points: vec![(0.0, 0.0), (1.0, 1.0)],
                stroke: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0.25,
                },
                stroke_width: 1.0,
            }],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(r#"stroke-width="1" stroke-opacity="0.25"/>"#),
            "got: {svg}"
        );
    }

    #[test]
    fn line_with_stroke_alpha_emits_stroke_opacity() {
        let scene = Scene {
            width: 10.0,
            height: 10.0,
            items: vec![Prim::Line {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
                stroke: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0.75,
                },
                stroke_width: 2.0,
            }],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(r#"stroke-width="2" stroke-opacity="0.75"/>"#),
            "got: {svg}"
        );
    }

    #[test]
    fn path_fill_only_and_stroke_only() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![
                Prim::Path {
                    d: "M0 0 L10 0 L10 10 Z".into(),
                    fill: Some(blue()),
                    stroke: None,
                    stroke_width: 0.0,
                },
                Prim::Path {
                    d: "M0 0 L10 10".into(),
                    fill: None,
                    stroke: Some(black()),
                    stroke_width: 3.0,
                },
            ],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(r##"<path d="M0 0 L10 0 L10 10 Z" fill="#36a2eb" stroke="none"/>"##),
            "got: {svg}"
        );
        assert!(
            svg.contains(
                r##"<path d="M0 0 L10 10" fill="none" stroke="#000000" stroke-width="3"/>"##
            ),
            "got: {svg}"
        );
    }

    #[test]
    fn path_with_both_fill_and_stroke_and_opacities() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Path {
                d: "M0 0 L1 1".into(),
                fill: Some(Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 0.5,
                }),
                stroke: Some(Color {
                    r: 4,
                    g: 5,
                    b: 6,
                    a: 0.25,
                }),
                stroke_width: 2.0,
            }],
        };
        let svg = render_svg(&scene, "Noto Sans JP, sans-serif");
        assert!(
            svg.contains(
                r##"<path d="M0 0 L1 1" fill="#010203" stroke="#040506" stroke-width="2" fill-opacity="0.5" stroke-opacity="0.25"/>"##
            ),
            "got: {svg}"
        );
    }

    #[test]
    fn circle_with_stroke_emits_correct_svg() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            items: vec![Prim::Circle {
                cx: 50.0,
                cy: 50.0,
                r: 10.0,
                fill: Color {
                    r: 153,
                    g: 102,
                    b: 255,
                    a: 0.5,
                },
                stroke: Color {
                    r: 153,
                    g: 102,
                    b: 255,
                    a: 1.0,
                },
                stroke_width: 1.0,
            }],
        };
        let svg = render_svg(&scene, "sans-serif");
        // stroke に alpha=1.0 のとき stroke-opacity は出ない
        assert!(
            svg.contains(r##"stroke="#9966ff" stroke-width="1""##),
            "stroke attrs missing or wrong: {svg}"
        );
        assert!(
            !svg.contains("stroke-opacity"),
            "stroke-opacity should be absent when alpha=1.0: {svg}"
        );
        // fill-opacity は fill の alpha=0.5 のとき出る
        assert!(
            svg.contains(r##"fill-opacity="0.5""##),
            "fill-opacity missing: {svg}"
        );
    }

    #[test]
    fn circle_with_zero_stroke_width_omits_stroke_attrs() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            items: vec![Prim::Circle {
                cx: 10.0,
                cy: 10.0,
                r: 5.0,
                fill: Color {
                    r: 54,
                    g: 162,
                    b: 235,
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
        };
        let svg = render_svg(&scene, "sans-serif");
        assert!(
            !svg.contains("stroke="),
            "stroke= should be absent when stroke_width=0: {svg}"
        );
    }

    #[test]
    fn deterministic_repeat() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
                fill: blue(),
            }],
        };
        assert_eq!(
            render_svg(&scene, "Noto Sans JP, sans-serif"),
            render_svg(&scene, "Noto Sans JP, sans-serif")
        );
    }

    #[test]
    fn full_output_is_byte_exact() {
        // 複数アイテムの完全出力を照合し、要素間にセパレータが入らないことと
        // ヘッダ・フッタの結合を固定する（byte-deterministic の確認）。
        let scene = Scene {
            width: 20.0,
            height: 10.0,
            items: vec![
                Prim::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 5.0,
                    h: 5.0,
                    fill: blue(),
                },
                Prim::Text {
                    x: 1.0,
                    y: 2.0,
                    size: 8.0,
                    anchor: Anchor::Start,
                    fill: black(),
                    content: "x".into(),
                    rotate_deg: None,
                },
            ],
        };
        let expected = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10">"##,
            r##"<rect x="0" y="0" width="5" height="5" fill="#36a2eb"/>"##,
            r##"<text x="1" y="2" font-family="Noto Sans JP, sans-serif" font-size="8" text-anchor="start" fill="#000000">x</text>"##,
            "</svg>\n",
        );
        assert_eq!(render_svg(&scene, "Noto Sans JP, sans-serif"), expected);
    }

    #[test]
    fn font_family_attribute_is_escaped() {
        // --font 由来の悪意ある family 名(二重引用符)が属性を破壊しないこと。
        let scene = Scene {
            width: 10.0,
            height: 10.0,
            items: vec![Prim::Text {
                x: 0.0,
                y: 0.0,
                size: 10.0,
                anchor: Anchor::Start,
                fill: black(),
                content: "x".into(),
                rotate_deg: None,
            }],
        };
        let svg = render_svg(&scene, r#"Evil" onload="boom"#);
        assert!(
            svg.contains("&quot;"),
            "二重引用符が実体参照化される: {svg}"
        );
        assert!(
            !svg.contains(r#"font-family="Evil" onload="#),
            "属性が早期終端していない: {svg}"
        );
    }

    #[test]
    fn gradient_path_emits_defs_and_url_ref() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            items: vec![Prim::GradientPath {
                d: "M0 0L10 0L10 10Z".to_string(),
                x0: 0.0,
                x1: 10.0,
                stop0: Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 0.5,
                },
                stop1: Color {
                    r: 0,
                    g: 128,
                    b: 0,
                    a: 0.5,
                },
            }],
        };
        let svg = render_svg(&scene, "sans-serif");
        assert!(svg.contains("<defs>"), "must emit defs");
        assert!(
            svg.contains("<linearGradient id=\"grad0\""),
            "deterministic id"
        );
        assert!(svg.contains("gradientUnits=\"userSpaceOnUse\""));
        assert!(svg.contains("stop-color=\"#ff0000\""));
        assert!(svg.contains("stop-color=\"#008000\""));
        assert!(
            svg.contains(r##"stop-opacity="0.5""##),
            "alpha<1 stop must emit stop-opacity"
        );
        assert!(
            svg.contains(r##"fill="url(#grad0)""##),
            "path must ref gradient"
        );
    }

    #[test]
    fn gradient_path_is_byte_deterministic() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            items: vec![Prim::GradientPath {
                d: "M0 0L5 0L5 5Z".into(),
                x0: 0.0,
                x1: 5.0,
                stop0: Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 1.0,
                },
                stop1: Color {
                    r: 4,
                    g: 5,
                    b: 6,
                    a: 1.0,
                },
            }],
        };
        assert_eq!(render_svg(&scene, "s"), render_svg(&scene, "s"));
    }

    #[test]
    fn text_with_rotate_deg_emits_transform() {
        let scene = Scene {
            width: 100.0,
            height: 50.0,
            items: vec![Prim::Text {
                x: 10.0,
                y: 20.0,
                size: 12.0,
                anchor: Anchor::Middle,
                fill: black(),
                content: "hello".into(),
                rotate_deg: Some(45.0),
            }],
        };
        let svg = render_svg(&scene, "sans-serif");
        assert!(
            svg.contains(r#"transform="rotate(45,10,20)""#),
            "got: {svg}"
        );
    }
}
