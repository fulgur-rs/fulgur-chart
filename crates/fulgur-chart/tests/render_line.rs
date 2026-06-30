use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::layout::line;
use fulgur_chart::raster_direct::render_chart_to_png;
use fulgur_chart::render::render_chart;
use fulgur_chart::scene::Prim;
use fulgur_chart::text::TextMeasurer;
fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

/// PNG バイト列（scale 1.0）。決定性・SVG↔PNG 一致テスト用。
fn render_png(json: &str) -> Vec<u8> {
    render_chart_to_png(&chartjs::parse(json, false).unwrap(), 1.0, DEFAULT_FONT).unwrap()
}

#[test]
fn line_has_polyline_and_markers() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B","C"],"datasets":[{"label":"s","data":[1,3,2]}]}}"#,
    );
    assert!(svg.contains("<polyline"));
    assert!(svg.matches("<circle").count() >= 3); // 各点にマーカー
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn area_emits_filled_path_with_opacity() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[1,2],"fill":true}]}}"#,
    );
    assert!(svg.contains("<path"));
    assert!(svg.contains("fill-opacity=")); // 半透明 area
    assert!(svg.contains("Z\"")); // 閉じたパス
}

#[test]
fn tension_uses_bezier_path() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B","C"],"datasets":[{"data":[1,3,2],"tension":0.4}]}}"#,
    );
    assert!(svg.contains("<path")); // 曲線はpath
    assert!(svg.contains(" C ")); // ベジエコマンド
}

#[test]
fn line_deterministic() {
    let j = r#"{"type":"line","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn line_snapshot() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["1月","2月","3月"],"datasets":[{"label":"売上","data":[120,200,150]}]},"options":{"plugins":{"title":{"display":true,"text":"推移"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn area_snapshot() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["Q1","Q2","Q3"],"datasets":[{"label":"累計","data":[30,75,130],"fill":true}]}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn tension_snapshot() {
    let svg = render(
        r#"{"type":"line","data":{"labels":["A","B","C","D"],"datasets":[{"label":"曲線","data":[1,3,2,4],"tension":0.4}]}}"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn offset_snapshot() {
    // chart.js options.scales.x.offset:true: 点・ラベルを band 中心へ寄せ、bar と同じ
    // chartArea(端余白なし)で描く。既定の edge-to-edge とは別の出力。
    let svg = render(
        r#"{"type":"line","data":{"labels":["1月","2月","3月"],"datasets":[{"label":"売上","data":[120,200,150]}]},"options":{"scales":{"x":{"offset":true}}}}"#,
    );
    insta::assert_snapshot!(svg);
}

// --- デシメーション配線（Task 7/8）---

/// scene 内の全 Polyline の点数合計。間引きで線が消えていないこと/間引かれたことの検証に使う。
fn polyline_pts(json: &str) -> usize {
    let spec = chartjs::parse(json, false).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &m);
    scene
        .items
        .iter()
        .filter_map(|p| match p {
            Prim::Polyline { points, .. } => Some(points.len()),
            _ => None,
        })
        .sum()
}

#[test]
fn large_line_is_decimated_vs_disabled() {
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    let body = format!(
        "\"labels\":[{}],\"datasets\":[{{\"data\":[{}]}}]",
        labels.join(","),
        data.join(",")
    );
    let on = format!(r#"{{"type":"line","data":{{{body}}}}}"#);
    let off = format!(
        r#"{{"type":"line","data":{{{body}}},"options":{{"plugins":{{"decimation":{{"enabled":false}}}}}}}}"#
    );
    let on_pts = polyline_pts(&on);
    let off_pts = polyline_pts(&off);
    assert_eq!(off_pts, n, "disabled must keep all points (single segment)");
    assert!(
        on_pts > 0 && on_pts < off_pts,
        "default must decimate: {on_pts} vs {off_pts}"
    );
}

#[test]
fn small_line_polyline_unchanged() {
    // 3点 line → 間引きされず 3点のまま（既存 golden と整合）。
    let pts = polyline_pts(
        r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#,
    );
    assert_eq!(pts, 3);
}

/// scene 内の Circle（マーカー）数。
fn circle_count(json: &str) -> usize {
    let spec = chartjs::parse(json, false).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(&spec, &m);
    scene
        .items
        .iter()
        .filter(|p| matches!(p, Prim::Circle { .. }))
        .count()
}

#[test]
fn large_line_suppresses_markers_by_default() {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", i % 50)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    assert_eq!(
        circle_count(&json),
        0,
        "large line should suppress markers by default"
    );
}

#[test]
fn large_line_keeps_markers_when_pointradius_set() {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", i % 50)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}],"pointRadius":2}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    assert!(
        circle_count(&json) > 0,
        "explicit pointRadius should keep markers"
    );
}

#[test]
fn small_line_markers_unchanged() {
    // 3点 → markers drawn as before (radius 3, not suppressed)。
    assert_eq!(
        circle_count(
            r#"{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]}}"#
        ),
        3
    );
}

