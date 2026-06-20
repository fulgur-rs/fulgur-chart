# frozen_string_literal: true

require_relative "test_helper"

class TestSmoke < Minitest::Test
  def test_version_is_string
    assert_kind_of String, FulgurChart.version
    assert_match(/\A\d+\.\d+\.\d+\z/, FulgurChart.version)
  end
end
