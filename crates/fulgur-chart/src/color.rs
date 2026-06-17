//! 色文字列パーサ。chart.js spec の色文字列を [`crate::ir::Color`] へ変換する。
//!
//! 対応形式: `#RGB` / `#RRGGBB` / `#RRGGBBAA` / `rgb(r,g,b)` / `rgba(r,g,b,a)` /
//! `hsl(h, s%, l%)` / `hsla(h, s%, l%, a)` / CSS 拡張色名（148 色）+ `transparent`。
//! HSL はカンマ区切り構文のみ対応し、`s`/`l` は末尾 `%` 必須。範囲外の値（例:
//! `rgb(256,0,0)` や `hsl(0,150%,50%)`）はクランプせず None。すべて決定的な固定小数点
//! 計算で、ラスタライズ出力の再現性を保つ（HashMap 不使用）。

use crate::ir::Color;

/// 色文字列を [`Color`] へ変換する。解釈できなければ `None`。
///
/// 前後空白は無視し、`rgb`/`rgba` の関数名と 16 進は大文字小文字を問わない。
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(hex) = s.strip_prefix('#') {
        parse_hex(hex)
    } else if let Some(inner) = s.strip_prefix("rgba(").and_then(strip_close_paren) {
        parse_rgb_args(inner, true)
    } else if let Some(inner) = s.strip_prefix("rgb(").and_then(strip_close_paren) {
        parse_rgb_args(inner, false)
    } else if let Some(inner) = s.strip_prefix("hsla(").and_then(strip_close_paren) {
        parse_hsl_args(inner, true)
    } else if let Some(inner) = s.strip_prefix("hsl(").and_then(strip_close_paren) {
        parse_hsl_args(inner, false)
    } else {
        named_color(&s)
    }
}

/// 末尾の `)` を取り除く。なければ `None`（閉じ括弧欠落や末尾ゴミを拒否）。
fn strip_close_paren(s: &str) -> Option<&str> {
    s.strip_suffix(')')
}

/// `#` を除いた 16 進部分（小文字化済み）を解釈する。
fn parse_hex(hex: &str) -> Option<Color> {
    let bytes = hex.as_bytes();
    match bytes.len() {
        3 => {
            // 各桁を複製: a -> aa
            let r = hex_nibble(bytes[0])?;
            let g = hex_nibble(bytes[1])?;
            let b = hex_nibble(bytes[2])?;
            Some(Color {
                r: r * 17,
                g: g * 17,
                b: b * 17,
                a: 1.0,
            })
        }
        6 => Some(Color {
            r: hex_pair(bytes[0], bytes[1])?,
            g: hex_pair(bytes[2], bytes[3])?,
            b: hex_pair(bytes[4], bytes[5])?,
            a: 1.0,
        }),
        8 => Some(Color {
            r: hex_pair(bytes[0], bytes[1])?,
            g: hex_pair(bytes[2], bytes[3])?,
            b: hex_pair(bytes[4], bytes[5])?,
            a: f32::from(hex_pair(bytes[6], bytes[7])?) / 255.0,
        }),
        _ => None,
    }
}

/// ASCII 1 文字の 16 進ニブル（0–15）。非 16 進は `None`。
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// 上位・下位ニブルの 2 バイトから 16 進バイト（0–255）を組む。非 16 進は `None`。
///
/// バイト単位で扱うため非 ASCII 入力でもパニックしない（`&str` のバイト境界スライスを避ける）。
fn hex_pair(hi: u8, lo: u8) -> Option<u8> {
    Some(hex_nibble(hi)? * 16 + hex_nibble(lo)?)
}

