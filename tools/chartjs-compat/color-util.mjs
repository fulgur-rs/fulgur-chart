//! 色まわりの最小ピュアヘルパ(leaf module)。chart.js や node-canvas のような
//! 重い依存を持たないため、extract.mjs(canvas 依存)と crosscheck.mjs(依存なし)の
//! 双方から安全に import できる。これにより fmtAlpha の実装が JS 内で 1 つに集約され、
//! クロス言語フィクスチャ(rgba-fixture)が全 JS 呼び出し元の共有コピーを pin する。

/// alpha を正規化整形する(>=1→"1", <=0→"0", それ以外は 3 桁丸め・末尾ゼロ除去)。
/// String(...) が末尾ゼロを自動で落とすため、Rust 側の f64 Display と一致する。
export function fmtAlpha(a) {
  if (a >= 1) return '1';
  if (a <= 0) return '0';
  return String(Math.round(a * 1000) / 1000);
}
