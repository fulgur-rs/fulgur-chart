# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name = "fulgur_chart"
  spec.version = "0.1.0"
  spec.authors = ["Fulgur"]
  spec.summary = "Render chart.js / Vega-Lite specs to deterministic SVG/PNG (Rust core)"
  spec.description = spec.summary
  spec.homepage = "https://github.com/fulgur-rs/fulgur-chart"
  spec.license = "MIT OR Apache-2.0"
  spec.required_ruby_version = ">= 3.0"

  spec.files = Dir["lib/**/*.rb", "ext/**/*.{rs,toml,rb,lock}", "README.md"]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/fulgur_chart/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9"
end
