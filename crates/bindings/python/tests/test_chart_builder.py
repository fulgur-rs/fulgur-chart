"""ChartBuilder のテスト — pyfulgur の test_engine_builder.py に対応。"""
import json

import pytest

from fulgur_chart import Chart, FulgurStrictError

CHARTJS_BAR = json.dumps({
    "type": "bar",
    "data": {
        "labels": ["A", "B"],
        "datasets": [{"data": [1, 2]}],
    },
})


def test_builder_returns_chart():
    chart = Chart.builder().build()
    assert chart is not None


def test_builder_width_height():
    chart = Chart.builder().width(400.0).height(300.0).build()
    svg = chart.render(CHARTJS_BAR, "svg")
    assert svg.startswith("<svg")


def test_builder_dsl():
    chart = Chart.builder().dsl("chartjs").build()
    svg = chart.render(CHARTJS_BAR, "svg")
    assert svg.startswith("<svg")


def test_builder_strict():
    chart = Chart.builder().strict(True).build()
    spec = json.dumps({
        "type": "bar",
        "data": {"labels": ["A"], "datasets": [{"data": [1]}]},
        "unknownKey": "value",
    })
    with pytest.raises(FulgurStrictError):
        chart.render(spec, "svg")


def test_builder_strict_no_arg_defaults_true():
    chart = Chart.builder().strict().build()
    spec = json.dumps({
        "type": "bar",
        "data": {"labels": ["A"], "datasets": [{"data": [1]}]},
        "unknownKey": "value",
    })
    with pytest.raises(FulgurStrictError):
        chart.render(spec, "svg")


def test_builder_scale():
    chart = Chart.builder().scale(2.0).build()
    png = chart.render(CHARTJS_BAR, "png")
    assert isinstance(png, bytes)
    assert png[:4] == b"\x89PNG"


def test_builder_chaining():
    chart = (
        Chart.builder()
        .width(800.0)
        .height(600.0)
        .dsl("chartjs")
        .scale(1.5)
        .build()
    )
    svg = chart.render(CHARTJS_BAR, "svg")
    assert svg.startswith("<svg")


def test_builder_is_reusable_after_build():
    """pyfulgur と異なり builder は消費されない — pure Python dict なのでリソース移転がない。"""
    b = Chart.builder().width(400.0)
    chart1 = b.build()
    chart2 = b.build()
    assert chart1 is not chart2
    svg1 = chart1.render(CHARTJS_BAR, "svg")
    svg2 = chart2.render(CHARTJS_BAR, "svg")
    assert svg1 == svg2


def test_built_chart_is_reusable():
    chart = Chart.builder().build()
    svg1 = chart.render(CHARTJS_BAR, "svg")
    svg2 = chart.render(CHARTJS_BAR, "svg")
    assert svg1 == svg2
