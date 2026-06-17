//! IR → SVG の最上位エントリ。

pub fn render_chart(spec: &crate::ir::ChartSpec) -> String {
    let scene = crate::layout::build_scene(spec);
    crate::svg::render_svg(&scene)
}
