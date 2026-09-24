import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

import init, { build, render, FulgurParseError } from '../index.js'
import { BAR } from './fixtures.mjs'

const wasmUrl = new URL('../pkg/fulgur_chart_wasm_bg.wasm', import.meta.url)
await init({ module_or_path: await readFile(fileURLToPath(wasmUrl)) })

const fontUrl = new URL('../../../fulgur-chart/assets/fonts/NotoSansJP-Regular.otf', import.meta.url)
const font = new Uint8Array(await readFile(fileURLToPath(fontUrl)))
const temporalLineUrl = new URL(
  '../../../fulgur-chart/tests/fixtures/vegalite-temporal-line.json',
  import.meta.url,
)
const temporalLine = await readFile(fileURLToPath(temporalLineUrl), 'utf8')

test('no-default-font build requires an explicit font for every format', () => {
  for (const format of ['svg', 'png', 'webp']) {
    assert.throws(() => build(BAR).render(format), FulgurParseError, `builder ${format}`)
    assert.throws(() => render(BAR, format), FulgurParseError, `render ${format}`)
  }
})

test('no-default-font build renders every format with an explicit font', () => {
  const builder = build(BAR).font(font)
  const svg = builder.render('svg')
  const png = builder.render('png')
  const webp = builder.render('webp')

  assert.ok(svg.startsWith('<svg'))
  assert.equal(String.fromCharCode(...png.subarray(0, 4)), '\x89PNG')
  assert.equal(String.fromCharCode(...webp.subarray(0, 4)), 'RIFF')
  assert.equal(String.fromCharCode(...webp.subarray(8, 12)), 'WEBP')

  assert.ok(render(BAR, 'svg', { font }).startsWith('<svg'))
  assert.equal(String.fromCharCode(...render(BAR, 'png', { font }).subarray(0, 4)), '\x89PNG')
  const directWebp = render(BAR, 'webp', { font })
  assert.equal(String.fromCharCode(...directWebp.subarray(0, 4)), 'RIFF')
  assert.equal(String.fromCharCode(...directWebp.subarray(8, 12)), 'WEBP')
})

test('no-default-font build validates temporal line charts with the supplied font', () => {
  assert.throws(() => build(temporalLine).render('svg'), FulgurParseError)
  assert.ok(build(temporalLine).font(font).render('svg').startsWith('<svg'))
})
