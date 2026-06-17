use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

/// 棒(bar) + 折れ線(line) の混合。3 カテゴリ、片方の dataset に type:"line"。
const MIXED_JSON: &str = r#"{
  "type": "bar",
  "data": {
    "labels": ["1月", "2月", "3月"],
    "datasets": [
      { "label": "売上", "data": [120, 200, 150] },
      { "type": "line", "label": "目標", "data": [140, 180, 170] }
    ]
  },
  "options": { "plugins": { "title": { "display": true, "text": "売上と目標" } } }
}"#;

#[test]
fn mixed_has_bars_line_and_markers() {
    let svg = render(MIXED_JSON);
    // 棒は <rect>。3 カテゴリの棒(+凡例 swatch 2 つ)があるので少なくとも 3 本。
    assert!(svg.matches("<rect").count() >= 3, "棒(rect)が足りない");
    // 折れ線は <polyline>(tension=0)または曲線 <path>。
    assert!(
        svg.contains("<polyline") || svg.contains("<path"),
        "折れ線(polyline/path)が無い"
    );
    // 折れ線のマーカー(circle)を各点に。
    assert!(
        svg.matches("<circle").count() >= 3,
        "マーカー(circle)が足りない"
    );
    // カテゴリラベル。
    assert!(
        svg.contains("1月") && svg.contains("3月"),
        "カテゴリラベルが無い"
    );
    // 凡例(系列名)。
    assert!(svg.contains("売上") && svg.contains("目標"), "凡例が無い");
    // 健全性。
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
}

#[test]
fn mixed_deterministic() {
    assert_eq!(render(MIXED_JSON), render(MIXED_JSON));
}

#[test]
fn mixed_snapshot() {
    let svg = render(MIXED_JSON);
    insta::assert_snapshot!(svg);
}