/// `rgb(...)` / `rgba(...)` の括弧内引数を解釈する。
/// `with_alpha` が真なら 4 要素ちょうど（末尾は f32 alpha）、偽なら 3 要素ちょうど。
fn parse_rgb_args(inner: &str, with_alpha: bool) -> Option<Color> {
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let expected = if with_alpha { 4 } else { 3 };
    if parts.len() != expected {
        return None;
    }
    // u8 パースは 0–255 範囲外（例: 256）を自動的に拒否する。
    let r = parts[0].parse::<u8>().ok()?;
    let g = parts[1].parse::<u8>().ok()?;
    let b = parts[2].parse::<u8>().ok()?;
    let a = if with_alpha {
        let a = parts[3].parse::<f32>().ok()?;
        if !(0.0..=1.0).contains(&a) {
            return None;
        }
        a
    } else {
        1.0
    };
    Some(Color { r, g, b, a })
}

/// `hsl(...)` / `hsla(...)` の括弧内引数を解釈する。
/// `with_alpha` が真なら 4 要素ちょうど（末尾は f32 alpha）、偽なら 3 要素ちょうど。
///
/// `h` は度（任意の有限値、`rem_euclid(360.0)` で正規化）、`s`/`l` は末尾 `%` 必須で
/// 0–100。範囲外や `%` 欠落は None。
fn parse_hsl_args(inner: &str, with_alpha: bool) -> Option<Color> {
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let expected = if with_alpha { 4 } else { 3 };
    if parts.len() != expected {
        return None;
    }
    let h = parts[0].parse::<f64>().ok()?;
    if !h.is_finite() {
        return None;
    }
    let h = h.rem_euclid(360.0);
    let s = parse_percent(parts[1])?;
    let l = parse_percent(parts[2])?;
    let a = if with_alpha {
        let a = parts[3].parse::<f32>().ok()?;
        if !(0.0..=1.0).contains(&a) {
            return None;
        }
        a
    } else {
        1.0
    };
    let (r, g, b) = hsl_to_rgb(h, s, l);
    Some(Color { r, g, b, a })
}

/// 末尾 `%` 必須のパーセント値を 0.0–1.0 へ変換する。
/// `%` 欠落、非数値、0–100 範囲外はいずれも `None`。
fn parse_percent(s: &str) -> Option<f64> {
    let v = s.strip_suffix('%')?.parse::<f64>().ok()?;
    if !(0.0..=100.0).contains(&v) {
        return None;
    }
    Some(v / 100.0)
}

/// HSL（h: 度 [0,360)、s/l: 0.0–1.0）を RGB へ。標準アルゴリズム + 四捨五入。
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;
    (r, g, b)
}

