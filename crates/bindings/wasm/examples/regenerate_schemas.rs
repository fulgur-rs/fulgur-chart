use std::{error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let schema_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let chartjs = serde_json::to_string(&schemars::schema_for!(fulgur_chart::schema::ChartJsSpec))?;
    let vegalite =
        serde_json::to_string(&schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec))?;

    std::fs::write(schema_dir.join("chartjs-schema.json"), chartjs)?;
    std::fs::write(schema_dir.join("vegalite-schema.json"), vegalite)?;
    Ok(())
}
