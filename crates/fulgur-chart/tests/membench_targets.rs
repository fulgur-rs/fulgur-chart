#[path = "../benches/cases.rs"]
mod cases;
#[path = "../benches/membench_targets.rs"]
mod membench_targets;

use membench_targets::OutputKind;

#[test]
fn every_case_has_svg_and_png_measurement_targets() {
    let cases = cases::all();
    let targets = membench_targets::all(&cases);

    assert_eq!(targets.len(), cases.len() * 2);
    for case in &cases {
        let svg = targets
            .iter()
            .find(|target| target.name == case.name)
            .unwrap_or_else(|| panic!("missing SVG target for {}", case.name));
        assert_eq!(svg.output, OutputKind::Svg);

        let png_name = format!("{}_png", case.name);
        let png = targets
            .iter()
            .find(|target| target.name == png_name)
            .unwrap_or_else(|| panic!("missing PNG target for {}", case.name));
        assert_eq!(png.output, OutputKind::Png);
    }

    let mut names: Vec<&str> = targets.iter().map(|target| target.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), targets.len(), "target names must be unique");
}

#[test]
fn targets_render_the_selected_output_format() {
    let cases = cases::all();
    let targets = membench_targets::all(&cases[..1]);

    let svg = membench_targets::render(&targets[0]).expect("SVG target renders");
    assert!(svg.starts_with(b"<svg"));

    let png = membench_targets::render(&targets[1]).expect("PNG target renders");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}
