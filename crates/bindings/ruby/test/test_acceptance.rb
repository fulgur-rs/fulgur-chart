# frozen_string_literal: true

require_relative "test_helper"

class TestAcceptance < Minitest::Test
  CJ = Fixtures::LINE
  VL = Fixtures::VEGALITE_BAR

  # Public surface is exactly: build (builder entry), render (low-level primitive), schema, version.
  def test_public_surface
    assert_equal %i[build render schema version], FulgurChart.methods(false).sort
  end

  def test_no_top_level_fulgur_constant
    # `Fulgur` (the PDF library namespace) must not be defined by requiring this gem.
    refute defined?(Fulgur), "top-level Fulgur must not be defined (collision with Fulgur PDF)"
  end

  def test_builder_full_flow
    assert FulgurChart.build(CJ).render(:svg).start_with?("<svg")
    assert FulgurChart.build(CJ).render(:png).start_with?(Fixtures::PNG_MAGIC)
    assert FulgurChart.build(VL).render(:svg).start_with?("<svg")
  end

  def test_meta_functions
    require "json"
    assert_kind_of Hash, JSON.parse(FulgurChart.schema(:chartjs))
    assert_kind_of Hash, JSON.parse(FulgurChart.schema("vegalite"))
    assert_match(/\A\d+\.\d+\.\d+\z/, FulgurChart.version)
  end

  def test_determinism
    assert_equal FulgurChart.build(CJ).render(:svg), FulgurChart.build(CJ).render(:svg)
    assert_equal FulgurChart.build(CJ).render(:png), FulgurChart.build(CJ).render(:png)
  end
end
