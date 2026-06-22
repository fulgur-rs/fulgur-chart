//! Verifies every benchmark case generates valid chart.js JSON that parses and
//! renders to a non-empty SVG. Shares the generators with the bench targets via
//! `#[path]` include so the benched inputs are exactly what we test here.
#[path = "../benches/cases.rs"]
mod cases;

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

#[test]
fn every_case_parses_and_renders() {
    let all = cases::all();
    assert!(!all.is_empty(), "expected at least one case");
    for case in &all {
        let spec = chartjs::parse(&case.json, false)
            .unwrap_or_else(|e| panic!("case {} failed to parse: {e}", case.name));
        let svg = render_chart(&spec);
        assert!(
            svg.starts_with("<svg"),
            "case {} did not render an SVG",
            case.name
        );
    }
}

#[test]
fn case_names_are_unique() {
    let all = cases::all();
    let mut names: Vec<&str> = all.iter().map(|c| c.name).collect();
    names.sort_unstable();
    let n = names.len();
    names.dedup();
    assert_eq!(names.len(), n, "case names must be unique");
}

#[test]
fn large_cases_have_expected_scale() {
    let all = cases::all();
    let line_large = all.iter().find(|c| c.name == "line_large").unwrap();
    // 10k points => the JSON is large; cheap sanity check on scale.
    assert!(line_large.json.len() > 10_000);
}
