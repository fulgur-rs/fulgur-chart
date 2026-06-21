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
