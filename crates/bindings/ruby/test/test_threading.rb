# frozen_string_literal: true

require_relative "test_helper"
require "timeout"

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

  # The render uses rb_thread_call_without_gvl2: an interrupt arriving mid-render is NOT
  # processed inside the call (the render runs to completion), but is honored at Ruby's next
  # checkpoint after the call returns. These guard that interrupts are still honored — not
  # silently dropped — which is the real regression risk of the v1->v2 switch. The loops are
  # capped so a dropped-interrupt regression fails the assertion instead of hanging forever.
  def test_timeout_is_honored_across_renders
    reference = FulgurChart.render(Fixtures::BAR, :svg)
    assert_raises(Timeout::Error) do
      Timeout.timeout(0.05) do
        2000.times { FulgurChart.render(Fixtures::BAR, :png, width: 600, height: 400, scale: 2.0) }
      end
    end
    # Interrupt was honored (raised); the VM is healthy and still deterministic.
    assert_equal reference, FulgurChart.render(Fixtures::BAR, :svg)
  end

  def test_thread_kill_during_render_is_honored
    reference = FulgurChart.render(Fixtures::BAR, :svg)
    started = Queue.new
    t = Thread.new do
      started.push(:go)
      loop { FulgurChart.render(Fixtures::BAR, :png, width: 600, height: 400, scale: 2.0) }
    end
    started.pop  # ensure the thread has entered the render loop
    sleep 0.02
    t.kill
    t.join(5)    # bounded; if kill were dropped the thread would still be alive
    refute t.alive?, "Thread#kill must terminate a thread looping on renders"
    # VM healthy + deterministic after an interrupt crossed the nogvl boundary.
    assert_equal reference, FulgurChart.render(Fixtures::BAR, :svg)
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
