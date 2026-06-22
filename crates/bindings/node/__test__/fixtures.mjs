// Shared spec fixtures (mirrors the Ruby binding's test_helper.rb).
export const BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'
export const LINE = '{"type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,3,2]}]}}'
export const VEGALITE_BAR =
  '{"mark":"bar","data":{"values":[{"a":"x","b":1}]},"encoding":{"x":{"field":"a"},"y":{"field":"b"}}}'
export const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47]) // \x89PNG
