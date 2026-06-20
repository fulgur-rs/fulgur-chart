# frozen_string_literal: true

require_relative "fulgur_chart/fulgur_chart" # native ext (Init_fulgur_chart -> module Fulgur)

# Contract-compliant error hierarchy. native side defines these; this is a safety net
# (idempotent via const_defined? guards) so requiring the file is robust.
module Fulgur
  class ParseError < StandardError; end unless const_defined?(:ParseError)
  class StrictError < ParseError; end unless const_defined?(:StrictError)
  class RenderError < StandardError; end unless const_defined?(:RenderError)
end

# Acceptance criteria use FulgurChart.* — provide it as an alias of Fulgur.
FulgurChart = Fulgur unless defined?(FulgurChart)
