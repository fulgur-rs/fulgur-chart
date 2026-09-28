# fulgur-chart-8do: Vega-Lite errorbar / errorband 設計

## 目的

Vega-Lite の複合 mark `errorbar` と `errorband` を、単体 view の inline data spec で扱う。raw values から統計的な区間を作る入力と、区間を事前計算した入力の両方を受け取り、native と WASM で同じ Scene geometry を描画する。

`errorbar` は区間の rule と任意の端点 ticks、`errorband` は上下限を結ぶ塗りつぶし領域と任意の境界線を作る。汎用 layer / concat の対応には依存せず、複合 mark 自身をひとつの IR chart kind として処理する。

## 受理する仕様

### 入力形式

data source は既存 Vega-Lite frontend と同じ `data.values` inline record array とする。mark は文字列と `{ "type": ... }` の両方を受理する。

raw input は、x または y のうちひとつを連続値の測定軸として扱う。測定軸が x なら水平、y なら垂直の範囲を作る。反対側の x/y channel は各 range point の独立座標になり、`color` と `detail` は系列 group key になる。raw aggregation は測定値を独立座標ごと、かつ系列ごとに集約する。1D mark では反対側の位置 channel を省略できる。この場合、errorbar は反対軸の plot 中央に置き、errorband は反対軸全体を覆う。x/y の両方が連続値で自動判定できない場合は `mark.orient` で測定軸を選ぶ。曖昧な指定、orient と測定 channel の矛盾は parse error にする。

事前集計 input は次の両形式を受理する。各 mark に対して測定軸は x または y の一方だけとする。

- lower / upper: quantitative な `x` + `x2`、または `y` + `y2`。主 channel を lower、`x2` / `y2` を upper とし、lower が upper を超える値は拒否する。
- center + error: quantitative な `x` + `xError` / `xError2`、または `y` + `yError` / `yError2`。`xError` / `yError` は center から upper への非負 offset、`xError2` / `yError2` は center から lower への非正 offset とする。2番目の error がない場合は1つ目を対称 offset に使う。offset 2つの符号が上記と逆の値は拒否する。

事前集計値と raw aggregation を混在させない。事前集計形式では `extent` を拒否し、`x2` と `y2`、`xError` と `yError` のように複数の測定軸を同時指定しない。欠落 field、field 型の不一致、非有限値、null、lower が upper を超える値、未知の extent / orient は、strict モードに限らず明示的な parse error にする。事前集計の range/error channel は field definition のみ受理し、datum/value 定義は拒否する。

### raw data の集計

`extent` は `stderr`、`stdev`、`ci`、`iqr` を受理する。既定は `stderr`。`stderr` は標本標準偏差 / √n、`stdev` は n−1 分母の標本標準偏差を mean の周りに適用する。`ci` は mean の 95% bootstrap percentile interval とし、グループごとに 1,000 回、各回 n 個を復元抽出して平均を計算する。2.5% と 97.5% 点は線形補間で求め、PRNG seed はグループキーから決める。`iqr` は type-7 quantile の q1 / q3 を端点にする。stderr / stdev / ci は n < 2 のグループを拒否する。bootstrap の総抽出数は 10,000,000 回までとし、guard が描画前に検証する。上限超過は truncate せず error にする。

カテゴリー値は first-seen 順に保持する。`color` と `detail` を含む各グループを決定的な順に生成する。必要な値が null / 欠落または算出区間が有限値でないグループは、曖昧な geometry にせず parse error にする。

### mark と encoding の表示属性

両 mark は `extent`、`orient`、`color`、`opacity` と color / opacity encoding を扱う。mark color の既定は `#4682b4`。`color` encoding は value または nominal / ordinal field を受理し、field は series color と凡例に反映する。quantitative color encoding は拒否する。opacity は mark property または `encoding.opacity.value` の 0..1 定数を受理し、field opacity は拒否する。mark opacity の既定は 1。共通 mark 属性は `clip` も受理する。

