// Public type definitions for the fulgur-chart Node.js binding (builder API).
// Hand-written: the generated `binding.d.ts` only types the low-level native primitive.

/// <reference types="node" />

export type Dsl = 'chartjs' | 'vegalite'
export type Format = 'svg' | 'png' | 'webp'

/** Render options. All fields optional; omitted fields use the spec / core defaults. */
export interface RenderOptions {
  /** Chart width (px). Overrides the spec value. */
  width?: number
  /** Chart height (px). Overrides the spec value. */
  height?: number
  /** Raster scale factor. Ignored when rendering SVG. Default 1.0. */
  scale?: number
  /** Reject unknown keys (strict mode). Default false. */
  strict?: boolean
  /** Force the input DSL. Omit to auto-detect (`mark` -> vegalite, `type` -> chartjs). */
  dsl?: Dsl
  /** TrueType/OpenType font bytes. Omit to use the bundled Noto Sans JP. */
  font?: Buffer | Uint8Array
}

/** Input/parse failure: invalid JSON, parse error, unknown DSL/format, dimension limit. */
export declare class FulgurParseError extends Error {}
/** Strict-mode unknown-key violation. A subclass of {@link FulgurParseError}. */
export declare class FulgurStrictError extends FulgurParseError {}
/** Raster conversion / IO failure. */
export declare class FulgurRenderError extends Error {}

/**
 * Fluent, reusable builder. Setters mutate and return `this`; `render` may be called
 * multiple times and the builder may be reconfigured between calls.
 *
 * This is a type-only interface (the runtime value is constructed via {@link build}, never
 * exported). Reference it as a type; it is not importable as a value.
 */
export interface Builder {
  width(value: number): this
  height(value: number): this
  scale(value: number): this
  dsl(value: Dsl): this
  font(bytes: Buffer | Uint8Array): this
  strict(value?: boolean): this
  format(value: Format): this
  /**
   * Render to the given format. Precedence: explicit argument > `.format()` setter >
   * default `'svg'`. An explicit `'svg'` returns a string and `'png'` a Buffer; a
   * no-argument call depends on the `.format()` state, so it is typed `string | Buffer`.
   */
  render(format: 'svg'): string
  render(format: 'png'): Buffer
  render(format: 'webp'): Buffer
  render(format?: Format): string | Buffer
}

/** Start a builder for the given chart.js v4 / Vega-Lite DSL JSON string. */
export declare function build(specJson: string): Builder

/**
 * Low-level render primitive (the builder calls this; also callable directly).
 * Unknown format -> {@link FulgurParseError}.
 */
export declare function render(specJson: string, format: 'svg', options?: RenderOptions): string
export declare function render(specJson: string, format: 'png', options?: RenderOptions): Buffer
export declare function render(specJson: string, format: 'webp', options?: RenderOptions): Buffer
export declare function render(
  specJson: string,
  format: Format,
  options?: RenderOptions,
): string | Buffer

/** Return the JSON Schema (as a JSON string) for the given DSL. Unknown DSL -> ParseError. */
export declare function schema(dsl: Dsl): string

/** Return the crate version string. */
export declare function version(): string
