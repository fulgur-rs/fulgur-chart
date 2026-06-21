import json

import fulgur_chart


# ── version ──────────────────────────────────────────────────────────

def test_version_returns_string():
    v = fulgur_chart.version()
    assert isinstance(v, str)
    assert len(v) > 0


def test_version_is_semver():
    v = fulgur_chart.version()
    parts = v.split(".")
    assert len(parts) == 3, f"期待: X.Y.Z 形式, 実際: {v}"


# ── schema ───────────────────────────────────────────────────────────

def test_schema_chartjs_is_valid_json():
    s = fulgur_chart.schema("chartjs")
    parsed = json.loads(s)
    assert isinstance(parsed, dict)


def test_schema_vegalite_is_valid_json():
    s = fulgur_chart.schema("vegalite")
    parsed = json.loads(s)
    assert isinstance(parsed, dict)


def test_schema_unknown_dsl_raises_parse_error():
    try:
        fulgur_chart.schema("unknown")
        assert False, "例外が送出されなかった"
    except fulgur_chart.FulgurParseError:
        pass


# ── テスト用フィクスチャ ───────────────────────────────────────────

CHARTJS_BAR = json.dumps({
    "type": "bar",
    "data": {
        "labels": ["A", "B"],
        "datasets": [{"data": [1, 2]}],
    },
})

VEGALITE_POINT = json.dumps({
    "mark": "point",
    "data": {"values": [{"x": 1, "y": 2}]},
    "encoding": {
        "x": {"field": "x", "type": "quantitative"},
        "y": {"field": "y", "type": "quantitative"},
    },
})


# ── render_svg ───────────────────────────────────────────────────────

def test_render_svg_chartjs_starts_with_svg_tag():
    svg = fulgur_chart.render_svg(CHARTJS_BAR)
    assert svg.startswith("<svg"), svg[:80]


def test_render_svg_vegalite_starts_with_svg_tag():
    svg = fulgur_chart.render_svg(VEGALITE_POINT)
    assert svg.startswith("<svg"), svg[:80]


def test_render_svg_is_deterministic():
    a = fulgur_chart.render_svg(CHARTJS_BAR)
    b = fulgur_chart.render_svg(CHARTJS_BAR)
    assert a == b


def test_render_svg_with_width_height_overrides():
    # width/height を指定してもクラッシュしない
    svg = fulgur_chart.render_svg(CHARTJS_BAR, width=400.0, height=300.0)
    assert svg.startswith("<svg")


def test_render_svg_invalid_json_raises_parse_error():
    try:
        fulgur_chart.render_svg("not json")
        assert False, "例外が送出されなかった"
    except fulgur_chart.FulgurParseError:
        pass


def test_render_svg_strict_unknown_key_raises_strict_error():
    spec = json.dumps({
        "type": "bar",
        "data": {"labels": ["A"], "datasets": [{"data": [1]}]},
        "unknownKey": "value",
    })
    try:
        fulgur_chart.render_svg(spec, strict=True)
        assert False, "strict モードで例外が送出されなかった"
    except fulgur_chart.FulgurStrictError:
        pass


def test_strict_error_is_subclass_of_parse_error():
    assert issubclass(fulgur_chart.FulgurStrictError, fulgur_chart.FulgurParseError)


def test_parse_error_is_subclass_of_value_error():
    assert issubclass(fulgur_chart.FulgurParseError, ValueError)
