use wasm_bindgen::prelude::*;

/// Return the crate version string of the core (mirrors the CLI / other bindings).
#[wasm_bindgen]
pub fn version() -> String {
    fulgur_chart::version().to_string()
}
