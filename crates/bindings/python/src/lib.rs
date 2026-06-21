use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

pyo3::create_exception!(fulgur_chart, FulgurParseError, PyValueError);
pyo3::create_exception!(fulgur_chart, FulgurStrictError, FulgurParseError);
pyo3::create_exception!(fulgur_chart, FulgurRenderError, PyRuntimeError);

fn parse_error(msg: impl Into<String>) -> PyErr {
    FulgurParseError::new_err(msg.into())
}

#[allow(dead_code)]
fn strict_error(msg: impl Into<String>) -> PyErr {
    FulgurStrictError::new_err(msg.into())
}

#[allow(dead_code)]
fn render_error(msg: impl Into<String>) -> PyErr {
    FulgurRenderError::new_err(msg.into())
}

#[pyfunction]
fn version() -> &'static str {
    fulgur_core::version()
}

#[pyfunction]
fn schema(dsl: &str) -> PyResult<String> {
    let s = match dsl {
        "chartjs" => serde_json::to_string(
            &schemars::schema_for!(fulgur_core::schema::ChartJsSpec),
        )
        .unwrap(),
        "vegalite" => serde_json::to_string(
            &schemars::schema_for!(fulgur_core::schema::VegaLiteSpec),
        )
        .unwrap(),
        other => return Err(parse_error(format!("未知のDSL: {other}"))),
    };
    Ok(s)
}

#[pymodule]
fn fulgur_chart(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FulgurParseError", m.py().get_type::<FulgurParseError>())?;
    m.add("FulgurStrictError", m.py().get_type::<FulgurStrictError>())?;
    m.add("FulgurRenderError", m.py().get_type::<FulgurRenderError>())?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(schema, m)?)?;
    Ok(())
}
