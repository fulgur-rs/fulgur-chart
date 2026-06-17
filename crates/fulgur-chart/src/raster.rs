//! SVG → PNG。resvg/usvg/tiny-skia で描画し、fontdb に同梱フォントをロードして
//! 計測・描画・ラスタの三者でフォントを一致させる。

use resvg::{tiny_skia, usvg};

use crate::font::DEFAULT_FONT;

/// SVG 文字列を PNG バイト列にラスタライズする。`scale` は解像度倍率（1.0=等倍）。
///
/// `scale` が正の有限値でない場合（0 以下・負・NaN）は 1.0 にフォールバックする。
/// 失敗は `Err(String)` に変換し、panic しない。
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    // 既定パス: 同梱フォントを fontdb に載せる。出力は従来と完全一致。
    svg_to_png_with_font(svg, scale, DEFAULT_FONT)
}

/// SVG 文字列を PNG バイト列にラスタライズする。`font_bytes` を fontdb にロードする。
///
/// `svg_to_png` との違いはロードするフォントのみ。`scale` ガード・寸法計算・
/// レンダリングは同一で、決定的・非 panic。
pub fn svg_to_png_with_font(svg: &str, scale: f32, font_bytes: &[u8]) -> Result<Vec<u8>, String> {
    // フォント検証: 計測(TextMeasurer)・SVG family・ラスタの三者で「有効フォント」の
    // 判定を一致させるため、計測層と同一の ttf_parser::Face::parse でゲートする。
    // (usvg fontdb は不正バイトを黙って無視し Err を返さないため、ここで弾く。)
    ttf_parser::Face::parse(font_bytes, 0).map_err(|e| format!("フォント解析失敗: {e}"))?;

    // scale ガード: `> 0.0` 形で 0/負/NaN をまとめて 1.0 に倒す。
    // 以後この `scale` を pixmap 寸法と Transform の両方で同一に使う。
    let scale = if scale > 0.0 { scale } else { 1.0 };

    // usvg::Options に指定フォントをロード（決定性のため1本のみ。system fonts は読まない）。
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_font_data(font_bytes.to_vec());

    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("SVG 解析失敗: {e}"))?;

    let size = tree.size();
    let w = (size.width() * scale).round().max(1.0) as u32;
    let h = (size.height() * scale).round().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| format!("Pixmap 確保失敗: 寸法 {w}x{h} が無効です"))?;

    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| format!("PNG エンコード失敗: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"10\"><rect x=\"0\" y=\"0\" width=\"20\" height=\"10\" fill=\"#36a2eb\"/></svg>";

    #[test]
    fn rasterizes_to_valid_png() {
        let png = svg_to_png(MIN_SVG, 1.0).unwrap();
        // PNG シグネチャ \x89PNG
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
        assert!(png.len() > 50);
    }

    #[test]
    fn scale_increases_size() {
        let small = svg_to_png(MIN_SVG, 1.0).unwrap();
        let large = svg_to_png(MIN_SVG, 3.0).unwrap();
        assert!(large.len() > small.len(), "scaled PNG should be larger");
    }

    #[test]
    fn non_positive_scale_falls_back() {
        // scale<=0 でも panic せず有効PNG（1.0扱い）
        let png = svg_to_png(MIN_SVG, 0.0).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn png_is_byte_deterministic() {
        // 同一 SVG を 2 回ラスタライズしてバイト一致（決定的出力の回帰テスト）。
        let a = svg_to_png(MIN_SVG, 1.0).unwrap();
        let b = svg_to_png(MIN_SVG, 1.0).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn renders_text_with_bundled_font() {
        // 日本語テキストを含む SVG がフォント解決でき、ラスタが空でないこと
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"30\"><text x=\"0\" y=\"20\" font-family=\"Noto Sans JP, sans-serif\" font-size=\"16\" fill=\"#000000\">売上</text></svg>";
        let png = svg_to_png(svg, 1.0).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn with_font_rasterizes_to_valid_png() {
        use crate::font::DEFAULT_FONT;
        let png = svg_to_png_with_font(MIN_SVG, 1.0, DEFAULT_FONT).unwrap();
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn with_invalid_font_is_err() {
        // フォントが usvg fontdb にロードできないバイト列のとき Err（panic しない）。
        let err = svg_to_png_with_font(MIN_SVG, 1.0, b"not a font");
        assert!(err.is_err());
    }
}
