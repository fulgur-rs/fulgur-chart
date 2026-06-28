// Jsonnet lets you add comments and use local variables to avoid repetition.
// This spec is equivalent to bar.json but parameterized for easy reuse.

local months = ["Jan", "Feb", "Mar", "Apr", "May"];
local color = "#36a2eb";
local title = "Monthly Revenue";

{
  type: "bar",
  data: {
    labels: months,
    datasets: [{
      label: "Revenue (10k JPY)",
      data: [120, 200, 150, 280, 240],
      backgroundColor: color,
    }],
  },
  options: {
    plugins: {
      title: { display: true, text: title },
      legend: { display: true, position: "top" },
    },
  },
}
