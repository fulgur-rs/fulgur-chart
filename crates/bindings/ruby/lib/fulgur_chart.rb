# frozen_string_literal: true

# Native extension. Defines the FulgurChart module with:
#   - FulgurChart.schema(dsl), FulgurChart.version
#   - FulgurChart.render(spec_json, format, **opts)  (low-level render primitive; the builder
#     below is the intended API, but render is also callable directly)
#   - errors:  FulgurChart::ParseError / StrictError / RenderError
# Module name is `FulgurChart` (NOT `Fulgur`) to avoid a top-level collision with the
# Fulgur (PDF) library when both gems are loaded in the same process.
require_relative "fulgur_chart/fulgur_chart"

module FulgurChart
  # Entry point for the builder API:
  #
  #   FulgurChart.build(spec_json).width(800).dsl(:chartjs).render(:svg)  # => String
  #   FulgurChart.build(spec_json).format(:png).render                    # => binary String
  #
  # `spec_json` is a chart.js v4 / Vega-Lite DSL JSON string. The behavior (DSL auto-detect,
  # options, error classes, determinism) follows docs/binding-api-contract.md.
  def self.build(spec_json)
    Builder.new(spec_json)
  end

  # Fluent, reusable builder. Setters mutate and return self; `render` may be called multiple
  # times and the builder may be reconfigured between calls.
  class Builder
    def initialize(spec_json)
      @spec = spec_json
      @opts = {}
    end

    # Override the chart width / height (px). Applied before input-limit validation.
    def width(value)  = set(:width, value)
    def height(value) = set(:height, value)

    # Raster scale factor (ignored when rendering SVG).
    def scale(value) = set(:scale, value)

    # Force the input DSL ("chartjs"/"vegalite" or the matching Symbol). Omit to auto-detect.
    def dsl(value) = set(:dsl, value)

    # TrueType/OpenType font bytes (binary String). Omit to use the bundled Noto Sans JP.
    def font(bytes) = set(:font, bytes)

    # Reject unknown keys. `strict` => true; `strict(false)` => false.
    def strict(value = true) = set(:strict, value)

    # Default output format for a terminal `render` with no argument (Symbol/String).
    def format(value) = set(:format, value)

    # Render to the given format ("svg"/"png" or the matching Symbol). Format precedence:
    # explicit argument > `.format()` setter > default :svg. Returns a UTF-8 String for svg
    # and a binary (ASCII-8BIT) String for png.
    def render(fmt = nil)
      resolved = fmt || @opts[:format] || :svg
      FulgurChart.render(@spec, resolved, **@opts.reject { |key, _| key == :format })
    end

    private

    def set(key, value)
      @opts[key] = value
      self
    end
  end
end
