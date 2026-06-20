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
    # Explicit correct dsl renders.
    assert FulgurChart.render_svg(CJ, dsl: "chartjs").start_with?("<svg")
    # Discriminating: VL auto-detects as vegalite and renders, but forcing dsl:chartjs must
    # actually switch the parser (the vegalite spec is invalid chartjs) → ParseError. This
    # fails if the override is silently ignored.
    assert FulgurChart.render_svg(VL).start_with?("<svg")
    assert_raises(FulgurChart::ParseError) { FulgurChart.render_svg(VL, dsl: "chartjs") }
  end

  def test_scale_changes_png
    refute_equal FulgurChart.render_png(CJ, scale: 1.0), FulgurChart.render_png(CJ, scale: 2.0)
  end

  def test_determinism_svg_and_png
    assert_equal FulgurChart.render_svg(CJ), FulgurChart.render_svg(CJ)
    assert_equal FulgurChart.render_png(CJ), FulgurChart.render_png(CJ)
  end

  # Ruby callers idiomatically pass symbols for enum-like options; String and Symbol must
  # behave identically for dsl / format / schema (no TypeError on symbols).
  def test_symbol_options_accepted
    assert_equal FulgurChart.render_svg(CJ, dsl: "chartjs"),
                 FulgurChart.render_svg(CJ, dsl: :chartjs)
    assert_equal FulgurChart.render_image(CJ, format: "png"),
                 FulgurChart.render_image(CJ, format: :png)
    assert_equal FulgurChart.schema("chartjs"), FulgurChart.schema(:chartjs)
  end

  def test_symbol_unknown_dsl_still_parse_error
    assert_raises(FulgurChart::ParseError) { FulgurChart.render_svg(CJ, dsl: :nope) }
    assert_raises(FulgurChart::ParseError) { FulgurChart.schema(:nope) }
    assert_raises(FulgurChart::ParseError) { FulgurChart.render_image(CJ, format: :zzz) }
  end
end
