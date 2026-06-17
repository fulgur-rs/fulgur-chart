//! 同梱フォントの提供。計測と描画で同一バイト列を使い、三者一致を保証する。

/// バイナリに埋め込んだ既定フォント（Noto Sans JP Regular, static OTF/CFF）。
pub static DEFAULT_FONT: &[u8] =
    include_bytes!("../assets/fonts/NotoSansJP-Regular.otf");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_font_parses() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        assert!(face.number_of_glyphs() > 0);
    }

    #[test]
    fn default_font_covers_ascii_kana_and_kanji() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        assert!(face.glyph_index('A').is_some());
        assert!(face.glyph_index('あ').is_some());
        assert!(face.glyph_index('売').is_some());
    }
}
