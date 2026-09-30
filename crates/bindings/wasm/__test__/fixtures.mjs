// Shared spec fixtures (mirrors the Node binding's fixtures.mjs).
export const BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
export const LINE = '{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,3,2]}]}}'
export const VEGALITE_BAR =
  '{"mark":"bar","data":{"values":[{"a":"x","b":1}]},"encoding":{"x":{"field":"a"},"y":{"field":"b"}}}'
export const VEGALITE_RULE_RANGE =
  '{"mark":{"type":"rule","color":"red"},"data":{"values":[{"start":1,"end":3}]},"encoding":{"x":{"field":"start","type":"quantitative"},"x2":{"field":"end"}}}'
export const VEGALITE_TICK =
  '{"mark":{"type":"tick","orient":"horizontal","color":"red","size":8},"data":{"values":[{"x":1,"group":"A"},{"x":4,"group":"B"}]},"encoding":{"x":{"field":"x","type":"quantitative"},"y":{"field":"group","type":"nominal"}}}'
// PNG magic \x89PNG as a plain Uint8Array (wasm returns Uint8Array, not Buffer).
export const PNG_MAGIC = Uint8Array.of(0x89, 0x50, 0x4e, 0x47)
