# frozen_string_literal: true

require_relative "fulgur_chart/fulgur_chart" # native ext (Init_fulgur_chart -> module Fulgur)

# 公開 API・エラー階層は native(ext) 側で定義される。ここでは受け入れ基準が要求する
# FulgurChart.* を Fulgur のエイリアスとして提供するのみ（定義の単一ソース化）。
FulgurChart = Fulgur unless defined?(FulgurChart)
