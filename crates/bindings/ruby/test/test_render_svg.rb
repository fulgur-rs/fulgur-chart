# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'

class TestRenderSvg < Minitest::Test
  def test_returns_svg_string
    out = FulgurChart.render_svg(BAR)
    assert_kind_of String, out
    assert out.start_with?("<svg"), "expected <svg, got #{out[0, 20].inspect}"
  end

  def test_invalid_json_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.render_svg("not json") }
  end

  def test_undetectable_dsl_raises_parse_error
    assert_raises(Fulgur::ParseError) { FulgurChart.render_svg('{"labels":[]}') }
  end

  def test_strict_unknown_key_raises_strict_error
    spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
    assert_raises(Fulgur::StrictError) { FulgurChart.render_svg(spec, strict: true) }
  end

  def test_strict_error_is_parse_error_subclass
    assert Fulgur::StrictError.ancestors.include?(Fulgur::ParseError)
  end

  def test_invalid_font_on_svg_path_raises_parse_error
    assert_raises(Fulgur::ParseError) do
      FulgurChart.render_svg(BAR, font: "not a font".b)
    end
  end

  def test_width_height_override
    big = FulgurChart.render_svg(BAR, width: 1234.0, height: 567.0)
    assert_includes big, 'width="1234"'
    assert_includes big, "567"
  end

  def test_dimension_over_limit_raises_parse_error
    assert_raises(Fulgur::ParseError) do
      FulgurChart.render_svg(BAR, width: 40000.0)
    end
  end
end
