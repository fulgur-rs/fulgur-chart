//! GradientPath のラスタ描画 byte 安定テスト + 実画素検証。
use fulgur_chart::ir::Color;
use fulgur_chart::raster_direct::scene_to_png;
use fulgur_chart::scene::{Prim, Scene};

const FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansJP-Regular.otf");

fn scene() -> Scene {
    Scene {
        width: 40.0,
        height: 20.0,
        // d は parse_path_data(split_ascii_whitespace)が要求する空白区切り形式。
        items: vec![Prim::GradientPath {
            d: "M 0 0 L 40 0 L 40 20 L 0 20 Z".into(),
            x0: 0.0,
            x1: 40.0,
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
    }
}

#[test]
fn gradient_png_is_byte_deterministic() {
    let a = scene_to_png(&scene(), 1.0, FONT).unwrap();
    let b = scene_to_png(&scene(), 1.0, FONT).unwrap();
    assert_eq!(a, b);
    assert!(!a.is_empty());
}

#[test]
fn gradient_png_renders_left_red_right_green() {
    // tiny-skia の LinearGradient が実際に描画されていることを画素で確認する。
    // (空白区切りでない d だと parse_path_data が None を返して早期 return するため、
    //  この検証が無いとグラデーション arm を丸ごと消してもテストが通ってしまう。)
    let png = scene_to_png(&scene(), 1.0, FONT).unwrap();
    let pm = tiny_skia::Pixmap::decode_png(&png).expect("生成 PNG はデコード可能");

    // 左端(x0=stop0=赤寄り)と右端(x1=stop1=緑寄り)の内側画素。
    let left = pm.pixel(2, 10).expect("左端画素は範囲内");
    let right = pm.pixel(37, 10).expect("右端画素は範囲内");

    // グラデーションが実際に塗られている(透明でない)こと。
    assert!(
        left.alpha() > 0 && right.alpha() > 0,
        "グラデーションは非透明な画素を塗るはず"
    );
    // alpha は両 stop とも 0.5 で一定なので、premultiplied 値の大小関係は
    // そのまま色の優勢を表す。左は赤優勢(stop0)、右は緑優勢(stop1)。
    assert!(
        left.red() > left.green() && left.red() > left.blue(),
        "左端は stop0(赤)優勢のはず: {:?}",
        (left.red(), left.green(), left.blue())
    );
    assert!(
        right.green() > right.red() && right.green() > right.blue(),
        "右端は stop1(緑)優勢のはず: {:?}",
        (right.red(), right.green(), right.blue())
    );
}
