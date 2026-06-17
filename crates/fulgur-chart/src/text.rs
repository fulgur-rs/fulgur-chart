//! 同梱フォントの advance width に基づく決定的な文字列幅計測。

use ttf_parser::Face;

pub struct TextMeasurer<'a> {
    face: Face<'a>,
    units_per_em: f32,
}

impl<'a> TextMeasurer<'a> {
    pub fn new(font_bytes: &'a [u8]) -> Result<Self, String> {
        let face = Face::parse(font_bytes, 0).map_err(|e| e.to_string())?;
        let upem = face.units_per_em() as f32;
        Ok(Self {
            face,
            units_per_em: upem,
        })
    }

    /// `text` を `size_px` で描いたときの advance 幅合計（px）。
    /// 字形シェーピング・カーニングは行わない（v1）。未収録文字は 0 幅扱い。
    pub fn width(&self, text: &str, size_px: f32) -> f32 {
        let scale = size_px / self.units_per_em;
        let mut total = 0.0_f32;
        for ch in text.chars() {
            if let Some(gid) = self.face.glyph_index(ch) {
                if let Some(adv) = self.face.glyph_hor_advance(gid) {
                    total += adv as f32 * scale;
                }
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;

    #[test]
    fn empty_string_is_zero_width() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        assert_eq!(m.width("", 12.0), 0.0);
    }

    #[test]
    fn wider_text_is_wider() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let a = m.width("W", 12.0);
        let b = m.width("WWW", 12.0);
        assert!(b > a && b > 0.0);
    }

    #[test]
    fn scales_with_font_size() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let small = m.width("売上", 10.0);
        let large = m.width("売上", 20.0);
        assert!((large / small - 2.0).abs() < 1e-6);
    }
}
