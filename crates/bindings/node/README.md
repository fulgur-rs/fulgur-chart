# @fulgur-rs/chart-node

Node.js binding for [fulgur-chart](https://github.com/fulgur-rs/fulgur-chart) — render
chart.js v4 / Vega-Lite JSON specs to deterministic SVG/PNG via a Rust native addon
([napi-rs](https://napi.rs)).

> For a zero-install command-line tool, see `@fulgur-rs/chart-cli` (`npx @fulgur-rs/chart-cli …`).

## Requirements

- Node.js >= 18.17 (the addon targets Node-API v9, available from Node 18.17 / 20.3)
- A Rust toolchain (`cargo`) — building from source compiles the native addon.

## Build / test from source

```sh
cd crates/bindings/node
npm install
npm run build        # napi build -> native addon (.node) + binding.js / binding.d.ts
npm test             # node:test suite
npm run typecheck    # tsc --noEmit against index.d.ts
```

## Usage

The API is a fluent **builder**: `build(spec)` returns a builder you configure with chainable
setters and finish with `render('svg')` / `render('png')`.

```js
const fs = require('node:fs')
const { build, render, schema, version } = require('@fulgur-rs/chart-node')

const spec = JSON.stringify({
  type: 'bar',
  data: { labels: ['a', 'b', 'c'], datasets: [{ data: [1, 3, 2] }] },
})

// SVG (UTF-8 string)
const svg = build(spec).render('svg')
fs.writeFileSync('chart.svg', svg)

// PNG (Buffer)
const png = build(spec).width(800).height(600).scale(2).render('png')
fs.writeFileSync('chart.png', png)

// Set a default format with .format(), then call render() with no argument
const png2 = build(spec).format('png').render()

// The builder is reusable and reconfigurable between renders
const chart = build(spec).dsl('chartjs')
const a = chart.width(400).render('svg')
const b = chart.width(1234).render('svg')

// Low-level primitive (the builder calls this; also callable directly)
const svg2 = render(spec, 'svg', { width: 800 })

// JSON Schema for a DSL (compact JSON string) + version
const chartjsSchema = schema('chartjs')
console.log(version())
```

### ESM

```js
import { build } from '@fulgur-rs/chart-node'
const svg = build(spec).render('svg')
```

## API

- `build(specJson) -> Builder`
  - chainable setters (return `this`): `.width(n)` `.height(n)` `.scale(n)` `.dsl('chartjs'|'vegalite')` `.font(buffer)` `.strict(bool = true)` `.format('svg'|'png')`
  - terminal: `.render('svg')` -> `string`, `.render('png')` -> `Buffer`, `.render()` -> svg (default)
- `render(specJson, format, options?)` — low-level primitive
- `schema('chartjs'|'vegalite') -> string`
- `version() -> string`

**Format precedence:** `render` argument > `.format()` setter > default `'svg'`. `render(undefined)`
falls back; `render(null)` / `render(false)` / unknown formats throw `FulgurParseError`.

### Options

| Field | Type | Default | Notes |
|---|---|---|---|
| `width` / `height` | `number` | spec value | 1–32768 px |
| `scale` | `number` | `1.0` | raster scale; ignored for svg |
| `strict` | `boolean` | `false` | reject unknown keys |
| `dsl` | `'chartjs' \| 'vegalite'` | auto-detect | `mark` -> vegalite, `type` -> chartjs |
| `font` | `Buffer \| Uint8Array` | bundled Noto Sans JP | TTF/OTF bytes |

### Errors

| Class | When |
|---|---|
| `FulgurParseError` | invalid JSON, parse failure, unknown DSL/format, dimension limit |
| `FulgurStrictError` (extends `FulgurParseError`) | strict-mode unknown key |
| `FulgurRenderError` | raster conversion / IO failure |

Font-error asymmetry (faithful to the render path): an invalid font throws `FulgurParseError`
on the svg path and `FulgurRenderError` on the png path.

## Determinism

Same `specJson` + same `format` + same `options` -> identical output bytes. SVG and PNG never
match byte-for-byte (different render paths). See `docs/binding-api-contract.md` §4.

## npm Package Distribution

Publishing is fully automated: when a `fulgur-chart-v*` GitHub Release is published, the
`node-npm-release.yml` workflow triggers and publishes 7 npm packages:

- `@fulgur-rs/chart-node` — loader + JS wrapper (main package)
- `@fulgur-rs/chart-node-linux-x64-gnu` — Linux x64 (glibc)
- `@fulgur-rs/chart-node-linux-x64-musl` — Linux x64 (musl)
- `@fulgur-rs/chart-node-linux-arm64-gnu` — Linux arm64 (glibc)
- `@fulgur-rs/chart-node-darwin-arm64` — macOS arm64
- `@fulgur-rs/chart-node-darwin-x64` — macOS x64
- `@fulgur-rs/chart-node-win32-x64-msvc` — Windows x64

### First-time setup (once only)

Register a Trusted Publisher on npm for all 7 packages above:

- **Owner**: `fulgur-rs`
- **Repo**: `fulgur-chart`
- **Workflow**: `node-npm-release.yml`
- **Environment**: `npm`
