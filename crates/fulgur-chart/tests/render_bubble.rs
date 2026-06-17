use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

/// SVG 中の全 `<circle ... r="..."/>` の半径文字列を出現順に集める。
fn circle_radii(svg: &str) -> Vec<String> {
    svg.match_indices("<circle")
        .map(|(start, _)| {
            let rest = &svg[start..];
            let r_at = rest.find(" r=\"").expect("circle に r 属性が無い") + 4;
            let after = &rest[r_at..];
            let end = after.find('"').expect("r 属性が閉じていない");
            after[..end].to_string()
        })
        .collect()
}

#[test]
fn bubble_radii_reflect_point_r() {
    // r:5 と r:20 の 2 点。半径はデータ駆動で異なり、r が大きい点ほど円が大きい。
    let svg = render(
        r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"r":5},{"x":3,"y":4,"r":20}]}]}}"#,
    );
    // フレームは円を描かない(凡例 swatch は Rect)。円数 == 点数。
    assert_eq!(svg.matches("<circle").count(), 2);

    let radii = circle_radii(&svg);
    assert_eq!(radii.len(), 2);
    // 2 つの半径はデータ由来で異なる。
    assert_ne!(radii[0], radii[1]);
    // データの r がそのまま半径になる(入力順: 5 → 20)。
    assert_eq!(radii[0], "5");
    assert_eq!(radii[1], "20");
    // r が大きいデータ点ほど大きい円。
    let r0: f64 = radii[0].parse().unwrap();
    let r1: f64 = radii[1].parse().unwrap();
    assert!(r1 > r0);

    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn bubble_deterministic() {
    let j = r#"{"type":"bubble","data":{"datasets":[{"data":[{"x":1,"y":2,"r":5},{"x":3,"y":4,"r":20}]}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn bubble_snapshot() {
    let svg = render(
        r#"{"type":"bubble","data":{"datasets":[{"label":"観測","data":[{"x":1,"y":2,"r":5},{"x":3,"y":5,"r":20},{"x":4,"y":3,"r":12}]}]},"options":{"plugins":{"title":{"display":true,"text":"バブルチャート"}}}}"#,
    );
    insta::assert_snapshot!(svg);
}
