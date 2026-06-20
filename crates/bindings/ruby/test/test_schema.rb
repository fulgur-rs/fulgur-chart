# frozen_string_literal: true

require "minitest/autorun"
require "json"
require "fulgur_chart"

class TestSchema < Minitest::Test
  def test_chartjs_schema_is_json
    s = FulgurChart.schema("chartjs")
    assert_kind_of String, s
    assert_kind_of Hash, JSON.parse(s)
  end

  def test_vegalite_schema_is_json
    assert_kind_of Hash, JSON.parse(FulgurChart.schema("vegalite"))
  end

  def test_unknown_dsl_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.schema("nope") }
  end

  def test_schema_determinism
    assert_equal FulgurChart.schema("chartjs"), FulgurChart.schema("chartjs")
  end
end
