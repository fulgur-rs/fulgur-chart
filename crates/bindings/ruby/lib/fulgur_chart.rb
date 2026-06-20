# frozen_string_literal: true

# 公開 API（FulgurChart.render_svg/render_image/render_png/schema/version）とエラー階層
# (FulgurChart::ParseError/StrictError/RenderError) はすべて native(ext) 側で定義される。
# モジュール名は `Fulgur` ではなく `FulgurChart`: top-level `Fulgur` は Fulgur(PDF) ライブラリと
# 衝突するため、最初から `FulgurChart` 名前空間で expose する。
require_relative "fulgur_chart/fulgur_chart" # native ext (Init_fulgur_chart -> module FulgurChart)
