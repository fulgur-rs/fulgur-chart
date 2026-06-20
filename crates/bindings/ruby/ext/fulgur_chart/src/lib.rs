use magnus::{function, prelude::*, Error, Ruby};

fn version() -> String {
    fulgur_chart::version().to_string()
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("Fulgur")?;
    module.define_module_function("version", function!(version, 0))?;
    Ok(())
}
