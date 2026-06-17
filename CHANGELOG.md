# Changelog

このプロジェクトの主な変更点を記録します。
フォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に従い、
バージョニングは [Semantic Versioning](https://semver.org/lang/ja/) に従います。

## [0.1.0] - 2026-06-17

### Added

- 棒グラフ（縦 / 横）・折れ線グラフ・エリアチャート・円グラフ・ドーナツグラフに対応。
- chart.js v4 互換のデータ専用・静的サブセットの入力に対応。
- SVG / PNG の出力に対応（PNG は `--scale` で解像度倍率を指定可能）。
- `render` サブコマンドを持つ CLI（ファイル / 標準入力・標準出力のパイプ、`--strict`）。
- 決定的な出力（同一入力なら byte-identical）。
- Noto Sans JP フォントを同梱（システムフォントは読み込まない）。

[0.1.0]: https://github.com/fulgur-rs/fulgur-chart/releases/tag/v0.1.0
