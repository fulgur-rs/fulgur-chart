import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

import init, {
  build,
  render,
  schema,
  version,
  FulgurParseError,
  FulgurStrictError,
  FulgurRenderError,
} from '../index.js'
import { BAR, LINE, VEGALITE_BAR, PNG_MAGIC } from './fixtures.mjs'

// --target web: instantiate once before any call. Node has no file fetch, so pass bytes
// via the object form (positional init(bytes) is deprecated by the current glue).
const wasmUrl = new URL('../pkg/fulgur_chart_wasm_bg.wasm', import.meta.url)
await init({ module_or_path: await readFile(fileURLToPath(wasmUrl)) })

const isU8 = (v) => v instanceof Uint8Array
const bytesEqual = (a, b) =>
  isU8(a) && isU8(b) && a.length === b.length && Buffer.compare(Buffer.from(a), Buffer.from(b)) === 0
const startsWithPngMagic = (b) => isU8(b) && b.length >= 4 && bytesEqual(b.subarray(0, 4), PNG_MAGIC)

// --- meta ---

test('version() returns a semver string', () => {
  assert.equal(typeof version(), 'string')
  assert.match(version(), /^\d+\.\d+\.\d+/)
})

// --- rendering ---

test("build(spec).render('svg') returns an SVG string", () => {
  const out = build(BAR).render('svg')
  assert.equal(typeof out, 'string')
  assert.ok(out.startsWith('<svg'), `expected <svg, got ${out.slice(0, 20)}`)
})

test("build(spec).render('png') returns a PNG Uint8Array", () => {
  const out = build(BAR).render('png')
  assert.ok(isU8(out), 'expected a Uint8Array')
  assert.ok(startsWithPngMagic(out), 'expected PNG magic bytes')
})

test('vegalite is auto-detected', () => {
  assert.ok(build(VEGALITE_BAR).render('svg').startsWith('<svg'))
})

// --- format precedence: argument > .format() setter > default 'svg' ---

test("default format is 'svg'", () => {
  assert.ok(build(BAR).render().startsWith('<svg'))
})

test('.format() setter is used when no argument', () => {
  assert.ok(startsWithPngMagic(build(BAR).format('png').render()))
})

test('render argument overrides .format() setter', () => {
  const out = build(BAR).format('png').render('svg')
  assert.ok(out.startsWith('<svg'), "render('svg') must win over .format('png')")
})

test('undefined argument falls back to setter or default', () => {
  assert.ok(build(BAR).render(undefined).startsWith('<svg'))
  assert.ok(startsWithPngMagic(build(BAR).format('png').render(undefined)))
})

test('explicit null/false format is invalid, not silently rendered', () => {
  assert.throws(() => build(BAR).render(null), FulgurParseError)
  assert.throws(() => build(BAR).render(false), FulgurParseError)
  assert.throws(() => build(BAR).format('png').render(null), FulgurParseError)
})

test('an explicitly stored format(null) is forwarded, not defaulted to svg', () => {
  assert.throws(() => build(BAR).format(null).render(), FulgurParseError)
})

// --- chainable setters: width/height/scale/dsl/strict ---

test('width/height override', () => {
  const big = build(BAR).width(1234).height(567).render('svg')
  assert.ok(big.includes('width="1234"'))
  assert.ok(big.includes('height="567"'))
})

test('scale changes the png output', () => {
  const a = build(BAR).scale(1.0).render('png')
  const b = build(BAR).scale(2.0).render('png')
  assert.ok(!bytesEqual(a, b), 'scale should change the rasterized output')
})

test('dsl override switches the parser', () => {
  assert.ok(build(VEGALITE_BAR).render('svg').startsWith('<svg'))
  assert.throws(() => build(VEGALITE_BAR).dsl('chartjs').render('svg'), FulgurParseError)
})

// --- builder is reusable; setters chain; renders are deterministic ---

