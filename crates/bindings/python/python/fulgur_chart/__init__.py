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


__all__ = [
    "FulgurParseError",
    "FulgurRenderError",
    "FulgurStrictError",
    "render_image",
    "render_png",
    "render_svg",
    "schema",
    "version",
]
