# PR #137 Fourth Review Fixes Design

## Goal

PR #137 に追加された5件のレビュー指摘を、既存のVega-Lite temporal line契約と
PlotAreaの要求サイズを維持したまま解消する。

## Schema boundary

公開JSON Schemaのline variantをtemporalとcategoricalに分ける。

- temporal lineは`encoding.x.type = "temporal"`を必須とし、`mark.point`、
  `mark.interpolate`、`background`、`config`、axis title、color title/scaleを公開する。
- categorical lineのx typeは`nominal | ordinal`または省略、y typeは
  `quantitative`または省略、color typeは`nominal | ordinal`または省略とする。
  temporal専用フィールドは公開しない。
- x、y、colorには用途別enumを使い、runtimeの`validate_line_channel_types`と同じ値だけを
  Schemaで受理する。
- temporal/categoricalともに、color channelを指定する場合は`field`を必須とする。
  runtimeのstrict検証も同じ契約に揃え、意味を持たないcolor objectを拒否する。

serdeのuntagged variantは、必須の`encoding.x.type = "temporal"`でtemporal variantを
識別する。categorical variantはtemporalをx typeとして受理しないため、両variantの
受理集合は重ならない。

## Temporal coordinate arithmetic

`temporal_x`は`value - min`と`max - min`を`i128`で計算してから`f64`へ変換する。
これにより`i64::MIN..i64::MAX`を含む、検証済みの単調増加timestampでもdebug panicや
release wrapを起こさず、端点をplotの左右へ写像できる。

## PlotArea vertical legend bounds

PlotAreaの要求plot高さは変更しない。縦凡例の行数（titleがあれば1行追加）から
`LEGEND_ROW_H`単位のgroup高さを計算し、groupがplotより高い場合の片側overflowを
上下に追加する。

- `plot_top`を上側overflowぶん下へ移動する。
- `plot_bottom = plot_top + spec.height`として要求plot高さを維持する。
- `scene_height`の末尾にも同じoverflowを追加する。

これにより中央配置された凡例全体がscene内へ収まり、既存の短い凡例ではscene寸法を
変えない。

## Error handling and compatibility

- 新しいruntime rejectionはstrict lineのcolor channel欠損fieldに限定する。
- categorical lineの既存有効入力とtemporal dogfood fixtureは引き続き受理する。
- Schema生成はschemarsのderiveを維持し、手書きSchema分岐は導入しない。

## Verification

各修正は先に失敗する回帰テストを追加してREDを確認し、最小実装後にGREENを確認する。
対象はSchema validation、strict parser、layout unit testsとする。最後にfmt、clippy、
fulgur-chart/chart-server tests、wasm check、workspace llvm-covを実行し、
`origin/main...HEAD`のpatch coverage 100%を確認する。
