# frozen_string_literal: true

require_relative "test_helper"

class TestBuilder < Minitest::Test
  # --- rendering ---

  def test_render_svg_returns_svg_string
    out = FulgurChart.build(Fixtures::BAR).render(:svg)
    assert_kind_of String, out
    assert_equal Encoding::UTF_8, out.encoding
    assert out.start_with?("<svg"), "expected <svg, got #{out[0, 20].inspect}"
  end

  def test_render_png_returns_binary_string
    out = FulgurChart.build(Fixtures::BAR).render(:png)
    assert_equal Encoding::ASCII_8BIT, out.encoding
    assert out.start_with?(Fixtures::PNG_MAGIC), "expected PNG magic"
  end

  def test_vegalite_autodetected
    assert FulgurChart.build(Fixtures::VEGALITE_BAR).render(:svg).start_with?("<svg")
  end

  # --- format precedence: argument > .format() setter > default :svg ---

  def test_default_format_is_svg
    assert FulgurChart.build(Fixtures::BAR).render.start_with?("<svg")
  end

  def test_format_setter_used_when_no_argument
    assert FulgurChart.build(Fixtures::BAR).format(:png).render.start_with?(Fixtures::PNG_MAGIC)
  end

  def test_render_argument_overrides_format_setter
    out = FulgurChart.build(Fixtures::BAR).format(:png).render(:svg)
    assert out.start_with?("<svg"), "render(:svg) must win over .format(:png)"
  end

  # --- chainable setters: width/height/scale/dsl/strict ---

  def test_width_height_override
    big = FulgurChart.build(Fixtures::BAR).width(1234.0).height(567.0).render(:svg)
    assert_includes big, 'width="1234"'
    assert_includes big, 'height="567"'
  end

  def test_scale_changes_png
    refute_equal FulgurChart.build(Fixtures::BAR).scale(1.0).render(:png),
                 FulgurChart.build(Fixtures::BAR).scale(2.0).render(:png)
  end

  def test_dsl_override_switches_parser
    # VEGALITE_BAR auto-detects vegalite; forcing chartjs must actually switch the parser
    # (the vegalite spec is invalid chartjs) → ParseError. Fails if the override is ignored.
    assert FulgurChart.build(Fixtures::VEGALITE_BAR).render(:svg).start_with?("<svg")
    assert_raises(FulgurChart::ParseError) do
      FulgurChart.build(Fixtures::VEGALITE_BAR).dsl(:chartjs).render(:svg)
    end
  end

  # --- builder is reusable; setters and render return chainably/deterministically ---

  def test_setters_return_self_for_chaining
    b = FulgurChart.build(Fixtures::BAR)
    assert_same b, b.width(800)
    assert_same b, b.strict(false)
  end

  def test_builder_reuse_is_deterministic
    b = FulgurChart.build(Fixtures::BAR)
    assert_equal b.render(:svg), b.render(:svg)
    assert_equal b.render(:png), b.render(:png)
  end

  def test_builder_reconfigurable_between_renders
    b = FulgurChart.build(Fixtures::BAR)
    small = b.width(400.0).render(:svg)
    big = b.width(1234.0).render(:svg)
    assert_includes small, 'width="400"'
    assert_includes big, 'width="1234"'
  end

  # --- String and Symbol both accepted for dsl / format ---

  def test_symbol_and_string_options_equivalent
    assert_equal FulgurChart.build(Fixtures::LINE).dsl("chartjs").render("svg"),
                 FulgurChart.build(Fixtures::LINE).dsl(:chartjs).render(:svg)
  end

  # --- errors (call-site classification preserved) ---

  def test_unknown_format_raises_parse_error
    assert_raises(FulgurChart::ParseError) { FulgurChart.build(Fixtures::BAR).render(:zzz) }
  end

  def test_invalid_json_raises_parse_error
    assert_raises(FulgurChart::ParseError) { FulgurChart.build("not json").render(:svg) }
  end

  def test_undetectable_dsl_raises_parse_error
    assert_raises(FulgurChart::ParseError) { FulgurChart.build('{"labels":[]}').render(:svg) }
  end

  def test_strict_unknown_key_raises_strict_error
    spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
    assert_raises(FulgurChart::StrictError) { FulgurChart.build(spec).strict.render(:svg) }
  end

  def test_strict_error_is_parse_error_subclass
    assert FulgurChart::StrictError.ancestors.include?(FulgurChart::ParseError)
  end

  def test_dimension_over_limit_raises_parse_error
    assert_raises(FulgurChart::ParseError) { FulgurChart.build(Fixtures::BAR).width(40000.0).render(:svg) }
  end

  # font-error asymmetry: SVG path → ParseError, image path → RenderError
  def test_invalid_font_on_svg_path_raises_parse_error
    assert_raises(FulgurChart::ParseError) do
      FulgurChart.build(Fixtures::BAR).font("not a font".b).render(:svg)
    end
  end

  def test_invalid_font_on_image_path_raises_render_error
    assert_raises(FulgurChart::RenderError) do
      FulgurChart.build(Fixtures::BAR).font("not a font".b).render(:png)
    end
  end

  # --- low-level FulgurChart.render primitive (same name; the builder calls it) ---

  def test_direct_render_primitive
    assert FulgurChart.render(Fixtures::BAR, "svg").start_with?("<svg")
    assert FulgurChart.render(Fixtures::BAR, :png).start_with?(Fixtures::PNG_MAGIC)
    assert_includes FulgurChart.render(Fixtures::BAR, :svg, width: 800.0), 'width="800"'
  end

  def test_direct_render_equals_builder_render
    assert_equal FulgurChart.render(Fixtures::BAR, :png, width: 640.0),
                 FulgurChart.build(Fixtures::BAR).width(640.0).render(:png)
  end
end
