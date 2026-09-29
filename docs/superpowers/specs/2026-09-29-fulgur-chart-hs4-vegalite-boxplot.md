# fulgur-chart-hs4: Vega-Lite boxplot 設計

## 目的

Vega-Lite v6 の複合 mark `boxplot` を、inline raw data の単体 view で描画する。各系列の Q1・中央値・Q3、whisker、Tukey outlier を dedicated IR に正規化し、専用 layout で箱ひげ図の各構成要素を Scene primitive として生成する。native と WASM は同じ parser、IR、guard、layout、Scene renderer を使う。

## 対応する仕様

### 入力と軸

- `mark` は文字列 `"boxplot"` と `{ "type": "boxplot", ... }` を受理する。
- data source は `data.values` の inline record array とする。`data.url`、欠落した data、空の配列、record 以外の要素は明示的な parse error にする。
- `encoding.x` と `encoding.y` のうち、測定値には quantitative field をひとつ指定する。反対側の position channel は省略するか、カテゴリ field にする。x/y が両方 quantitative、または両方とも測定値として解釈できない入力は拒否する。
- 測定値を x に置くと horizontal、y に置くと vertical とし、`mark.orient` の省略時はこれを自動判定する。明示値は `"horizontal"` / `"vertical"` を受理するが、測定軸と矛盾する場合は parse error にする。
- 位置カテゴリ、`color`、`detail` の組み合わせごとにひとつの箱を作る。カテゴリ値と group はデータの first-seen 順で安定させる。カテゴリ位置が省略された 1D boxplot は測定値軸と直交する plot 中央に配置する。
- `transform`、`layer`、pre-aggregated summary、URL data は対象外とし、strict/non-strict の両 parser mode で書き込みや描画前に明示エラーにする。これらを raw records と混在させない。
- 必須 field の欠落、field 型の不一致、null、有限数でない測定値、group field の欠落は parse error にする。黙って行を落としたり、0 に置換したりしない。

### 統計とグループ化

各カテゴリ・color・detail group の測定値から、線形補間による type-7 quantile で Q1、median、Q3 を求める。IQR は `Q3 - Q1` とする。入力順と group の first-seen order を保ち、同一入力の結果は決定的にする。

- `extent` の既定値は Tukey の係数 `1.5`。有限で 0 以上の数値を指定した場合はその係数を使う。
- Tukey fence を `[Q1 - k * IQR, Q3 + k * IQR]` とし、whisker endpoint は fence 内にある実データの最小値と最大値にする。範囲外の各 raw observation を outlier として保持する。
- `extent: "min-max"` は実データの最小値と最大値を whisker endpoint にし、outlier primitive は生成しない。
- 統計に使う点数、グループ数、および layout が生成する primitive 数は既存 `InputLimits` を拡張して検査する。上限超過を切り詰めずエラーにし、上限検査は大きな集計・Scene allocation の前に行う。
- 軸 domain は whisker だけでなく全入力測定値から決め、outlier も含める。明示 scale bounds は既存 axis の hard-bound / clipping 規則に従う。

### mark・encoding・part style

受理する mark-level property は `extent`、`orient`、`size`、`color`、`opacity`、`clip` と、下記の component properties とする。encoding は `x`、`y`、`color`、`detail`、`size`、`opacity` を受理する。未対応 property、未対応 channel、未対応の `scale` / `legend` 拡張は strict/non-strict を問わず明示エラーにする。既存の共通 title など、parser が既に扱う chart-level property は従来どおり扱う。

- mark の既定色は既存 Vega-Lite palette を使う。mark `color` と `encoding.color` は箱と outlier に適用し、カテゴリ color field は group 分けと凡例にも反映する。
- `encoding.size` は quantitative field/value を受理し、Vega-Lite の意味に合わせて box と median tick の幅へ反映する。mark `size` は同じ構成部分の固定幅を指定する。encoding opacity は指定された mark 全体の opacity に適用し、値は有限な 0..1 とする。
- mark `clip` は plot frame 外へ出る primitive の Scene clipping を制御する。省略時の既定動作は既存 Vega-Lite mark default に合わせる。
- `box`、`median`、`outliers`、`rule`、`ticks` は boolean または style object を受理する。object で受理する key は `color`、`fill`、`stroke`、`strokeWidth`、`strokeDash`、`opacity`、`size` のみ。false は該当 component を省略し、true は既定 style を使う。style object の指定属性は mark-level / encoding の対応属性より優先する。
- `box` は Q1〜Q3 の矩形、`median` は中央値 tick、`rule` は whisker、`ticks` は whisker endpoint caps、`outliers` は Tukey outlier points に対応する。未指定 component の可視性と default style は Vega-Lite v6 の boxplot default に合わせる。
- opacity は 0..1、size / strokeWidth は有限な 0 以上の値とし、strokeDash の各要素は有限な 0 以上とする。不正値や component object の未知 key は parser mode に関わらず拒否する。

