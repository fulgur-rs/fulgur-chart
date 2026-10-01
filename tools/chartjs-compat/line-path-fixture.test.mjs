import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Chart, LineElement } from 'chart.js/auto';
import { createCanvas } from 'canvas';

// Shared with Rust's fast_path_matches_chartjs_451_recorded_coordinates.
// Recording real LineElement commands keeps the Rust expectations independent
// of its implementation. Consecutive identical vertices do not change the path.
const fixture = JSON.parse(readFileSync(new URL('./line-path-fixture.json', import.meta.url)));

test('Rust line-path fixtures match the pinned Chart.js drawing implementation', () => {
  assert.equal(Chart.version, fixture.version);
  for (const { name, points, path, reverse = false } of fixture.cases) {
    const actual = [];
    const push = (x, y) => {
      const last = actual.at(-1);
      if (!last || x !== last[0] || y !== last[1]) actual.push([x, y]);
    };
    const line = new LineElement({
      points: points.map(([x, y]) => ({ x, y })),
      options: { tension: 0, stepped: false, borderDash: [] },
    });
    if (points.length) {
      line.pathSegment({ moveTo: push, lineTo: push }, {
        start: 0, end: points.length - 1, loop: false,
      }, { reverse });
    }
    assert.deepEqual(actual, path, name);
  }
});

test('category decimation retains all PointElements and default markers', () => {
  for (const decimation of [
    undefined, {}, { threshold: 1 },
    { enabled: true, threshold: 1 },
    { enabled: true, algorithm: 'lttb', samples: 3, threshold: 1 },
  ]) {
    const chart = new Chart(createCanvas(300, 220), {
      type: 'line',
      data: {
        labels: Array(5_000).fill(''),
        datasets: [{ data: Array.from({ length: 5_000 }, (_, i) => i % 73) }],
      },
      options: { animation: false, responsive: false, plugins: { decimation } },
    });
    try {
      const meta = chart.getDatasetMeta(0);
      assert.equal(chart.data.datasets[0].data.length, 5_000);
      assert.equal(meta.data.length, 5_000);
      assert.ok(meta.data.every(element => element.options.radius === 3));
    } finally {
      chart.destroy();
    }
  }
});
