// Type-level checks for index.d.ts (compiled with `tsc --noEmit`, never executed).
import {
  build,
  render,
  schema,
  version,
  FulgurParseError,
  FulgurStrictError,
  FulgurRenderError,
} from '../index.js'

const SPEC = '{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}'

// Builder return-type overloads.
const a: string = build(SPEC).width(800).height(600).dsl('chartjs').strict().render('svg')
const b: Buffer = build(SPEC).scale(2).format('png').render('png')
// No-argument render() depends on the .format() state, so it is typed string | Buffer.
const c: string | Buffer = build(SPEC).render()
const cPng: Buffer = build(SPEC).format('png').render('png') // explicit png stays precise

// Low-level primitive overloads.
const d: string = render(SPEC, 'svg', { width: 800 })
const e: Buffer = render(SPEC, 'png')

// Meta.
const f: string = schema('chartjs')
const g: string = version()

// Error classes are Errors; StrictError is assignable to ParseError.
const h: Error = new FulgurRenderError('x')
const i: FulgurParseError = new FulgurStrictError('x')

// @ts-expect-error png returns Buffer, not string
const wrong1: string = build(SPEC).render('png')
// @ts-expect-error unknown dsl is rejected
build(SPEC).dsl('zzz')
// @ts-expect-error unknown format is rejected
render(SPEC, 'jpeg')

void [a, b, c, cPng, d, e, f, g, h, i, wrong1]