test('setters return this for chaining', () => {
  const b = build(BAR)
  assert.equal(b.width(800), b)
  assert.equal(b.strict(false), b)
})

test('builder reuse is deterministic', () => {
  const b = build(BAR)
  assert.equal(b.render('svg'), b.render('svg'))
  assert.ok(bytesEqual(b.render('png'), b.render('png')))
})

test('builder is reconfigurable between renders', () => {
  const b = build(BAR)
  const small = b.width(400).render('svg')
  const big = b.width(1234).render('svg')
  assert.ok(small.includes('width="400"'))
  assert.ok(big.includes('width="1234"'))
})

// --- errors (call-site classification preserved) ---

test('unknown format raises ParseError', () => {
  assert.throws(() => build(BAR).render('zzz'), FulgurParseError)
})

test('invalid JSON raises ParseError', () => {
  assert.throws(() => build('not json').render('svg'), FulgurParseError)
})

test('undetectable DSL raises ParseError', () => {
  assert.throws(() => build('{"labels":[]}').render('svg'), FulgurParseError)
})

test('unknown dsl raises ParseError', () => {
  assert.throws(() => build(BAR).dsl('zzz').render('svg'), FulgurParseError)
})

test('strict mode unknown key raises StrictError', () => {
  const spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
  assert.throws(() => build(spec).strict().render('svg'), FulgurStrictError)
})

test('StrictError is a ParseError subclass', () => {
  const spec = '{"type":"bar","data":{"labels":[],"datasets":[]},"bogusKey":1}'
  assert.throws(() => build(spec).strict().render('svg'), FulgurParseError)
  assert.ok(FulgurStrictError.prototype instanceof FulgurParseError)
})

test('dimension over the limit raises ParseError', () => {
  assert.throws(() => build(BAR).width(40000).render('svg'), FulgurParseError)
})

// font-error asymmetry: SVG path -> ParseError, image path -> RenderError
test('invalid font on the svg path raises ParseError', () => {
  assert.throws(() => build(BAR).font(Uint8Array.of(1, 2, 3, 4)).render('svg'), FulgurParseError)
})

test('invalid font on the image path raises RenderError', () => {
  assert.throws(() => build(BAR).font(Uint8Array.of(1, 2, 3, 4)).render('png'), FulgurRenderError)
})

// --- low-level render primitive (the builder calls it) ---

test('direct render primitive', () => {
  assert.ok(render(BAR, 'svg').startsWith('<svg'))
  assert.ok(startsWithPngMagic(render(BAR, 'png')))
  assert.ok(render(BAR, 'svg', { width: 800 }).includes('width="800"'))
})

test('direct render equals builder render', () => {
  assert.ok(bytesEqual(render(BAR, 'png', { width: 640 }), build(BAR).width(640).render('png')))
  assert.equal(render(LINE, 'svg'), build(LINE).render('svg'))
})

// --- schema / version meta functions ---

test('schema(dsl) returns JSON schema strings', () => {
  assert.ok(JSON.parse(schema('chartjs')))
  assert.ok(JSON.parse(schema('vegalite')))
})

test('schema unknown dsl raises ParseError', () => {
  assert.throws(() => schema('zzz'), FulgurParseError)
})

test('schema with a non-string dsl raises ParseError (never a raw wasm error)', () => {
  assert.throws(() => schema(null), FulgurParseError)
  assert.throws(() => schema(false), FulgurParseError)
})

// --- public surface lock ---

test('public surface is exactly the documented exports', async () => {
  // index.js is ESM; introspect via dynamic import. The default export (init) is part of
  // the public surface, so the locked key set is the 7 named exports + 'default'.
  const pkg = await import('../index.js')
  assert.deepEqual(
    Object.keys(pkg).sort(),
    [
      'default', // init
      'FulgurParseError',
      'FulgurRenderError',
      'FulgurStrictError',
      'build',
      'render',
      'schema',
      'version',
    ].sort(),
  )
})
