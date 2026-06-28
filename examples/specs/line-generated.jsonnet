// std.range and std.map let you generate chart data programmatically.
// This plots one full cycle of a sine wave across 24 points.

local n = 24;
local pi = 3.14159265358979;
local xs = std.range(0, n - 1);

// Scale sine output ([-1, 1]) to a readable range ([0, 100]).
local sine(x) = std.floor(std.sin(2 * pi * x / n) * 50 + 50);

{
  type: "line",
  data: {
    labels: std.map(std.toString, xs),
    datasets: [{
      label: "sin(x)",
      data: std.map(sine, xs),
      borderColor: "#ff6384",
      tension: 0.4,
      pointRadius: 2,
    }],
  },
  options: {
    plugins: {
      title: { display: true, text: "Sine Wave (24 points)" },
      legend: { display: true, position: "top" },
    },
  },
}
