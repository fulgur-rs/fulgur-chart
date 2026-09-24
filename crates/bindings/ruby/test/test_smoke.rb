# frozen_string_literal: true

require_relative "test_helper"

class TestSmoke < Minitest::Test
  def test_version_is_string
    assert_kind_of String, FulgurChart.version
    assert_match(/\A\d+\.\d+\.\d+\z/, FulgurChart.version)
  end

  def test_webp_renders_with_riff_container
    out = FulgurChart.render(Fixtures::BAR, :webp)
    assert_equal Encoding::ASCII_8BIT, out.encoding
    assert_equal "RIFF", out.byteslice(0, 4)
    assert_equal "WEBP", out.byteslice(8, 4)
  end
end
