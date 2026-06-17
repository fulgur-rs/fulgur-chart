# examples

fulgur-chart のサンプル spec と、それを実 CLI で描画した SVG、Fulgur に通す想定の
レポート HTML を置いています。

## ディレクトリ

- `specs/` … chart.js v4 互換の入力 spec（JSON）
  - `bar.json` … 棒グラフ（月次売上）
  - `line.json` … 折れ線グラフ（2 系列・`tension` でなめらかに）
  - `area.json` … エリアチャート（`"fill": true` の line）
  - `pie.json` … 円グラフ（スライスごとに自動配色）
- `out/` … 上記 spec を CLI で描画した SVG（リポジトリに同梱）
- `report.html` … 生成 SVG を `<img>` で埋め込んだ最小レポート

## SVG の生成（再生成手順）

リポジトリのルートから、各 spec を CLI に通して `out/` を再生成します。

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.json  -o examples/out/bar.svg
cargo run -q -p fulgur-chart-cli -- render examples/specs/line.json -o examples/out/line.svg
cargo run -q -p fulgur-chart-cli -- render examples/specs/area.json -o examples/out/area.svg
cargo run -q -p fulgur-chart-cli -- render examples/specs/pie.json  -o examples/out/pie.svg
```

出力は決定的（同一入力なら byte-identical）なので、再生成しても差分は出ません。

PNG が欲しい場合は `--format png`（任意で `--scale 2` などの倍率）を付けます。

```sh
cargo run -q -p fulgur-chart-cli -- render examples/specs/bar.json -o examples/out/bar.png --format png --scale 2
```

## Fulgur で PDF 化する

`report.html` は生成 SVG を相対パスで参照しているだけの普通の HTML です。
[Fulgur](https://github.com/fulgur-rs) に通すと、SVG がベクターのまま PDF に埋め込まれます。

```sh
fulgur render -o report.pdf report.html
```

PDF 側でも同じ Noto Sans JP をバンドルすると、チャート内テキストの字形が一致します。
