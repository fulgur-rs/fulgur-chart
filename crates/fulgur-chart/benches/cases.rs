//! Representative benchmark cases shared by the `render` (speed) and `membench`
//! (memory) bench targets. JSON is generated programmatically (deterministic,
//! no path coupling to examples/). Pure std — no criterion/dhat imports — so it
//! can also be `#[path]`-included by an integration test.
#![allow(dead_code)] // not every includer uses every helper

/// A single benchmark case: a name and a ready-to-parse chart.js JSON spec.
pub struct Case {
    pub name: &'static str,
    pub json: String,
}

/// All benchmark cases, small + synthetic-large, E2E-oriented.
pub fn all() -> Vec<Case> {
    vec![
        Case {
            name: "bar_small",
            json: bar(12),
        },
        Case {
            name: "bar_large",
            json: bar(1_000),
        },
        Case {
            name: "line_small",
            json: line(12),
        },
        Case {
            name: "line_large",
            json: line(10_000),
        },
        Case {
            name: "scatter_large",
            json: scatter(10_000),
        },
        Case {
            name: "pie_small",
            json: pie(6),
        },
    ]
}

/// Deterministic pseudo value in [0, 100) from an index (no RNG → reproducible).
fn val(i: usize) -> usize {
    (i * 37 + 13) % 100
}

fn labels(n: usize) -> String {
    (0..n)
        .map(|i| format!("\"L{i}\""))
        .collect::<Vec<_>>()
        .join(",")
}

fn numbers(n: usize) -> String {
    (0..n)
        .map(|i| val(i).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn bar(n: usize) -> String {
    format!(
        r#"{{"type":"bar","data":{{"labels":[{}],"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        labels(n),
        numbers(n)
    )
}

fn line(n: usize) -> String {
    format!(
        r#"{{"type":"line","data":{{"labels":[{}],"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        labels(n),
        numbers(n)
    )
}

fn scatter(n: usize) -> String {
    let pts = (0..n)
        .map(|i| format!(r#"{{"x":{},"y":{}}}"#, i, val(i)))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"type":"scatter","data":{{"datasets":[{{"label":"d","data":[{pts}]}}]}}}}"#)
}

fn pie(n: usize) -> String {
    format!(
        r#"{{"type":"pie","data":{{"labels":[{}],"datasets":[{{"label":"d","data":[{}]}}]}}}}"#,
        labels(n),
        numbers(n)
    )
}
