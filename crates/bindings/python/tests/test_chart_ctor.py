"""Chart() コンストラクタのテスト — pyfulgur の test_engine_ctor.py に対応。"""
import json

import pytest

import fulgur_chart
from fulgur_chart import Chart, FulgurParseError, FulgurRenderError

CHARTJS_BAR = json.dumps({
    "type": "bar",
    "data": {
        "labels": ["A", "B"],
        "datasets": [{"data": [1, 2]}],
    },
})


def test_chart_no_args_render_svg():
    chart = Chart()
    svg = chart.render(CHARTJS_BAR, "svg")
    assert svg.startswith("<svg")


def test_chart_no_args_render_default_is_svg():
    chart = Chart()
    svg = chart.render(CHARTJS_BAR)
    assert svg.startswith("<svg")


def test_chart_width_height_kwargs():
    chart = Chart(width=400.0, height=300.0)
    svg = chart.render(CHARTJS_BAR, "svg")
    assert svg.startswith("<svg")


def test_chart_dsl_kwarg_chartjs():
    chart = Chart(dsl="chartjs")
    svg = chart.render(CHARTJS_BAR, "svg")
    assert svg.startswith("<svg")


def test_chart_strict_kwarg():
    chart = Chart(strict=True)
    spec = json.dumps({
        "type": "bar",
        "data": {"labels": ["A"], "datasets": [{"data": [1]}]},
        "unknownKey": "value",
    })
    with pytest.raises(fulgur_chart.FulgurStrictError):
        chart.render(spec, "svg")


def test_chart_render_svg_method():
    chart = Chart()
    svg = chart.render_svg(CHARTJS_BAR)
    assert svg.startswith("<svg")


def test_chart_render_png_method():
    chart = Chart()
    png = chart.render_png(CHARTJS_BAR)
    assert isinstance(png, bytes)
    assert png[:4] == b"\x89PNG"


def test_chart_render_png_format():
    chart = Chart()
    png = chart.render(CHARTJS_BAR, "png")
    assert isinstance(png, bytes)
    assert png[:4] == b"\x89PNG"


def test_chart_render_unknown_format_raises_parse_error():
    chart = Chart()
    with pytest.raises(FulgurParseError):
        chart.render(CHARTJS_BAR, "jpeg")


def test_chart_reusable_multiple_renders():
    chart = Chart(width=400.0)
    svg1 = chart.render(CHARTJS_BAR, "svg")
    svg2 = chart.render(CHARTJS_BAR, "svg")
    assert svg1 == svg2


def test_chart_scale_kwarg():
    chart = Chart(scale=2.0)
    png = chart.render_png(CHARTJS_BAR)
    assert isinstance(png, bytes)
    assert png[:4] == b"\x89PNG"
