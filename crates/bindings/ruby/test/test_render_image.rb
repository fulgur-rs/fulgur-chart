# frozen_string_literal: true

require_relative "test_helper"

class TestRenderImage < Minitest::Test
  def test_render_png_magic_bytes
    out = FulgurChart.render_png(Fixtures::BAR)
    assert_kind_of String, out
    assert_equal Encoding::ASCII_8BIT, out.encoding
    assert out.start_with?(Fixtures::PNG_MAGIC), "expected PNG magic"
  end

  def test_render_image_png_equals_render_png
    assert_equal FulgurChart.render_image(Fixtures::BAR, format: "png"), FulgurChart.render_png(Fixtures::BAR)
  end

  def test_unknown_format_raises_parse_error
    assert_raises(FulgurChart::ParseError) { FulgurChart.render_image(Fixtures::BAR, format: "zzz") }
  end

  def test_invalid_font_on_image_path_raises_render_error
    assert_raises(FulgurChart::RenderError) do
      FulgurChart.render_png(Fixtures::BAR, font: "not a font".b)
    end
  end

  def test_render_image_with_options
    assert FulgurChart.render_image(Fixtures::BAR, format: "png", width: 400.0, scale: 2.0).start_with?(Fixtures::PNG_MAGIC)
  end

  def test_png_determinism
    assert_equal FulgurChart.render_png(Fixtures::BAR), FulgurChart.render_png(Fixtures::BAR)
  end

  def test_svg_and_png_differ
    refute_equal FulgurChart.render_svg(Fixtures::BAR).b, FulgurChart.render_png(Fixtures::BAR)
  end
end
