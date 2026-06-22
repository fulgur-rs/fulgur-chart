# frozen_string_literal: true

# Demonstrates that rendering releases the GVL: the same workload spread across N
# Ruby threads finishes in well under the serial wall-clock on a multi-core machine.
# Without GVL release the threaded run would be ~equal to (or slower than) serial,
# since every render would serialize on the lock.
#
#   bundle exec ruby benchmark/nogvl_bench.rb

require "fulgur_chart"
require "benchmark"
require "json"

# A heavier render (many points → more layout + rasterization) so per-call cost
# dominates thread/scheduling overhead and parallelism is visible.
points = (0...200).map { |i| Math.sin(i / 7.0) * 50 + 60 }
labels = (0...200).map { |i| "p#{i}" }
SPEC = JSON.generate(
  type: "line",
  data: { labels: labels, datasets: [{ data: points }] }
)

THREADS = (ENV["THREADS"] || 4).to_i
PER_THREAD = (ENV["PER_THREAD"] || 30).to_i
TOTAL = THREADS * PER_THREAD

def render_n(count)
  count.times { FulgurChart.render(SPEC, :png, width: 800, height: 600, scale: 2.0) }
end

# Warm up (load font, JIT-less but caches, allocator).
render_n(2)

serial = Benchmark.realtime { render_n(TOTAL) }

threaded = Benchmark.realtime do
  ts = Array.new(THREADS) { Thread.new { render_n(PER_THREAD) } }
  ts.each(&:join)
end

puts "cores=#{`nproc`.to_i} threads=#{THREADS} per_thread=#{PER_THREAD} total=#{TOTAL} renders"
puts format("serial   : %.3f s", serial)
puts format("threaded : %.3f s", threaded)
puts format("speedup  : %.2fx", serial / threaded)