- `errorbar`: `rule` は既定で表示し、`ticks` は既定で非表示。`ticks: true` または ticks style object がある場合に両端 cap を描く。part style object は color/fill/stroke、strokeWidth、opacity、size、strokeDash を反映する。`rule: false` なら rule を省く。
- `errorband`: `band` は既定 opacity 0.3 で塗り、`borders` は既定で非表示。`borders: true` または style object がある場合に上下境界を描く。part style object は color/fill/stroke、opacity、strokeWidth、strokeDash を反映する。`band: false` なら塗りを省く。
- `errorband` の 2D path は独立軸の昇順で結び、Vega-Lite v6 の `linear`、`linear-closed`、`step`、`step-before`、`step-after`、`basis`、`basis-open`、`basis-closed`、`cardinal`、`cardinal-open`、`cardinal-closed`、`bundle`、`monotone` interpolation を上下境界に同じ設定で適用し、`tension` を該当曲線へ反映する。1D での `interpolate` / `tension` は拒否する。

共通 mark color / opacity は各 component mark に伝播し、part-specific style が指定された属性を上書きする。mark/part の style field は上記の列挙分だけ受理し、unsupported style key は strict/non-strict を問わずエラーにする。opacity / tension は 0..1、strokeWidth / size は有限で 0 以上とする。errorband の同一系列内に同じ独立座標が複数ある事前集計入力は、path の順序が曖昧になるため拒否する。測定軸または独立軸が log scale の場合、範囲の端点・座標に 0 以下の値があれば明示的な error にする。静的 SVG / PNG では tooltip や selection は描画されないため今回の対象外とする。

## アーキテクチャ

1. `schema/vegalite.rs` に errorbar / errorband の discriminated schema と encoding / mark definition types を追加する。strict runtime allowlist と生成 schema の受理範囲を一致させる。
2. `frontend/vegalite.rs` は入力形式と channel combination を判定し、raw values をグループ化・集計するか事前区間を読む。両者を共通の error-range representation に正規化する。統計集計は pure helper に分けて単体テストする。
3. IR は mark 種別、orient、グループ順、各点の独立軸座標・center・lower / upper・解決済み style を保持する。chart kind 固有データとして保持し、既存全 `Series` に optional range vector を追加して通常 chart の IR を増量しない。
4. `layout/error_mark.rs` は範囲データから軸 layout を作り、既存 `AxisSpec` の scale、hard bounds、clip policy と chart frame の余白 / legend を反映して `Scene` primitive を生成する。errorbar は rule / endpoint lines、errorband は fill polygon / optional boundary primitives を出す。native SVG、raster、WASM は既存 Scene renderer を共有する。
5. `model.rs` と `guard.rs` に chart kind の type、axes / series / geometry、total points / generated primitives / bootstrap work の検証を加える。範囲外や不正値で panic せず、既存の parser → guard → layout 境界で error を返す。

## 互換性と制約

- 既存 mark の parsing / layout と出力を変えない。
- URL data、generic transform、layer / concat、facet、tooltip / interaction は別機能の範囲とする。errorbar / errorband 単体 view の内部展開はこの機能内で処理する。
- native と WASM は同じ IR、guard、layout、Scene path を使う。WASM の埋め込み Vega-Lite schema は変更後に再生成する。
- `ChartKind` に public enum variant が加わるため、公開 IR の exhaustive match との source compatibility を release review で確認する。

## 検証

- schema roundtrip と strict parser で string/object mark 形、raw / lower-upper / error-offset 各形式を検証する。
- 統計 helper で stderr、stdev、ci、iqr、group order、同一入力での CI determinism、計算上限を固定する。
- layout test で horizontal / vertical、1D / 2D、categories / temporal / quantitative cross-axis、part styles、各 interpolation、hard bounds clipping、degenerate ranges、non-positive log values を検証する。
- errorbar raw と事前集計、errorband raw と事前集計の examples を追加し、代表的な native render の PNG goldens を登録する。
- WASM の生成 schema parity と parser/render binding test を実行し、既存 `cargo test -p fulgur-chart`、WASM tests、clippy、fmt を通す。

## 参照

- [Vega-Lite v6 Error Bar](https://vega.github.io/vega-lite/docs/errorbar.html)
- [Vega-Lite v6 Error Band](https://vega.github.io/vega-lite/docs/errorband.html)
- [Vega-Lite errorbar normalizer](https://raw.githubusercontent.com/vega/vega-lite/main/src/compositemark/errorbar.ts)
- [Vega-Lite errorband normalizer](https://raw.githubusercontent.com/vega/vega-lite/main/src/compositemark/errorband.ts)
