use fulgur_chart::frontend::vegalite;
use fulgur_chart::render::render_chart;

fn parsed() -> fulgur_chart::ir::ChartSpec {
    vegalite::parse(
        include_str!("../../../examples/specs/vegalite_geoshape.json"),
        true,
    )
    .expect("geoshape fixture parses")
}

#[test]
fn geoshape_snapshot() {
    let svg = render_chart(&parsed());
    insta::assert_snapshot!(svg);
}