/// 不透明な定数 [`Color`] を組む小ヘルパ（色名テーブルを読みやすく保つ）。
const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// CSS 拡張色名（CSS Color Module Level 4 の 148 色）と `transparent` を解釈する。
/// 入力は呼び出し側で小文字化済みであること。未知名は `None`。
fn named_color(name: &str) -> Option<Color> {
    let c = match name {
        "transparent" => Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        },
        "aliceblue" => rgb(240, 248, 255),
        "antiquewhite" => rgb(250, 235, 215),
        "aqua" => rgb(0, 255, 255),
        "aquamarine" => rgb(127, 255, 212),
        "azure" => rgb(240, 255, 255),
        "beige" => rgb(245, 245, 220),
        "bisque" => rgb(255, 228, 196),
        "black" => rgb(0, 0, 0),
        "blanchedalmond" => rgb(255, 235, 205),
        "blue" => rgb(0, 0, 255),
        "blueviolet" => rgb(138, 43, 226),
        "brown" => rgb(165, 42, 42),
        "burlywood" => rgb(222, 184, 135),
        "cadetblue" => rgb(95, 158, 160),
        "chartreuse" => rgb(127, 255, 0),
        "chocolate" => rgb(210, 105, 30),
        "coral" => rgb(255, 127, 80),
        "cornflowerblue" => rgb(100, 149, 237),
        "cornsilk" => rgb(255, 248, 220),
        "crimson" => rgb(220, 20, 60),
        "cyan" => rgb(0, 255, 255),
        "darkblue" => rgb(0, 0, 139),
        "darkcyan" => rgb(0, 139, 139),
        "darkgoldenrod" => rgb(184, 134, 11),
        "darkgray" => rgb(169, 169, 169),
        "darkgreen" => rgb(0, 100, 0),
        "darkgrey" => rgb(169, 169, 169),
        "darkkhaki" => rgb(189, 183, 107),
        "darkmagenta" => rgb(139, 0, 139),
        "darkolivegreen" => rgb(85, 107, 47),
        "darkorange" => rgb(255, 140, 0),
        "darkorchid" => rgb(153, 50, 204),
        "darkred" => rgb(139, 0, 0),
        "darksalmon" => rgb(233, 150, 122),
        "darkseagreen" => rgb(143, 188, 143),
        "darkslateblue" => rgb(72, 61, 139),
        "darkslategray" => rgb(47, 79, 79),
        "darkslategrey" => rgb(47, 79, 79),
        "darkturquoise" => rgb(0, 206, 209),
        "darkviolet" => rgb(148, 0, 211),
        "deeppink" => rgb(255, 20, 147),
        "deepskyblue" => rgb(0, 191, 255),
        "dimgray" => rgb(105, 105, 105),
        "dimgrey" => rgb(105, 105, 105),
        "dodgerblue" => rgb(30, 144, 255),
        "firebrick" => rgb(178, 34, 34),
        "floralwhite" => rgb(255, 250, 240),
        "forestgreen" => rgb(34, 139, 34),
        "fuchsia" => rgb(255, 0, 255),
        "gainsboro" => rgb(220, 220, 220),
        "ghostwhite" => rgb(248, 248, 255),
        "gold" => rgb(255, 215, 0),
        "goldenrod" => rgb(218, 165, 32),
        "gray" => rgb(128, 128, 128),
        "green" => rgb(0, 128, 0),
        "greenyellow" => rgb(173, 255, 47),
        "grey" => rgb(128, 128, 128),
        "honeydew" => rgb(240, 255, 240),
        "hotpink" => rgb(255, 105, 180),
        "indianred" => rgb(205, 92, 92),
        "indigo" => rgb(75, 0, 130),
        "ivory" => rgb(255, 255, 240),
        "khaki" => rgb(240, 230, 140),
        "lavender" => rgb(230, 230, 250),
        "lavenderblush" => rgb(255, 240, 245),
        "lawngreen" => rgb(124, 252, 0),
        "lemonchiffon" => rgb(255, 250, 205),
        "lightblue" => rgb(173, 216, 230),
        "lightcoral" => rgb(240, 128, 128),
        "lightcyan" => rgb(224, 255, 255),
        "lightgoldenrodyellow" => rgb(250, 250, 210),
        "lightgray" => rgb(211, 211, 211),
        "lightgreen" => rgb(144, 238, 144),
        "lightgrey" => rgb(211, 211, 211),
        "lightpink" => rgb(255, 182, 193),
        "lightsalmon" => rgb(255, 160, 122),
        "lightseagreen" => rgb(32, 178, 170),
        "lightskyblue" => rgb(135, 206, 250),
        "lightslategray" => rgb(119, 136, 153),
        "lightslategrey" => rgb(119, 136, 153),
        "lightsteelblue" => rgb(176, 196, 222),
        "lightyellow" => rgb(255, 255, 224),
        "lime" => rgb(0, 255, 0),
        "limegreen" => rgb(50, 205, 50),
        "linen" => rgb(250, 240, 230),
        "magenta" => rgb(255, 0, 255),
        "maroon" => rgb(128, 0, 0),
        "mediumaquamarine" => rgb(102, 205, 170),
        "mediumblue" => rgb(0, 0, 205),
        "mediumorchid" => rgb(186, 85, 211),
        "mediumpurple" => rgb(147, 112, 219),
        "mediumseagreen" => rgb(60, 179, 113),
        "mediumslateblue" => rgb(123, 104, 238),
        "mediumspringgreen" => rgb(0, 250, 154),
        "mediumturquoise" => rgb(72, 209, 204),
        "mediumvioletred" => rgb(199, 21, 133),
        "midnightblue" => rgb(25, 25, 112),
        "mintcream" => rgb(245, 255, 250),
        "mistyrose" => rgb(255, 228, 225),
        "moccasin" => rgb(255, 228, 181),
        "navajowhite" => rgb(255, 222, 173),
        "navy" => rgb(0, 0, 128),
        "oldlace" => rgb(253, 245, 230),
        "olive" => rgb(128, 128, 0),
        "olivedrab" => rgb(107, 142, 35),
        "orange" => rgb(255, 165, 0),
        "orangered" => rgb(255, 69, 0),
        "orchid" => rgb(218, 112, 214),
        "palegoldenrod" => rgb(238, 232, 170),
        "palegreen" => rgb(152, 251, 152),
        "paleturquoise" => rgb(175, 238, 238),
        "palevioletred" => rgb(219, 112, 147),
        "papayawhip" => rgb(255, 239, 213),
        "peachpuff" => rgb(255, 218, 185),
        "peru" => rgb(205, 133, 63),
        "pink" => rgb(255, 192, 203),
        "plum" => rgb(221, 160, 221),
        "powderblue" => rgb(176, 224, 230),
        "purple" => rgb(128, 0, 128),
        "rebeccapurple" => rgb(102, 51, 153),
        "red" => rgb(255, 0, 0),
        "rosybrown" => rgb(188, 143, 143),
        "royalblue" => rgb(65, 105, 225),
        "saddlebrown" => rgb(139, 69, 19),
        "salmon" => rgb(250, 128, 114),
        "sandybrown" => rgb(244, 164, 96),
        "seagreen" => rgb(46, 139, 87),
        "seashell" => rgb(255, 245, 238),
        "sienna" => rgb(160, 82, 45),
        "silver" => rgb(192, 192, 192),
        "skyblue" => rgb(135, 206, 235),
        "slateblue" => rgb(106, 90, 205),
        "slategray" => rgb(112, 128, 144),
        "slategrey" => rgb(112, 128, 144),
        "snow" => rgb(255, 250, 250),
        "springgreen" => rgb(0, 255, 127),
        "steelblue" => rgb(70, 130, 180),
        "tan" => rgb(210, 180, 140),
        "teal" => rgb(0, 128, 128),
        "thistle" => rgb(216, 191, 216),
        "tomato" => rgb(255, 99, 71),
        "turquoise" => rgb(64, 224, 208),
        "violet" => rgb(238, 130, 238),
        "wheat" => rgb(245, 222, 179),
        "white" => rgb(255, 255, 255),
        "whitesmoke" => rgb(245, 245, 245),
        "yellow" => rgb(255, 255, 0),
        "yellowgreen" => rgb(154, 205, 50),
        _ => return None,
    };
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex6() {
        let c = parse_color("#36A2EB").unwrap();
        assert_eq!((c.r, c.g, c.b), (54, 162, 235));
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn parses_hex3() {
        let c = parse_color("#abc").unwrap();
        assert_eq!((c.r, c.g, c.b), (0xaa, 0xbb, 0xcc));
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn parses_rgb() {
        let c = parse_color("rgb(255, 99, 132)").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 99, 132));
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn parses_rgba() {
        let c = parse_color("rgba(255, 99, 132, 0.5)").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 99, 132));
        assert!((c.a - 0.5).abs() < 1e-6);
    }

    #[test]
    fn handles_whitespace_and_case() {
        // 前後空白・大文字小文字・内部空白の揺れを許容
        assert_eq!(
            parse_color("  #FFF ").unwrap(),
            parse_color("#ffffff").unwrap()
        );
        let c = parse_color("RGB(1,2,3)").unwrap();
        assert_eq!((c.r, c.g, c.b), (1, 2, 3));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_color("not-a-color").is_none());
        assert!(parse_color("#12").is_none()); // 不正な桁数
        assert!(parse_color("#GGGGGG").is_none()); // 非16進
        assert!(parse_color("rgb(1,2)").is_none()); // 引数不足
        assert!(parse_color("").is_none());
        assert!(parse_color("#+abcde").is_none()); // 符号は16進ではない
        assert!(parse_color("rgb(1,2,3)x").is_none()); // 末尾ゴミ
        assert!(parse_color("rgb(1,2,3").is_none()); // 閉じ括弧欠落
    }

    #[test]
    fn does_not_panic_on_non_ascii() {
        // マルチバイト入力はバイト長が 6/8 でも char 境界外スライスでパニックしてはならない。
        assert!(parse_color("#一二").is_none()); // 6 バイト
        assert!(parse_color("#一二三").is_none()); // 9 バイト
        assert!(parse_color("rgb(あ,い,う)").is_none());
    }

    #[test]
    fn clamps_or_rejects_out_of_range() {
        // 仕様判断: rgb の 0-255 範囲外をどう扱うか。
        // 実装方針として「範囲外はクランプせず None」で実装すること。
        assert!(parse_color("rgb(256, 0, 0)").is_none());
    }

    #[test]
    fn parses_named_colors() {
        assert_eq!(
            parse_color("red").unwrap(),
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 1.0
            }
        );
        assert_eq!(
            parse_color("blue").unwrap(),
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 1.0
            }
        );
        assert_eq!(
            parse_color("rebeccapurple").unwrap(),
            Color {
                r: 102,
                g: 51,
                b: 153,
                a: 1.0
            }
        );
        assert_eq!(
            parse_color("ReD").unwrap(),
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 1.0
            }
        ); // 大小無視
        let t = parse_color("transparent").unwrap();
        assert_eq!(t.a, 0.0);
    }

    #[test]
    fn parses_hsl() {
        assert_eq!(
            parse_color("hsl(0, 100%, 50%)").unwrap(),
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 1.0
            }
        );
        assert_eq!(
            parse_color("hsl(120, 100%, 50%)").unwrap(),
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 1.0
            }
        );
        assert_eq!(
            parse_color("hsl(240, 100%, 50%)").unwrap(),
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 1.0
            }
        );
        // h 正規化: 360 == 0
        assert_eq!(
            parse_color("hsl(360, 100%, 50%)").unwrap(),
            parse_color("hsl(0,100%,50%)").unwrap()
        );
        let c = parse_color("hsla(0, 100%, 50%, 0.5)").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
        assert!((c.a - 0.5).abs() < 1e-6);
    }

    #[test]
    fn rejects_bad_hsl_and_names() {
        assert!(parse_color("hsl(0, 100, 50)").is_none()); // % 欠落
        assert!(parse_color("hsl(0, 150%, 50%)").is_none()); // s 範囲外
        assert!(parse_color("hsla(0, 100%, 50%, 2)").is_none()); // a 範囲外
        assert!(parse_color("definitelynotacolor").is_none()); // 未知名
    }

    #[test]
    fn pins_tricky_named_color_values() {
        // 基本16色の罠（green≠lime, gray, purple 等）と地味な値を W3C 標準値で固定。
        let cases = [
            ("green", (0u8, 128u8, 0u8)),
            ("lime", (0, 255, 0)),
            ("gray", (128, 128, 128)),
            ("grey", (128, 128, 128)),
            ("silver", (192, 192, 192)),
            ("purple", (128, 0, 128)),
            ("teal", (0, 128, 128)),
            ("aqua", (0, 255, 255)),
            ("cyan", (0, 255, 255)),
            ("fuchsia", (255, 0, 255)),
            ("magenta", (255, 0, 255)),
            ("mediumvioletred", (199, 21, 133)),
            ("papayawhip", (255, 239, 213)),
            ("lightgoldenrodyellow", (250, 250, 210)),
            ("mediumslateblue", (123, 104, 238)),
            ("slategray", (112, 128, 144)),
        ];
        for (name, (r, g, b)) in cases {
            let c = parse_color(name).unwrap();
            assert_eq!((c.r, c.g, c.b), (r, g, b), "色名 {name} の値が不一致");
            assert_eq!(c.a, 1.0);
        }
    }
}
