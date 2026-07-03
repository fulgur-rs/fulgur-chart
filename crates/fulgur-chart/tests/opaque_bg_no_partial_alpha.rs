//! 不変条件: 不透明背景のチャートは部分α画素(0<a<255)を持たない。
//! これは opaque skip 最適化(a7c)の前提であり、崩れると PNG/WebP が byte 破壊する。
use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png;
use tiny_skia::Pixmap;

fn partial_alpha_count(png: &[u8]) -> usize {
    let pm = Pixmap::decode_png(png).unwrap();
    pm.pixels()
        .iter()
        .filter(|p| {
            let a = p.alpha();
            a != 0 && a != 255
        })
        .count()
}

#[test]
fn opaque_background_produces_no_partial_alpha() {
    let cases = [
        r##"{"type":"bar","data":{"labels":["a","b","c"],"datasets":[{"data":[3,1,2]}]},
          "options":{"theme":{"backgroundColor":"#ff00ff"}}}"##,
        r##"{"type":"line","data":{"labels":["a","b","c","d"],"datasets":[{"data":[1,3,2,4]}]},
          "options":{"theme":{"backgroundColor":"#00aa88"}}}"##,
        r##"{"type":"pie","data":{"labels":["a","b","c"],"datasets":[{"data":[3,1,2]}]},
          "options":{"theme":{"backgroundColor":"#123456"}}}"##,
    ];
    // 整数・分数・丸め上げ(800*1.000625=800.5)を含む代表 scale。
    let scales = [1.0f32, 2.0, 1.5, 1.000625];
    for json in &cases {
        let spec = chartjs::parse(json, false).unwrap();
        for &scale in &scales {
            let png = render_chart_to_png(&spec, scale, fulgur_chart::font::DEFAULT_FONT).unwrap();
            assert_eq!(
                partial_alpha_count(&png),
                0,
                "不透明背景で部分α画素が出た (scale={scale}, json={json})"
            );
        }
    }
}
