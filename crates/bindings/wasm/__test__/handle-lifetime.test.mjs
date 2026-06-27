// Regression test for the wasm-specific handle-lifetime contract (no napi counterpart).
//
// `nativeRender`/`nativeSchema` return wasm-bindgen class instances — handles into wasm
// linear memory, NOT plain objects. The wrapper (index.js) copies the values out via getters
// and then releases the handle with `r.free()` in a `finally`. Without that explicit free the
// handle lingers until GC eventually runs the FinalizationRegistry, piling up wasm-side
// allocations in a render loop. These tests pin the explicit-free contract deterministically,
// WITHOUT relying on GC / FinalizationRegistry timing.
//
// Mechanism: spy on `free()` via the class prototype. The wrapper creates the handles
// internally (we never see them), but test and wrapper both resolve pkg/fulgur_chart_wasm.js
// to the same ESM singleton, so `RenderResult`/`SchemaResult` here ARE the classes the wrapper
// instantiates — patching their prototype intercepts the wrapper's `r.free()`. Each spy
// delegates to the original free, so the handle is still released (the test introduces no leak
// of its own). The free count alone is the complete signal: 0 = leak (free omitted), 2 =
// double-free; only exactly-once passes.
//
// Negative control (run manually during development): removing `r.free()` from index.js makes
// every assertion below fail with count 0.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

import init, { build, render, schema, FulgurParseError } from '../index.js'
import { RenderResult, SchemaResult } from '../pkg/fulgur_chart_wasm.js'
import { BAR } from './fixtures.mjs'

// --target web: instantiate once before any call (same object form the other tests use).
const wasmUrl = new URL('../pkg/fulgur_chart_wasm_bg.wasm', import.meta.url)
await init({ module_or_path: await readFile(fileURLToPath(wasmUrl)) })

// Install a counting spy on a handle class's `free`, delegating to the original so the wasm
// allocation is still released. Returns a mutable counter reset before each measured block.
function installFreeSpy(klass) {
  const original = klass.prototype.free
  const state = { count: 0 }
  klass.prototype.free = function free() {
    state.count += 1
    return original.call(this)
  }
  return state
}

const renderFrees = installFreeSpy(RenderResult)
const schemaFrees = installFreeSpy(SchemaResult)

// --- render handle ---

test('render() frees its result handle exactly once', () => {
  renderFrees.count = 0
  build(BAR).render('svg')
  assert.equal(renderFrees.count, 1, 'one render must free its handle exactly once (no leak, no double-free)')
})

test('a render loop frees one handle per iteration (no accumulation)', () => {
  renderFrees.count = 0
  const N = 50
  for (let i = 0; i < N; i++) build(BAR).render('svg')
  assert.equal(renderFrees.count, N, 'each iteration must release its own handle')
})

test('render() frees the handle even when the body throws (finally path)', () => {
  // The realistic future regression: someone moves free() onto the success path. The handle is
  // created before `if (!r.ok) throw`, so the `finally` must still release it on error.
  renderFrees.count = 0
  assert.throws(() => render('not json', 'svg'), FulgurParseError)
  assert.equal(renderFrees.count, 1, 'the error path must still free the handle')
})

// --- schema handle (same never-throw + explicit-free convention) ---

test('schema() frees its result handle exactly once', () => {
  schemaFrees.count = 0
  schema('chartjs')
  assert.equal(schemaFrees.count, 1, 'schema must free its handle exactly once')
})

test('schema() frees the handle even when the body throws (finally path)', () => {
  schemaFrees.count = 0
  assert.throws(() => schema('zzz'), FulgurParseError)
  assert.equal(schemaFrees.count, 1, 'the error path must still free the schema handle')
})
