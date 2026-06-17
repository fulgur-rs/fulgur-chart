# fulgur-chart 設計ドキュメント

- **日付**: 2026-06-17
- **ステータス**: 設計合意済み（実装前）
- **位置づけ**: Fulgur（HTML/CSS → PDF レンダラ）のサイドプロジェクト

## 1. 目的・スコープ・非目標

### 目的

chart.js v4 互換の JSON スペックを入力に、**決定的（byte-stable）な静的 SVG** を生成し、
Fulgur で PDF にベクター埋め込みできる CLI ツールを作る。PNG 出力もサポートする。

Fulgur はインライン `<svg>` を `usvg`/`krilla-svg` 経由でベクターとして PDF に描画できる。
fulgur-chart はこの接続点を活かし、Fulgur のテンプレート駆動レポート生成を「チャート」で補完する。

### スコープ（v1 に入れる）

- chart.js v4 設定オブジェクトの **データ専用・静的サブセット**（→ 第4章）
- 4 チャート種: `bar`（縦/横）・`line`・`pie`/`doughnut`・`area`（= `line` + dataset `fill: true`）
- 複数データセット（系列）、軸・グリッド・凡例・タイトル・データラベル
- chart.js v4 のデフォルト配色・レイアウトへの準拠
- 日本語ラベル（**Noto Sans JP 同梱**）

### 非目標（v1 で作らない = YAGNI）

- JavaScript 由来の機能全般: `ticks.callback` 等のスクリプタブルオプション、ツールチップ、
  プラグイン、**アニメーション・インタラクション**
- Vega-Lite フロントエンド（IR 境界だけ用意し、実装は後）
- 散布図・レーダー・バブル・混合チャート等（v1 の 4 種以外）
- テーマカスタマイズ機構（v1 は chart.js v4 デフォルト準拠のみ）

### 最重要の品質目標

**決定性**。同一入力 → byte-identical な SVG。Fulgur の CI / スナップショット用途に必須。

## 2. アーキテクチャ（層構造）

両 DSL を見据え、**フロントエンド（DSL）→ IR → 描画コア** を分離する。
chart.js フロントエンドだけ先行出荷し、描画コアは一本化する。

```
┌─────────────────────────────────────────────────────────┐
│ CLI (fulgur-chart)                                       │
│  render <spec.json> -o out.svg --format svg|png          │
└───────────────┬─────────────────────────────────────────┘
                │ 入力JSON（chart.js v4 spec）
                ▼
┌─────────────────────────────────────────────────────────┐
│ ① Frontend / DSL アダプタ                                │
│   chartjs::parse(spec) → IR     (将来: vegalite::parse)  │
│   ・未知/非対応キーは無視 or エラー（--strict）          │
│   ・色解決・デフォルト補完をここで完了                    │
└───────────────┬─────────────────────────────────────────┘
                ▼  IR (ChartSpec) — DSL非依存の正規化モデル
┌─────────────────────────────────────────────────────────┐
│ ② Layout / Scale エンジン                                │
│   ・データ範囲→軸スケール、目盛り(nice ticks)算出        │
│   ・凡例/タイトル/軸の領域確保、プロット領域の確定        │
│   ・文字幅計測（同梱フォントのメトリクスで）             │
│   → Scene（描画プリミティブの集合: 矩形/線/円弧/text）   │
└───────────────┬─────────────────────────────────────────┘
                ▼  Scene（純粋な幾何 + スタイル）
┌──────────────────────────┬──────────────────────────────┐
│ ③ SVG レンダラ           │ ④ PNG ラスタライザ           │
│   Scene → SVG文字列       │   SVG → resvg/tiny-skia → PNG │
└──────────────────────────┴──────────────────────────────┘
```

### 設計の要点

- **IR（`ChartSpec`）が安定境界**。chart.js も将来の Vega-Lite もここへ落とす。
  ただし IR は v1 の 4 種が必要とする形に限定する（Vega-Lite の文法に無理に寄せない）。
  Vega-Lite が実際に来たときにリファクタする。
- **Scene 層**を挟むのが肝。「データの解釈」（②まで）と「SVG の文字列化」（③）を分離する。
- **PNG は SVG を正としてラスタライズして得る**（Scene から直接 tiny-skia で描かない）。
  描画ロジックを一本化し、SVG と PNG の見た目の乖離を防ぐ。
- 各層は純関数。乱数・時刻・グローバル状態なし → 決定性を構造的に担保する。

