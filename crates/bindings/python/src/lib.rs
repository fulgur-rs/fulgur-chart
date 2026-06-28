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

fn detect_dsl(json: &str) -> Option<&'static str> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("mark").is_some() {
        Some("vegalite")
    } else if v.get("type").is_some() {
        Some("chartjs")
    } else {
        None
    }
}

fn parse_spec(json: &str, strict: bool, dsl: &str) -> PyResult<fulgur_core::ir::ChartSpec> {
    // 非 strict でパース（JSON / DSL 構文エラー → ParseError）
    let spec = match dsl {
        "chartjs" => fulgur_core::frontend::chartjs::parse(json, false),
        "vegalite" => fulgur_core::frontend::vegalite::parse(json, false),
        other => return Err(parse_error(format!("未知のDSL: {other}"))),
    }
    .map_err(parse_error)?;

    // strict モードなら strict=true で再パース（未知キー → StrictError）
    if strict {
        let _ = match dsl {
            "chartjs" => fulgur_core::frontend::chartjs::parse(json, true),
            "vegalite" => fulgur_core::frontend::vegalite::parse(json, true),
            _ => unreachable!(),
        }
        .map_err(strict_error)?;
    }

    Ok(spec)
}

fn build_ir(
    spec_json: &str,
    width: Option<f64>,
    height: Option<f64>,
    strict: bool,
    dsl: Option<&str>,
) -> PyResult<fulgur_core::ir::ChartSpec> {
    let dsl_name = match dsl {
        Some(d) => d,
        None => detect_dsl(spec_json)
            .ok_or_else(|| parse_error("DSL自動判定失敗: 'mark'または'type'キーが必要"))?,
    };
    let mut spec = parse_spec(spec_json, strict, dsl_name)?;
    // ChartSpec.width / .height は f64 フィールド
    if let Some(w) = width {
        spec.width = w;
    }
    if let Some(h) = height {
        spec.height = h;
    }
    // 寸法制限チェック（1–32768 px）
    fulgur_core::guard::validate_spec(&spec, &fulgur_core::guard::InputLimits::default())
        .map_err(parse_error)?;
    Ok(spec)
}

#[pyfunction]
#[pyo3(signature = (spec_json, *, width=None, height=None, scale=1.0, strict=false, dsl=None, font=None))]
fn render_svg(
    spec_json: &str,
    width: Option<f64>,
    height: Option<f64>,
    scale: f64,
    strict: bool,
    dsl: Option<&str>,
    font: Option<&[u8]>,
) -> PyResult<String> {
    let _ = scale; // render_svg では scale を使用しない（API 仕様通り）
    let spec = build_ir(spec_json, width, height, strict, dsl)?;
    if let Some(font_bytes) = font {
        // SVG パスのフォントエラー → ParseError（binding-api-contract の非対称規約）
        fulgur_core::render::render_chart_with_font(&spec, font_bytes).map_err(parse_error)
    } else {
        Ok(fulgur_core::render::render_chart(&spec))
    }
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (spec_json, format, *, width=None, height=None, scale=1.0, strict=false, dsl=None, font=None))]
fn render_image<'py>(
    py: Python<'py>,
    spec_json: &str,
    format: &str,
    width: Option<f64>,
    height: Option<f64>,
    scale: f64,
    strict: bool,
    dsl: Option<&str>,
    font: Option<&[u8]>,
) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    let spec = build_ir(spec_json, width, height, strict, dsl)?;
    let font_bytes = font.unwrap_or(fulgur_core::font::DEFAULT_FONT);
    match format {
        "png" => {
            // PNG パスの全エラー（フォントエラー含む）→ RenderError（binding-api-contract の非対称規約）
            let png_data =
                fulgur_core::raster_direct::render_chart_to_png(&spec, scale as f32, font_bytes)
                    .map_err(render_error)?;
            Ok(pyo3::types::PyBytes::new(py, &png_data))
        }
        "webp" => {
            let webp_data =
                fulgur_core::raster_direct::render_chart_to_webp(&spec, scale as f32, font_bytes)
                    .map_err(render_error)?;
            Ok(pyo3::types::PyBytes::new(py, &webp_data))
        }
        other => Err(parse_error(format!(
            "サポートされていないフォーマット: '{other}' (supported: png, webp)"
        ))),
    }
}

#[pyfunction]
fn version() -> &'static str {
    fulgur_core::version()
}

#[pyfunction]
fn schema(dsl: &str) -> PyResult<String> {
    let s = match dsl {
        "chartjs" => {
            serde_json::to_string(&schemars::schema_for!(fulgur_core::schema::ChartJsSpec)).unwrap()
        }
        "vegalite" => {
            serde_json::to_string(&schemars::schema_for!(fulgur_core::schema::VegaLiteSpec))
                .unwrap()
        }
        other => return Err(parse_error(format!("未知のDSL: {other}"))),
    };
    Ok(s)
}

#[pymodule]
fn fulgur_chart(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FulgurParseError", m.py().get_type::<FulgurParseError>())?;
    m.add("FulgurStrictError", m.py().get_type::<FulgurStrictError>())?;
    m.add("FulgurRenderError", m.py().get_type::<FulgurRenderError>())?;
    m.add_function(wrap_pyfunction!(render_svg, m)?)?;
    m.add_function(wrap_pyfunction!(render_image, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(schema, m)?)?;
    Ok(())
}
