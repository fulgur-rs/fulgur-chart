# fulgur-chart

[![CI](https://github.com/fulgur-rs/fulgur-chart/actions/workflows/ci.yml/badge.svg)](https://github.com/fulgur-rs/fulgur-chart/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/fulgur-chart-cli.svg)](https://crates.io/crates/fulgur-chart-cli)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#ライセンス)

chart.js v4 互換の JSON spec から、静的な SVG / PNG チャートを生成する CLI です
（[Fulgur](https://github.com/fulgur-rs) のサイドプロジェクト）。

## Why

ブラウザも JavaScript も使わずに、決定的（同一入力なら byte-identical）なチャートを
生成します。Fulgur と組み合わせれば、生成した SVG をベクターのまま PDF に埋め込めます。
CI でレポートを再生成しても差分が出ないため、図版を版管理しやすくなります。

## インストール

```sh
cargo install --path crates/fulgur-chart-cli
```

`fulgur-chart` という名前のバイナリがインストールされます。

## 使い方

最小の chart.js spec（`chart.json`）を用意します。

```json
{
  "type": "bar",
  "data": {
    "labels": ["1月", "2月", "3月"],
    "datasets": [
      { "label": "売上 (万円)", "data": [120, 200, 150], "backgroundColor": "#36a2eb" }
    ]
  },
  "options": {
    "plugins": { "title": { "display": true, "text": "月次売上" } }
  }
}
```

SVG / PNG を生成します。

```sh
# SVG（既定）
fulgur-chart render chart.json -o chart.svg

# PNG（--scale で解像度倍率。2 なら 2 倍の画素数）
fulgur-chart render chart.json -o chart.png --format png --scale 2
```

`-` で標準入力・標準出力をパイプできます。

```sh
cat chart.json | fulgur-chart render - -o - > chart.svg
```

主なオプション:

- `--format svg|png` … 出力形式。省略時は出力拡張子で判定（`.png` なら png、それ以外/stdout は svg）。
- `--width <px>` / `--height <px>` … キャンバスの幅・高さを上書き（既定 800 x 450）。
- `--scale <倍率>` … PNG の解像度倍率（既定 1.0）。
- `--font <path>` … 計測・SVG・PNG で使うフォントを差し替え（既定は同梱 Noto Sans JP）。
- `--out-dir <dir>` … 複数 spec の一括出力先（後述のバッチ生成）。
- `--dsl chartjs|vegalite` … 入力 DSL（既定 chartjs。後述の Vega-Lite 入力）。
- `--strict` … 未知/非対応のキーをエラーにする（既定では無視）。

```sh
# 幅・高さを上書きし、strict で未知キーを検出
fulgur-chart render chart.json -o chart.svg --width 1024 --height 576 --strict
```

### バッチ生成

複数の spec を一括でレンダリングできます（CI でのレポート図版生成向け）。各入力
`X.json` は `<out-dir>/X.<拡張子>` に出力されます（出力はファイル単位で byte-identical）。

```sh
fulgur-chart render specs/*.json --out-dir out/            # それぞれ out/<名前>.svg
fulgur-chart render specs/*.json --out-dir out/ --format png
```

## 対応チャート種

- 棒グラフ（縦 / 横。横は `options.indexAxis: "y"`）
- 積み上げ棒グラフ（`options.scales.{x,y}.stacked: true`）
- 折れ線グラフ
- エリアチャート（`datasets[].fill: true` の line）
- 円グラフ（pie）
- ドーナツグラフ（doughnut）
- 散布図（scatter。`{x, y}` 点データ）
- バブルチャート（bubble。`{x, y, r}` 点データ）
- レーダーチャート（radar）
- 混合チャート（dataset ごとに `type` を変える bar + line）

## 対応する chart.js サブセット

データ専用・静的な範囲をサポートします。

- `type` … `bar` / `line` / `pie` / `doughnut` / `scatter` / `bubble` / `radar`
- `data.labels`
- `data.datasets[]` … `label` / `data`（数値配列、または scatter/bubble の `{x,y}` /
  `{x,y,r}` 配列）/ `backgroundColor` / `borderColor` / `borderWidth` / `fill` /
  `tension` / `pointRadius` / `type`（混合チャート用の dataset 別種別）
- `options.indexAxis`
- `options.plugins.title` / `options.plugins.legend`（`position` は top/bottom/left/right）
- `options.plugins.datalabels`（`display`。各データ点に値を描画）
- `options.scales`（`stacked` 等、一部）
- `options.theme`（拡張。後述のテーマ）

JS 由来の動的な機能（`callback` / `animation` / `interaction` / プラグインの
スクリプト等）は非対応です。**未知のキーは既定で無視**し、`--strict` を付けると
検出してエラーにします。

## テーマ（`options.theme`）

chart.js v4 既定の配色・スタイルを基準に、`options.theme` で見た目を上書きできます
（chart.js 本体には無い拡張キー。指定しなければ既定のまま）。

- `palette` … 系列/スライスの自動配色に使う色文字列の配列
- `gridColor` / `textColor` … グリッド線色 / 文字色
- `backgroundColor` … キャンバス背景（既定は透明）
- `fontSize` … ラベルの基準フォントサイズ（px）

色は `#rgb` / `#rrggbb` / `rgb()` / `rgba()` / `hsl()` / `hsla()` / CSS 色名で指定できます。

## Vega-Lite 入力（`--dsl vegalite`）

chart.js spec に加えて、Vega-Lite の最小サブセットを入力にできます。

```sh
fulgur-chart render chart.vl.json -o chart.svg --dsl vegalite
```

対応サブセット: `mark`（`bar` / `line` / `point`→散布図 / `arc`→円）、インラインの
`data.values`、`encoding` の `x` / `y` / `color` / `theta`。内部で共通の中間表現へ
変換するため、出力の決定性や Fulgur 連携は chart.js 入力と同じです。

## Fulgur 連携

生成した SVG を HTML に `<img>` で埋め込み、Fulgur で PDF 化します。

```html
<img src="out/bar.svg" alt="月次売上">
```

```sh
fulgur render -o report.pdf report.html
```

最小例は [`examples/report.html`](examples/report.html) を参照してください。
PDF 化の際、Fulgur 側でも同じ Noto Sans JP をバンドルすると、チャート内テキストの
字形が一致します。

## 決定性

同一の入力 spec からは byte-identical な出力が得られます。フォントは同梱の
Noto Sans JP 1 本のみを使い、システムフォントは読み込みません。

## 将来対応（未対応）

以下は現時点では実装していません（将来対応の候補）。

- レーダーの値目盛りラベル、散布図/レーダーのデータラベル
- 混合チャートの二軸（左右で別 y スケール）・横棒/積み上げとの混合
- Vega-Lite の URL データ・`transform`・`aggregate`（現状は inline `data.values` のみ）
- フォントのサブセット同梱（バイナリサイズ削減）

## ライセンス

コードは [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE) のデュアルライセンスです。

同梱フォント Noto Sans JP は [SIL Open Font License 1.1](crates/fulgur-chart/assets/fonts/LICENSE-NotoSansJP.txt)
で配布されており、上流 [notofonts / noto-cjk](https://github.com/notofonts/noto-cjk)
の配布物をそのまま同梱しています。