## 3. chart.js v4 サブセットと IR

### 入力の形（chart.js v4 そのまま）

```json
{
  "type": "bar",
  "data": {
    "labels": ["1月", "2月", "3月"],
    "datasets": [
      { "label": "売上", "data": [120, 200, 150], "backgroundColor": "#36A2EB" }
    ]
  },
  "options": {
    "plugins": { "title": { "display": true, "text": "四半期売上" } }
  }
}
```

### v1 でサポートするキー（データ専用・静的サブセット）

- `type`: `bar` / `line` / `pie` / `doughnut`（`area` は `line` + dataset `fill: true`）
- `data.labels`, `data.datasets[]`:
  `label` / `data` / `backgroundColor` / `borderColor` / `borderWidth` /
  `fill` / `tension`（line 曲線）/ `pointRadius`
- `options.indexAxis`（`"y"` で横棒）
- `options.plugins.title`（display / text）
- `options.plugins.legend`（display / position）
- `options.scales`（min / max / title / grid 表示 / beginAtZero）
- `options.plugins.datalabels` 相当の最小データラベル
- **デフォルト配色**: 単色指定が無い場合、chart.js v4 のデフォルトカラー
  （`#36A2EB`, `#FF6384`, … の循環）を再現する。実 v4 の自動配色挙動を実装前に検証する。

### 非対応キーの扱い

デフォルトは **「未知キーは無視して描画継続」**（chart.js も寛容）。
ただし `--strict` で未知 / 非対応キー検出時にエラーにする。
「壊れず動く」と「取りこぼし検知」を両立する。

### IR（`ChartSpec`）の概形（DSL 非依存・正規化済み）

```
ChartSpec {
  kind: Bar { horizontal } | Line { area, tension } | Pie { donut_ratio },
  series: Vec<Series { name, values: Vec<f64>, color, ... }>,
  categories: Vec<String>,              // x軸ラベル
  axes: { x: AxisSpec, y: AxisSpec },   // pie では無視
  legend: LegendSpec,
  title: Option<TitleSpec>,
  size: { width, height },
  theme: Theme,                          // v1 は固定 theme
}
```

色解決・デフォルト補完は **① フロントエンドで完了**させ、
IR は「決まった値」だけを持つ（② ③ は判断しない）。

## 4. フォントとテキスト（決定性の要）

「**計測フォント = `font-family` 指定 = 描画時フォントの三者一致**」を構造的に守る。

### 同梱フォント

**Noto Sans JP**（Latin + 仮名 + 常用漢字をカバー）を **1 本だけ**バンドルする。
Latin と日本語を同一フォントで扱うことで、計測と描画のフォントが必ず一致する。

- chart.js の既定 Arial メトリクスへの厳密一致は**断念**する。日本語必須を選んだ時点で、
  chart.js 自身もシステム CJK フォントに落ちるため、メトリクス一致は元々無意味。
  代わりに **chart.js v4 のレイアウト・配色・構造**に準拠し、フォントは中立的な
  Noto Sans JP を既定とする。
- `--font <path>` で差し替え可能（Latin 専用に Arial 互換 Liberation Sans を使う等）。

### 文字幅計測

同梱した Noto Sans JP のフォントデータを `ttf-parser` / `rustybuzz` で読み、
グリフ進み幅を自前計算する。軸ラベルの右寄せ・中央寄せ・はみ出し回避・凡例幅を
この計測値で決定する → **どの環境でも同じ座標**になる。

### SVG への出力

`<text>` 要素に `font-family="Noto Sans JP, sans-serif"` と確定座標を付与する。
アウトライン化しない（軽量・Tagged PDF と相性が良い）。

### フォントの所在（三者一致の担保）

1. **計測**: バイナリに埋め込んだ（or 同梱パスから読む）Noto Sans JP。
2. **PNG ラスタライズ**: resvg の fontdb に同じ Noto Sans JP をロード → 一致。
3. **Fulgur で PDF 化**: 利用者が Fulgur 側でも**同じ Noto Sans JP をバンドル**するよう
   README で明示する。仮に異なってもレイアウト座標は ① で固定済みなので崩れにくい。

### バイナリサイズ

フル Noto Sans JP は数 MB。v1 は素直に埋め込む（offline / 決定性優先 = Fulgur 思想）。
将来、出力テキストに対するサブセット同梱を検討する。

## 5. CLI 設計

`type` がスペック内にあるので、チャート種ごとのサブコマンドは廃し、**`render` 一本**に統一する。

