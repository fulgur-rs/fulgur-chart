import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'

import {
  build,
  render,
  schema,
  version,
  FulgurParseError,
  FulgurStrictError,
  FulgurRenderError,
} from '../index.js'
import { BAR, LINE, VEGALITE_BAR, PNG_MAGIC } from './fixtures.mjs'

const startsWithPngMagic = (buf) => Buffer.isBuffer(buf) && buf.subarray(0, 4).equals(PNG_MAGIC)

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

test("build(spec).render('png') returns a PNG Buffer", () => {
  const out = build(BAR).render('png')
  assert.ok(Buffer.isBuffer(out), 'expected a Buffer')
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
  // A non-undefined falsy format must NOT fall back; it is an invalid format.
  assert.throws(() => build(BAR).render(null), FulgurParseError)
  assert.throws(() => build(BAR).render(false), FulgurParseError)
  assert.throws(() => build(BAR).format('png').render(null), FulgurParseError)
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
  assert.ok(!a.equals(b), 'scale should change the rasterized output')
})

test('dsl override switches the parser', () => {
  // VEGALITE_BAR auto-detects vegalite; forcing chartjs must actually switch the parser
  // (the vegalite spec is invalid chartjs) -> ParseError.
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
  assert.ok(b.render('png').equals(b.render('png')))
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
  assert.throws(() => build(BAR).font(Buffer.from('not a font')).render('svg'), FulgurParseError)
})

test('invalid font on the image path raises RenderError', () => {
  assert.throws(() => build(BAR).font(Buffer.from('not a font')).render('png'), FulgurRenderError)
})

// font accepts a plain Uint8Array (not only Buffer): the napi boundary converts it, so the
// bytes reach the Rust font parser and an invalid font surfaces as FulgurParseError (svg path)
// rather than a raw napi conversion error. Locks the `font: Buffer | Uint8Array` type claim.
test('font accepts a plain Uint8Array at the native boundary', () => {
  const bad = new Uint8Array([1, 2, 3, 4])
  assert.equal(Buffer.isBuffer(bad), false)
  assert.throws(() => build(BAR).font(bad).render('svg'), FulgurParseError)
})

// --- low-level render primitive (the builder calls it) ---

test('direct render primitive', () => {
  assert.ok(render(BAR, 'svg').startsWith('<svg'))
  assert.ok(startsWithPngMagic(render(BAR, 'png')))
  assert.ok(render(BAR, 'svg', { width: 800 }).includes('width="800"'))
})

test('direct render equals builder render', () => {
  assert.ok(render(BAR, 'png', { width: 640 }).equals(build(BAR).width(640).render('png')))
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

// --- public surface lock ---

test('public surface is exactly the documented exports', () => {
  const require = createRequire(import.meta.url)
  const pkg = require('../index.js')
  assert.deepEqual(
    Object.keys(pkg).sort(),
    [
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
