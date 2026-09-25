# @fulgur-rs/chart-wasm

Deterministic chart.js v4 / Vega-Lite JSON → SVG/PNG renderer, compiled to WebAssembly.

Built with `wasm-pack --target web`: **you must `await init()` once before any call.**

## Browser

```js
import init, { build } from '@fulgur-rs/chart-wasm'

await init() // fetches the bundled .wasm once
const spec = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
const svg = build(spec).width(800).render('svg') // string
const png = build(spec).render('png') // Uint8Array
```

## Node.js

Node has no file `fetch`, so pass the wasm bytes to `init`:

```js
import init, { build } from '@fulgur-rs/chart-wasm'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const wasmUrl = new URL(
  '../node_modules/@fulgur-rs/chart-wasm/pkg/fulgur_chart_wasm_bg.wasm',
  import.meta.url,
)
await init({ module_or_path: await readFile(fileURLToPath(wasmUrl)) })

const svg = build('{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}').render('svg')
```

## API

- `build(specJson)` → Builder: `.width/.height/.scale/.dsl/.font/.strict/.format` (chainable) → `.render('svg')` (string) / `.render('png')` (Uint8Array)
- `render(specJson, format, options?)` — low-level primitive
- `schema('chartjs' | 'vegalite')` → JSON Schema string
- `version()` → core version string

Errors: `FulgurParseError`, `FulgurStrictError` (`< FulgurParseError`), `FulgurRenderError`.

Behavior (DSL auto-detection, options, error classification, determinism, font asymmetry)
follows `docs/binding-api-contract.md`.

## Slim build without the bundled font

The default build includes Noto Sans JP. To build a smaller WASM binary for applications
that supply their own TrueType/OpenType font, run:

```sh
npm run build:no-default-font
```

In this build, pass `font` to each low-level `render(...)` call or set it on a builder;
omitting it raises `FulgurParseError`:

```js
const svg = build(spec).font(fontBytes).render('svg')
```

Run `npm run test:no-default-font` to build this variant, verify that the generated WASM
does not contain the bundled font bytes, and check that explicit fonts work for SVG, PNG,
and WebP. It also runs the full core unit suite without `default-font`; tests use a separate
test-only font fixture where font bytes are needed.

## npm Package Distribution

Releases are triggered automatically when a `fulgur-chart-v*` GitHub Release is published,
running `node-npm-release.yml` to build and publish `@fulgur-rs/chart-wasm`.

### One-time setup

Register a Trusted Publisher on npm for `@fulgur-rs/chart-wasm`:
- **Owner**: `fulgur-rs`
- **Repo**: `fulgur-chart`
- **Workflow**: `node-npm-release.yml`
- **Environment**: `npm`
