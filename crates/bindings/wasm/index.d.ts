// Public type definitions for the fulgur-chart WASM binding (builder API).
// Hand-written: the generated pkg/*.d.ts only types the low-level native primitive.

export type Dsl = 'chartjs' | 'vegalite'
export type Format = 'svg' | 'png'

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
  font?: Uint8Array
}

/** Accepted wasm source for {@link init}. Kept to types available without the DOM lib. */
export type InitInput = Uint8Array | ArrayBuffer

/**
 * Instantiate the WebAssembly module. MUST be awaited once before any other call.
 * Browser: `await init()` (fetches the bundled .wasm). Node (no file fetch): pass the
 * bytes via the object form: `await init({ module_or_path: bytes })`.
 * Re-exported from the wasm-pack generated glue (`--target web`).
 */
export default function init(
  options?: { module_or_path: InitInput } | InitInput,
): Promise<unknown>

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
 * Type-only interface (the runtime value is constructed via {@link build}, never exported).
 */
export interface Builder {
  width(value: number): this
  height(value: number): this
  scale(value: number): this
  dsl(value: Dsl): this
  font(bytes: Uint8Array): this
  strict(value?: boolean): this
  format(value: Format): this
  /**
   * Render to the given format. Precedence: explicit argument > `.format()` setter >
   * default `'svg'`. `'svg'` returns a string and `'png'` a Uint8Array; a no-argument
   * call depends on the `.format()` state, so it is typed `string | Uint8Array`.
   */
  render(format: 'svg'): string
  render(format: 'png'): Uint8Array
  render(format?: Format): string | Uint8Array
}

/** Start a builder for the given chart.js v4 / Vega-Lite DSL JSON string. */
export declare function build(specJson: string): Builder

/**
 * Low-level render primitive (the builder calls this; also callable directly).
 * Unknown format -> {@link FulgurParseError}.
 */
export declare function render(specJson: string, format: 'svg', options?: RenderOptions): string
export declare function render(
  specJson: string,
  format: 'png',
  options?: RenderOptions,
): Uint8Array
export declare function render(
  specJson: string,
  format: Format,
  options?: RenderOptions,
): string | Uint8Array

/** Return the JSON Schema (as a JSON string) for the given DSL. Unknown DSL -> ParseError. */
export declare function schema(dsl: Dsl): string

/** Return the crate version string. */
export declare function version(): string