/// scene 内の各 Polyline の点列を順に返す（セグメント数・各セグメント点数・座標有限性の検証用）。
fn polylines(spec: &fulgur_chart::ir::ChartSpec) -> Vec<Vec<(f64, f64)>> {
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = line::build(spec, &m);
    scene
        .items
        .iter()
        .filter_map(|p| match p {
            Prim::Polyline { points, .. } => Some(points.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn gapped_large_line_keeps_segments_and_decimates() {
    // segment-first 設計の回帰テスト: gap のある巨大系列が、間引き後も
    //   (a) セグメント融合せず ≥2 本の Polyline を保ち、
    //   (b) 間引きが効いて総点数が大幅に減り、
    //   (c) どのセグメントも崩壊・消失しない
    // ことを保証する。素朴な「間引き後に cat で再分割」実装ではここで全点が gap 扱いになり
    // 各点が長さ1セグメントへ割れて Polyline が 0 本になる（(a) が FAIL する）。
    //
    // JSON の data は untagged `Nums(Vec<f64>)` で非有限値を表現できないため（null も 1e400 も
    // parse エラー）、parse 後に中央へ NaN を注入して gap を作る。これは line の gap 分割が
    // 認識する表現そのもの（`valid` フィルタが非有限値を落とし cat 不連続を生む）。
    let n = 8000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 37) % 101)).collect();
    let json = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    );
    let mut spec = chartjs::parse(&json, false).unwrap();
    spec.series[0].values[n / 2] = f64::NAN; // 中央に gap を作る

    let polys = polylines(&spec);
    let counts: Vec<usize> = polys.iter().map(|p| p.len()).collect();

    // (a) gap が保たれ 2 セグメント以上（gap をまたいで融合していない／線が消えていない）。
    assert!(
        counts.len() >= 2,
        "gap must yield >=2 polylines, got {}: {counts:?}",
        counts.len()
    );
    // (c) どのセグメントも 2 点以上（崩壊・消失していない）。
    assert!(
        counts.iter().all(|&c| c >= 2),
        "every polyline must have >=2 points: {counts:?}"
    );
    // (b) 間引きが効いて総点数が大幅に減る（gap で 1 点落ちた n-1 ではなく、明確に半分未満）。
    let total: usize = counts.iter().sum();
    assert!(
        total < n / 2,
        "decimation must substantially reduce total points: {total} vs n={n}"
    );
    // 念のため: y スケールが NaN 汚染されておらず、出力座標がすべて有限。
    assert!(
        polys
            .iter()
            .flatten()
            .all(|&(x, y)| x.is_finite() && y.is_finite()),
        "all emitted polyline points must be finite"
    );
}

// --- 決定性・no-op サニティ・SVG↔PNG 一致（Task 9）---

#[test]
fn disabled_decimation_keeps_all_points_sanity() {
    // サニティ: enabled:false の巨大 line は単一セグメント全点を保持し、間引きされない。
    // （pre-feature バイト不変の真の保証は threshold 未満で緑のままの既存小 golden。
    //   これは passthrough = 非間引きと同形であることの確認のみ。）
    let n = 3000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 13) % 50)).collect();
    let off = format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}},"options":{{"plugins":{{"decimation":{{"enabled":false}}}}}}}}"#,
        labels.join(","),
        data.join(",")
    );
    assert_eq!(polyline_pts(&off), n);
}

/// 5000 点（threshold 超過確実）の自動間引き line spec を組み立てる。
fn big_decimated_line_json() -> String {
    let n = 5000;
    let labels: Vec<String> = (0..n).map(|i| format!("\"{i}\"")).collect();
    let data: Vec<String> = (0..n).map(|i| format!("{}", (i * 29) % 83)).collect();
    format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"data":[{}]}}]}}}}"#,
        labels.join(","),
        data.join(",")
    )
}

#[test]
fn decimated_line_is_deterministic() {
    // 同一入力 → 同一バイト列（SVG・PNG 双方）。間引き経路の決定性を担保。
    let json = big_decimated_line_json();
    assert_eq!(render(&json), render(&json), "SVG must be byte-identical");
    assert_eq!(
        render_png(&json),
        render_png(&json),
        "PNG must be byte-identical"
    );
}

#[test]
fn decimated_line_renders_svg_and_png_consistently() {
    // SVG/PNG は build() の同一 Scene を消費するため、間引き-on の line が
    // 両出力でエラー無く・決定的にレンダされることを確認（geometry 共有の担保）。
    let json = big_decimated_line_json();
    let svg = render(&json);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.contains("<polyline"));

    let png = render_png(&json);
    assert!(png.len() > 8, "PNG must be non-empty");
    // 妥当な PNG であること（デコードできる）を tiny-skia で確認。
    let pix = tiny_skia::Pixmap::decode_png(&png).expect("decimated PNG must decode");
    assert!(pix.width() > 0 && pix.height() > 0);
}
