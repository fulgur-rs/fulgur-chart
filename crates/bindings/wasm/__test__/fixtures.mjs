// Shared spec fixtures (mirrors the Node binding's fixtures.mjs).
export const BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
export const LINE = '{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,3,2]}]}}'
export const VEGALITE_BAR =
  '{"mark":"bar","data":{"values":[{"a":"x","b":1}]},"encoding":{"x":{"field":"a"},"y":{"field":"b"}}}'
// PNG magic \x89PNG as a plain Uint8Array (wasm returns Uint8Array, not Buffer).
export const PNG_MAGIC = Uint8Array.of(0x89, 0x50, 0x4e, 0x47)
