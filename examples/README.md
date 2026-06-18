# examples

fulgur-chart のサンプル spec と、それを実 CLI で描画した SVG、Fulgur に通す想定の
レポート HTML を置いています。対応するチャート種と主な機能を一通りカバーしています。

## ディレクトリ

- `specs/` … 入力 spec（JSON）。既定は chart.js v4 互換、`vegalite.json` のみ Vega-Lite。
  - チャート種:
    - `bar.json` … 棒グラフ（縦・月次売上）
    - `bar-horizontal.json` … 横棒グラフ（`indexAxis: "y"`）
    - `stacked-bar.json` … 積み上げ棒（`scales.y.stacked`）
    - `line.json` … 折れ線（2 系列・`tension` でなめらかに）
    - `area.json` … エリアチャート（`"fill": true` の line）
    - `pie.json` … 円グラフ（スライスごとに自動配色）
    - `doughnut.json` … ドーナツグラフ（凡例は右）
    - `scatter.json` … 散布図（`{x, y}` 点データ・2 系列）
    - `bubble.json` … バブルチャート（`{x, y, r}` で第3次元を半径に）
    - `radar.json` … レーダーチャート（多変量・2 系列）
    - `mixed.json` … 混合チャート（棒 + 折れ線・dataset 別 `type`）
  - 機能:
    - `datalabels.json` … データラベル（`plugins.datalabels.display`）
    - `theme.json` … テーマ上書き（`options.theme` でダーク配色）
    - `vegalite.json` … Vega-Lite サブセット入力（`--dsl vegalite`）
- `out/` … 上記 spec を CLI で描画した SVG（リポジトリに同梱）
- `report.html` … 生成 SVG を `<img>` で並べた最小ギャラリー

## SVG の生成（再生成手順）

リポジトリのルートから、各 spec を CLI に通して `out/` を再生成します。出力は決定的
（同一入力なら byte-identical）なので、再生成しても差分は出ません。

chart.js 系（`vegalite` 以外）はまとめて生成できます。

```sh
for n in bar bar-horizontal stacked-bar line area pie doughnut \
         scatter bubble radar mixed datalabels theme; do
  cargo run -q -p fulgur-chart-cli -- render "examples/specs/$n.json" -o "examples/out/$n.svg"
done
```

Vega-Lite 入力は `--dsl vegalite` を付けます。

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/vegalite.json -o examples/out/vegalite.svg --dsl vegalite
```

PNG が欲しい場合は `--format png`（任意で `--scale 2` などの倍率）を付けます。

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.json -o examples/out/bar.png --format png --scale 2
```

複数 spec をまとめて出力するなら `--out-dir` でバッチ生成もできます。

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.json examples/specs/pie.json --out-dir examples/out/
```

## Fulgur で PDF 化する

`report.html` は生成 SVG を相対パスで参照しているだけの普通の HTML です。
[Fulgur](https://github.com/fulgur-rs) に通すと、SVG がベクターのまま PDF に埋め込まれます。

```sh
fulgur render -o report.pdf report.html
```

PDF 側でも同じ Noto Sans JP をバンドルすると、チャート内テキストの字形が一致します。