## アーキテクチャ

1. `schema/vegalite.rs` に boxplot 用の root variant、mark string/object、data、encoding、component style types を追加する。生成 JSON Schema と runtime allowlist の受理範囲を揃える。
2. `frontend/vegalite_boxplot.rs` を専用 parser とし、既存 `frontend/vegalite.rs` から `boxplot` を共通 parser より先に dispatch する。inline records の検証、channel/orientation 解決、統計集計、style 解決を担う。`transform` / `layer` / pre-aggregated inputs は strict/non-strict 両 mode で拒否する。
3. `ir.rs` に Vega-Lite 固有の `VegaBoxPlot` chart kind と専用 data / group / style structures を追加する。各 group はカテゴリ位置、color/detail identity、Q1/median/Q3、実データ whisker endpoint、raw outlier values、resolved component styles を保持する。既存 Chart.js `ChartKind::BoxPlot` と generic `Series` contract は変更しない。
4. `layout/vega_boxplot.rs` を Scene builder として追加する。縦横両 orient の axis/frame を作り、1D chart は直交方向の中央へ、カテゴリ別 chart はカテゴリ中心へ置く。同じカテゴリ内の複数 group は決定的な順で横並びに配置し、box、median、whisker rule、cap ticks、outlier primitives を生成する。軸目盛り、凡例、hard bounds、clip は既存共通 layout / Scene 規約に合わせる。
5. `layout/mod.rs`、`model.rs`、`guard.rs` など ChartKind を列挙する接続点へ新 variant を登録し、データ件数と生成 primitive 数を guard する。既存 chart の parser / layout / renderer の動作は変えない。
6. example と integration / golden tests に fixture を加え、native と WASM で同じ parser と Scene 描画経路を検証する。WASM に埋め込み Vega-Lite JSON Schema がある場合は生成物も更新する。

## 互換性と制約

- この機能は Vega-Lite の単体 boxplot mark に限る。generic `transform`、`layer`、facet / concat、pre-aggregated summary、URL 読み込み、tooltip / interaction は別対応とする。
- `ChartKind::BoxPlot` は QuickChart / Chart.js 用なので流用しない。Vega-Lite 固有 variant と専用 layout で既存契約を分離する。
- 新しい公開 `ChartKind` variant を追加する場合は、公開 IR を exhaustive match する downstream crate への source compatibility を release review で確認する。
- native と WASM は同じ IR、guard、layout、Scene path を共有し、同一入力に対して同じ geometry を生成する。

## 検証

- schema と parser tests で mark string/object、inline data、必須 field、1D / categorical 2D、horizontal / vertical auto-orient、明示 orient、矛盾した orient、strict/non-strict の unsupported input を検証する。
- 統計 tests で type-7 Q1/median/Q3、Tukey default と別係数、実データ whiskers、複数 outlier、min-max、同一値・単一値 group、first-seen group order を固定する。
- style tests で mark / encoding / component precedence、color grouping、size と opacity、各 component の表示切替、clip と不正 style input を検証する。
- layout tests で縦横 orient、1D 中央配置、カテゴリ/group の deterministic side-by-side 配置、outlier を含む axis domain、hard bounds と clip、primitive limit を検証する。
- Vega-Lite boxplot example と登録済み PNG golden を追加し、native render と WASM parser/render の同じケースをテストする。既存の boxplot、Vega-Lite mark、WASM tests も回帰確認する。

## 参照

- [Vega-Lite v6 Box Plot](https://vega.github.io/vega-lite/docs/boxplot.html)
- [Vega-Lite boxplot composite mark implementation](https://github.com/vega/vega-lite/blob/main/src/compositemark/boxplot.ts)