```bash
# 基本: chart.js spec → SVG
fulgur-chart render spec.json -o chart.svg

# PNG 出力（SVGをラスタライズ）
fulgur-chart render spec.json -o chart.png --format png --scale 2

# stdin → stdout（パイプ運用）
cat spec.json | fulgur-chart render - -o -

# サイズ・フォント上書き
fulgur-chart render spec.json -o c.svg --width 800 --height 400 --font ./Liberation.ttf

# 取りこぼし検知
fulgur-chart render spec.json -o c.svg --strict
```

### サブコマンド / フラグ

| 項目 | 内容 |
|------|------|
| `render <spec>` | `-` で stdin。唯一のコアコマンド |
| `-o, --output` | `-` で stdout。拡張子から format 自動推定 |
| `--format svg\|png` | 明示指定（省略時は出力拡張子で判定、stdout は svg 既定） |
| `--width` / `--height` | spec を上書き（既定: `options` → 無ければ 800×450） |
| `--scale` | PNG の解像度倍率（既定 1.0、決定的） |
| `--font <path>` | 既定フォント差し替え |
| `--strict` | 未知 / 非対応キーでエラー |
| `--dsl chartjs` | 入力 DSL 明示（既定 `chartjs`、将来 `vegalite`） |

### 典型ワークフロー（Fulgur 連携）

```bash
fulgur-chart render sales.json -o out/sales.svg
# report.html が <img src="sales.svg"> もしくはインライン展開で参照
fulgur render -o report.pdf report.html
```

### 終了コード

- `0`: 成功
- `1`: 入力エラー（JSON 不正・必須キー欠落）
- `2`: `--strict` 違反
- `3`: 描画 / IO 失敗

エラーは stderr に行番号付きで出す。

## 6. 決定性・テスト・リポジトリ構成

### 決定性の機構（byte-identical を構造的に保証）

- 浮動小数は**固定精度でフォーマット**（例: 座標は小数 2 桁、`format!("{:.2}")`）。
  ロケール非依存にする。
- 要素の出力順序を**安定化**（`HashMap` の反復順に依存しない。系列・凡例は入力順
  or 明示ソート）。
- **時刻・乱数・絶対パスを出力に含めない**。SVG に生成日時等を埋めない。
- フォントは同梱版にピン留め。`usvg` / `resvg` / `tiny-skia` のバージョンも
  `Cargo.lock` で固定する。

### テスト戦略

- **スナップショット（VRT）**: 代表 spec → SVG を `insta` 等で固定。差分でリグレッション検知。
  Fulgur に `fulgur-vrt` があるので発想を踏襲する。
- **PNG ピクセル比較**: 少数の golden PNG を許容誤差付きで比較
  （resvg / tiny-skia のマイナー差を吸収）。
- **chart.js 適合テスト**: 同じ spec を実 chart.js（puppeteer 等）でも描き、
  構造・配色を目視 / 数値照合（CI 外の参照ツールとして）。
- 単体: スケール / nice-ticks、色解決、文字幅計測の純粋関数テスト。

### リポジトリ構成（独立 git リポジトリ）

```
fulgur-chart/
  Cargo.toml                 # [workspace]
  crates/
    fulgur-chart/            # コアlib: frontend/ir/layout/svg/raster
    fulgur-chart-cli/        # CLIバイナリ（薄いラッパ）
    fulgur-chart/assets/fonts/NotoSansJP... # 同梱フォント（クレート内に格納）
  tests/                     # snapshot + golden
  examples/                  # サンプル spec & 期待出力
  README.md  CHANGELOG.md  LICENSE(MIT or Apache-2.0)
```

### 主要依存

- `usvg` / `resvg` / `tiny-skia`（0.x、Fulgur と整合）: SVG パース・ラスタライズ
- `ttf-parser` / `rustybuzz`: 文字幅計測
- `serde` / `serde_json`: spec パース
- `clap`: CLI
- `fontdb`: フォント管理
- `insta`: スナップショットテスト

## 未解決事項 / 実装時に確定する点

- chart.js v4 の自動配色アルゴリズムの正確な再現（実 v4 で要検証）
- nice-ticks アルゴリズムの選定（chart.js の目盛り生成挙動にどこまで寄せるか）
- 同梱フォントのライセンス確認（Noto Sans JP は SIL OFL）
- `usvg` / `resvg` のバージョンを Fulgur と完全一致させるか、独立に選ぶか
