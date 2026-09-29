//! DSL フロントエンド。各 DSL を IR(ChartSpec) に変換する。
pub mod chartjs;
pub mod vegalite;
mod vegalite_boxplot;
// This module is implemented before public dispatch in Task 4, so its private entry points are
// temporarily reached only by its unit tests.
#[allow(dead_code)]
mod vegalite_composition;
mod vegalite_error;
