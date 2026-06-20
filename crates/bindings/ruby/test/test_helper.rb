# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

module Fixtures
  BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
  LINE = '{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,3,2]}]}}'
  VEGALITE_BAR = '{"mark":"bar","data":{"values":[{"a":"x","b":1}]},"encoding":{"x":{"field":"a"},"y":{"field":"b"}}}'
  PNG_MAGIC = "\x89PNG".b
end
