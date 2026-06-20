# frozen_string_literal: true

require "minitest/autorun"
require "fulgur_chart"

class TestSmoke < Minitest::Test
  def test_version_is_string
    assert_kind_of String, FulgurChart.version
    assert_match(/\A\d+\.\d+\.\d+/, FulgurChart.version)
  end
end
