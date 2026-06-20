# frozen_string_literal: true

require_relative "test_helper"

class TestAcceptance < Minitest::Test
  CJ = Fixtures::LINE
  VL = Fixtures::VEGALITE_BAR

  def test_full_api_present
    %i[render_svg render_image render_png schema version].each do |m|
      assert FulgurChart.respond_to?(m), "FulgurChart.#{m} missing"
    end
  end

  def test_chartjs_svg_and_png
    assert FulgurChart.render_svg(CJ).start_with?("<svg")
    assert FulgurChart.render_png(CJ).start_with?(Fixtures::PNG_MAGIC)
  end

  def test_vegalite_autodetected
    assert FulgurChart.render_svg(VL).start_with?("<svg")
  end

  def test_dsl_override
    assert FulgurChart.render_svg(CJ, dsl: "chartjs").start_with?("<svg")
  end

  def test_scale_changes_png
    refute_equal FulgurChart.render_png(CJ, scale: 1.0), FulgurChart.render_png(CJ, scale: 2.0)
  end

  def test_determinism_svg_and_png
    assert_equal FulgurChart.render_svg(CJ), FulgurChart.render_svg(CJ)
    assert_equal FulgurChart.render_png(CJ), FulgurChart.render_png(CJ)
  end
end
