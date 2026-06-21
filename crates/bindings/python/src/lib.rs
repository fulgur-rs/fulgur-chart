use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

pyo3::create_exception!(fulgur_chart, FulgurParseError, PyValueError);
pyo3::create_exception!(fulgur_chart, FulgurStrictError, FulgurParseError);
pyo3::create_exception!(fulgur_chart, FulgurRenderError, PyRuntimeError);

fn parse_error(msg: impl Into<String>) -> PyErr {
    FulgurParseError::new_err(msg.into())
}

fn strict_error(msg: impl Into<String>) -> PyErr {
    FulgurStrictError::new_err(msg.into())
}

fn render_error(msg: impl Into<String>) -> PyErr {
    FulgurRenderError::new_err(msg.into())
}

#[pymodule]
fn fulgur_chart(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FulgurParseError", m.py().get_type::<FulgurParseError>())?;
    m.add("FulgurStrictError", m.py().get_type::<FulgurStrictError>())?;
    m.add("FulgurRenderError", m.py().get_type::<FulgurRenderError>())?;
    Ok(())
}
