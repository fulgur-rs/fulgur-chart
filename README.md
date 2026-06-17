# fulgur-chart

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
- `--strict` … 未知/非対応のキーをエラーにする（既定では無視）。

```sh
# 幅・高さを上書きし、strict で未知キーを検出
fulgur-chart render chart.json -o chart.svg --width 1024 --height 576 --strict
```

## 対応チャート種

- 棒グラフ（縦 / 横。横は `options.indexAxis: "y"`）
- 折れ線グラフ
- エリアチャート（`datasets[].fill: true` の line）
- 円グラフ（pie）
- ドーナツグラフ（doughnut）

## 対応する chart.js サブセット

データ専用・静的な範囲をサポートします。

- `type` … `bar` / `line` / `pie` / `doughnut`
- `data.labels`
- `data.datasets[]` … `label` / `data` / `backgroundColor` / `borderColor` /
  `borderWidth` / `fill` / `tension` / `pointRadius`
- `options.indexAxis`
- `options.plugins.title` / `options.plugins.legend`
- `options.scales`（一部）

JS 由来の動的な機能（`callback` / `animation` / `interaction` / プラグインの
スクリプト等）は非対応です。**未知のキーは既定で無視**し、`--strict` を付けると
検出してエラーにします。

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

## 将来対応（v1 では未対応）

以下は v1 では実装していません（将来対応の候補）。

- データラベル（datalabels）
- 凡例の Left / Right 配置
- `--font` によるフォント差し替え
- Vega-Lite DSL 入力

## ライセンス

コードは [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE) のデュアルライセンスです。

同梱フォント Noto Sans JP は [SIL Open Font License 1.1](crates/fulgur-chart/assets/fonts/LICENSE-NotoSansJP.txt)
で配布されており、上流 [notofonts / noto-cjk](https://github.com/notofonts/noto-cjk)
の配布物をそのまま同梱しています。
