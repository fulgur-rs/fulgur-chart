//! Measurement targets shared by the memory bench and integration tests.

use crate::cases::Case;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::raster_direct::render_chart_to_png_default;
use fulgur_chart::render::render_chart;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputKind {
    Svg,
    Png,
}

pub struct MeasurementTarget<'a> {
    pub name: String,
    pub case: &'a Case,
    pub output: OutputKind,
}

pub fn all(cases: &[Case]) -> Vec<MeasurementTarget<'_>> {
    cases
        .iter()
        .flat_map(|case| {
            [
                MeasurementTarget {
                    name: case.name.to_string(),
                    case,
                    output: OutputKind::Svg,
                },
                MeasurementTarget {
                    name: format!("{}_png", case.name),
                    case,
                    output: OutputKind::Png,
                },
            ]
        })
        .collect()
}

pub fn render(target: &MeasurementTarget<'_>) -> Result<Vec<u8>, String> {
    let spec = chartjs::parse(&target.case.json, false)?;
    match target.output {
        OutputKind::Svg => Ok(render_chart(&spec).into_bytes()),
        OutputKind::Png => render_chart_to_png_default(&spec, 1.0),
    }
}
