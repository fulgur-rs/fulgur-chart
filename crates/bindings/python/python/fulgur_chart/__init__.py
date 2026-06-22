from .fulgur_chart import (
    FulgurParseError,
    FulgurRenderError,
    FulgurStrictError,
    render_image,
    render_svg,
    schema,
    version,
)


def render_png(
    spec_json: str,
    *,
    width=None,
    height=None,
    scale: float = 1.0,
    strict: bool = False,
    dsl=None,
    font=None,
) -> bytes:
    """PNG バイト列を返す（render_image(spec, 'png', ...) の短縮形）。"""
    return render_image(
        spec_json,
        "png",
        width=width,
        height=height,
        scale=scale,
        strict=strict,
        dsl=dsl,
        font=font,
    )


class ChartBuilder:
    """フルエントビルダー。Chart.builder() で取得する。

    pyfulgur の EngineBuilder に対応。setter はすべて self を返すためチェーン可能。
    build() は何度でも呼べる（純 Python dict を保持するだけで Rust リソースを転送しない）。
    """

    def __init__(self):
        self._opts = {}

    def width(self, value: float) -> "ChartBuilder":
        return self._set("width", value)

    def height(self, value: float) -> "ChartBuilder":
        return self._set("height", value)

    def scale(self, value: float) -> "ChartBuilder":
        return self._set("scale", value)

    def dsl(self, value: str) -> "ChartBuilder":
        return self._set("dsl", value)

    def font(self, value: bytes) -> "ChartBuilder":
        return self._set("font", value)

    def strict(self, value: bool = True) -> "ChartBuilder":
        return self._set("strict", value)

    def build(self) -> "Chart":
        return Chart(**self._opts)

    def _set(self, key, value):
        self._opts[key] = value
        return self


class Chart:
    """設定済みチャートレンダラー。

    pyfulgur の Engine に対応。設定はコンストラクタまたは Chart.builder() で与え、
    render(spec_json, fmt) でコンテンツ（spec_json）をレンダリングする。

    Usage::

        # コンストラクタ形式（シンプルケース）
        chart = Chart(width=800, dsl="chartjs")
        svg = chart.render(spec_json, "svg")

        # ビルダー形式（pyfulgur と同じメンタルモデル）
        chart = Chart.builder().width(800).dsl("chartjs").build()
        svg = chart.render(spec_json, "svg")
        png = chart.render(spec_json, "png")
    """

    def __init__(
        self,
        *,
        width=None,
        height=None,
        scale: float = 1.0,
        strict: bool = False,
        dsl=None,
        font=None,
    ):
        self._opts = {
            "scale": scale,
            "strict": strict,
        }
        if width is not None:
            self._opts["width"] = width
        if height is not None:
            self._opts["height"] = height
        if dsl is not None:
            self._opts["dsl"] = dsl
        if font is not None:
            self._opts["font"] = font

    @classmethod
    def builder(cls) -> ChartBuilder:
        """ChartBuilder を返す。Engine.builder() に相当。"""
        return ChartBuilder()

    def render(self, spec_json: str, fmt: str = "svg"):
        """spec_json を指定フォーマットでレンダリングする。

        fmt="svg" → str、fmt="png" → bytes。
        engine.render_html(html) に相当。
        """
        if fmt == "svg":
            return render_svg(spec_json, **self._opts)
        return render_image(spec_json, fmt, **self._opts)

    def render_svg(self, spec_json: str) -> str:
        """SVG 文字列を返すコンビニエンスメソッド。"""
        return render_svg(spec_json, **self._opts)

    def render_png(self, spec_json: str) -> bytes:
        """PNG バイト列を返すコンビニエンスメソッド。"""
        return render_image(spec_json, "png", **self._opts)


__all__ = [
    "Chart",
    "ChartBuilder",
    "FulgurParseError",
    "FulgurRenderError",
    "FulgurStrictError",
    "render_image",
    "render_png",
    "render_svg",
    "schema",
    "version",
]
