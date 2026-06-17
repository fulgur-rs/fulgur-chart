//! 色文字列パーサ。chart.js spec の色文字列を [`crate::ir::Color`] へ変換する。
//!
//! 対応形式: `#RGB` / `#RRGGBB` / `#RRGGBBAA` / `rgb(r,g,b)` / `rgba(r,g,b,a)`。
//! CSS 色名・HSL 等は v1 では非対応（None を返す）。範囲外の値（例: `rgb(256,0,0)`）は
//! クランプせず None。

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
    } else {
        None
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
}
