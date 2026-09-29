//! DSL フロントエンド。各 DSL を IR(ChartSpec) に変換する。
pub mod chartjs;
pub mod vegalite;
#[cfg(test)]
mod vegalite_boxplot;
mod vegalite_error;
