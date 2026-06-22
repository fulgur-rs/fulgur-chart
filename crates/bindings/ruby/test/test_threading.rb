# frozen_string_literal: true

require_relative "test_helper"

# Guards the safety of the GVL-releasing render region: rendering concurrently
# from many Ruby threads must produce byte-identical output to a single-threaded
# render and must never crash. This is the invariant that could break if the
# nogvl region touched a Ruby VALUE, skipped catch_unwind, or aliased state.
class TestThreading < Minitest::Test
  THREADS = 8
  ITERATIONS = 4

  def test_concurrent_svg_matches_serial
    reference = FulgurChart.render(Fixtures::BAR, :svg)
    assert_concurrent_renders_match(reference, :svg)
  end

  def test_concurrent_png_matches_serial
    reference = FulgurChart.render(Fixtures::BAR, :png)
    assert reference.start_with?(Fixtures::PNG_MAGIC)
    assert_concurrent_renders_match(reference, :png)
  end

  # A thread that raises inside the render region (invalid spec) must surface the
  # exception normally without corrupting other in-flight renders or the VM.
  def test_concurrent_errors_are_isolated
    good = FulgurChart.render(Fixtures::LINE, :svg)
    threads = Array.new(THREADS) do |i|
      Thread.new do
        if i.even?
          FulgurChart.render(Fixtures::LINE, :svg)
        else
          begin
            FulgurChart.render("{ not json", :svg)
            :no_raise
          rescue FulgurChart::ParseError
            :raised
          end
        end
      end
    end
    results = threads.map(&:value)
    results.each_with_index do |r, i|
      assert_equal(i.even? ? good : :raised, r)
    end
  end

  private

  def assert_concurrent_renders_match(reference, format)
    threads = Array.new(THREADS) do
      Thread.new do
        Array.new(ITERATIONS) { FulgurChart.render(Fixtures::BAR, format) }
      end
    end
    threads.flat_map(&:value).each do |out|
      assert_equal reference, out
    end
  end
end
