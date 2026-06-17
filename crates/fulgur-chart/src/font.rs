//! 同梱フォントの提供。計測と描画で同一バイト列を使い、三者一致を保証する。

/// バイナリに埋め込んだ既定フォント（Noto Sans JP Regular, static OTF/CFF）。
pub static DEFAULT_FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansJP-Regular.otf");

/// 既定フォントのファミリ名(font-family の主名)。
pub const DEFAULT_FAMILY: &str = "Noto Sans JP";

/// フォントバイト列からファミリ名(name table の name_id 1)を取り出す。
///
/// パース不能・ファミリ名が無い場合は `None`。Unicode/英語のレコードを優先し、
/// 無ければデコード可能な任意のファミリ名にフォールバックする。
/// `names()` の走査順はフォントファイルで固定のため、結果は決定的。
pub fn family_name(bytes: &[u8]) -> Option<String> {
    use ttf_parser::{Face, name_id};

    let face = Face::parse(bytes, 0).ok()?;

    // name_id == 1(FAMILY) のうち、英語(language_id 0x0409)を最優先、
    // 次に任意の Unicode デコード可能レコードへフォールバックする。
    let mut fallback: Option<String> = None;
    for name in face.names() {
        if name.name_id != name_id::FAMILY {
            continue;
        }
        if let Some(s) = name.to_string() {
            // Windows English (US) を最優先で即返し。
            if name.language_id == 0x0409 {
                return Some(s);
            }
            if fallback.is_none() {
                fallback = Some(s);
            }
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_font_parses() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        assert!(face.number_of_glyphs() > 0);
    }

    #[test]
    fn family_name_of_default_font() {
        let fam = family_name(DEFAULT_FONT).expect("family name");
        assert_eq!(fam, DEFAULT_FAMILY);
    }

    #[test]
    fn family_name_of_garbage_is_none() {
        assert!(family_name(b"not a font").is_none());
    }

    #[test]
    fn default_font_covers_ascii_kana_and_kanji() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        assert!(face.glyph_index('A').is_some());
        assert!(face.glyph_index('あ').is_some());
        assert!(face.glyph_index('売').is_some());
    }
}
