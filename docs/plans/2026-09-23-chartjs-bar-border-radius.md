# Chart.js dataset-level bar borderRadius

Beads: `fulgur-chart-7ro`

## Goal

Chart.js の bar dataset ごとに `borderRadius` を指定でき、SVG と PNG の両方で角丸棒を描けるようにする。

## Accepted scope

- `borderRadius` は Chart.js の dataset option として `number` または `{topLeft, topRight, bottomLeft, bottomRight}` を受け付ける。
- bar chart の `BarDataset` と、mixed chart 用 `LineDataset` 内で `type: "bar"` を指定した dataset が対象。line dataset の描画には適用しない。
- 既定の `borderSkipped: "start"` を使う。基点に接する角は丸めない。垂直棒・水平棒とも値の符号に合わせて基点側を選ぶ。
- 数値指定は Chart.js に合わせて単独棒の値終端側を丸める。値積み上げ時は、正負それぞれの stack の外端にある segment だけを丸める。
- 角別 object は指定値を使い、未指定角は 0 とする。既定の基点側を飛ばす動作は適用する。
- 半径は有限な非負値に整え、各角の値を `min(width, height) / 2` までに制限する。全角の半径が 0 の場合は既存の矩形を出力する。
- `borderRadius` 単独指定は bar の幅・位置計算を変更しない。
- 角丸は既存の `Prim::Path` で表す。座標は `fmt_num` を通し、PNG path parser が要求する空白区切りの `M/L/C/Z` コマンドにする。
- scriptable/indexable、hover 半径、`borderSkipped` の個別指定、chart-wide defaults は対象外。

## Acceptance criteria

1. 公開 Chart.js schema と parser が数値・角別 object の両方を受け入れ、strict parser も `borderRadius` を既知の dataset key として扱う。
2. 正負の垂直棒で値終端側の角を丸める。水平棒では対応する右端・左端の角を丸める。
3. 積み上げ棒の数値指定では正負それぞれの stack の外端だけが丸まり、角別 object は明示した角を使う。
4. 半径 0、負値、大きな値、片側だけの corner object で有限かつ範囲内の path を生成する。
5. `borderRadius` 未指定の scene は既存の矩形を維持する。角丸 scene は SVG と PNG の両方に出力できる。
6. `borderRadius` だけを加えた場合、`BarBox` の位置と寸法は指定なしの chart と一致する。

## Reference

- [Chart.js Bar chart dataset properties and borderRadius behavior](https://www.chartjs.org/docs/latest/charts/bar.html#borderradius)
- [Chart.js BarElement path generation](https://github.com/chartjs/Chart.js/blob/master/src/elements/element.bar.js)
