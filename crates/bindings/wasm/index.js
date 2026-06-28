// Hand-written public wrapper (ESM). The native primitive lives in the wasm-pack
// generated glue (./pkg/fulgur_chart_wasm.js). This layer adds the builder API and the
// error-class hierarchy, mirroring the Node binding (pure-JS builder over a single
// native `render` primitive).
//
// `--target web`: the wasm must be instantiated once via the default-exported `init`
// before any call. In a browser `await init()` fetches the bundled .wasm; in Node (no
// file fetch) pass the bytes via the object form:
// `await init({ module_or_path: await readFile(wasmUrl) })`.
// (The current wasm-bindgen glue uses the object form; positional `init(bytes)` still
// works but logs a deprecation warning.)
import init, {
  render as nativeRender,
  schema as nativeSchema,
  version as nativeVersion,
} from './pkg/fulgur_chart_wasm.js'

export default init

// --- error hierarchy (mirrors Node/Ruby/Python: StrictError is a ParseError subclass) ---

export class FulgurParseError extends Error {
  constructor(message) {
    super(message)
    this.name = 'FulgurParseError'
  }
}

export class FulgurStrictError extends FulgurParseError {
  constructor(message) {
    super(message)
    this.name = 'FulgurStrictError'
  }
}

export class FulgurRenderError extends Error {
  constructor(message) {
    super(message)
    this.name = 'FulgurRenderError'
  }
}

// Map the native discriminant `code` -> error class (mechanical; no message parsing).
function makeError(code, message) {
  switch (code) {
    case 'STRICT_ERROR':
      return new FulgurStrictError(message)
    case 'RENDER_ERROR':
      return new FulgurRenderError(message)
    default: // 'PARSE_ERROR'
      return new FulgurParseError(message)
  }
}

// --- low-level render primitive (the builder calls this; also callable directly) ---

export function render(specJson, format, options) {
  const o = options ?? {}
  // Coerce the format to a string before crossing the wasm boundary (mirrors Node's
  // String(format)): non-string values like null/false become "null"/"false", which the
  // native layer rejects as an unsupported format (ParseError). The Builder resolves
  // `undefined` to a fallback first. Options are unpacked positionally.
  const r = nativeRender(
    specJson,
    String(format),
    o.width,
    o.height,
    o.scale,
    o.strict,
    o.dsl,
    o.font,
  )
  // `r` is a wasm-bindgen class instance (a handle into wasm linear memory), NOT a plain
  // object like napi's result. The getters copy svg/png out into JS-owned values, after
  // which the handle must be `free()`d — otherwise it lingers until GC runs the
  // FinalizationRegistry, piling up wasm-side allocations in a render loop.
  try {
    if (!r.ok) {
      throw makeError(r.code, r.message)
    }
    // Read each field out of wasm memory once (a getter clones; the unused ones are
    // undefined). Exactly one of svg/png/webp is set on success; png/webp are Uint8Arrays.
    const svg = r.svg
    const png = r.png
    const webp = r.webp
    return svg != null ? svg : (png != null ? png : webp)
  } finally {
    r.free()
  }
}

// --- fluent, reusable builder (setters mutate and return `this`) ---

class Builder {
  constructor(specJson) {
    this._spec = specJson
    this._opts = {}
  }

  width(value) {
    this._opts.width = value
    return this
  }

  height(value) {
    this._opts.height = value
    return this
  }

  scale(value) {
    this._opts.scale = value
    return this
  }

  dsl(value) {
    this._opts.dsl = value
    return this
  }

  font(bytes) {
    this._opts.font = bytes
    return this
  }

  strict(value = true) {
    this._opts.strict = value
    return this
  }

  format(value) {
    this._opts.format = value
    return this
  }

  // Format precedence: explicit argument > .format() setter > default 'svg'.
  // Presence is tested with `in` (not `??`) so `.format(null).render()` matches
  // `render(null)` (an explicit invalid value -> ParseError) rather than rendering svg.
  render(format) {
    const resolved =
      format !== undefined ? format : 'format' in this._opts ? this._opts.format : 'svg'
    const { format: _ignored, ...rest } = this._opts
    return render(this._spec, resolved, rest)
  }
}

export function build(specJson) {
  return new Builder(specJson)
}

export function schema(dsl) {
  // Coerce to a string before the wasm boundary (like render): non-string values become
  // e.g. "null" and are rejected as an unsupported DSL (FulgurParseError) instead of a raw
  // wasm-bindgen conversion error.
  const r = nativeSchema(String(dsl))
  try {
    if (!r.ok) {
      throw makeError(r.code, r.message)
    }
    return r.value
  } finally {
    r.free() // wasm-bindgen handle; free after copying `value` out (see render()).
  }
}

export function version() {
  return nativeVersion()
}
