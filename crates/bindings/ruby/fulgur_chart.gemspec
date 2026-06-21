# frozen_string_literal: true

# Version is read dynamically from ext/fulgur_chart/Cargo.toml so that
# `gem build` in CI always picks up the version set by the release workflow.
ext_toml = File.read(File.join(__dir__, "ext/fulgur_chart/Cargo.toml"))
package_section = ext_toml.match(/^\[package\](.*?)(?=^\[|\z)/m)&.captures&.first || ""
m = package_section.match(/^version\s*=\s*"([^"]+)"/)
raise "Could not parse version from ext/fulgur_chart/Cargo.toml" unless m
crate_version = m[1]

Gem::Specification.new do |spec|
  spec.name = "fulgur_chart"
  spec.version = crate_version
  spec.authors = ["Fulgur"]
  spec.summary = "Render chart.js / Vega-Lite specs to deterministic SVG/PNG (Rust core)"
  spec.description = spec.summary
  spec.homepage = "https://github.com/fulgur-rs/fulgur-chart"
  spec.license = "MIT OR Apache-2.0"
  spec.required_ruby_version = ">= 3.0"

  spec.metadata = {
    "homepage_uri"    => spec.homepage,
    "source_code_uri" => spec.homepage,
    "changelog_uri"   => "#{spec.homepage}/blob/main/crates/fulgur-chart/CHANGELOG.md",
  }

  spec.files = Dir["lib/**/*.rb", "ext/**/*.{rs,toml,rb,lock}", "README.md"]
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/fulgur_chart/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9"
end
