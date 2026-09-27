//! Vega-Lite inline GeoJSON rendering through the shared projection pipeline.

use super::common::{OUTER_PAD, TITLE_BAND, TITLE_FONT};
use crate::geoshape::{ProjectedGeometry, project_features};
use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Anchor, ClipRect, Prim, Scene};
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _measurer: &TextMeasurer) -> Result<Scene, String> {
    let ChartKind::GeoShape { data } = &spec.kind else {
        unreachable!("geoshape::build called on non-geoshape kind");
    };

    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let viewport = ClipRect {
        x: OUTER_PAD,
        y: OUTER_PAD + title_band,
        w: (spec.width - 2.0 * OUTER_PAD).max(0.0),
        h: (spec.height - 2.0 * OUTER_PAD - title_band).max(0.0),
    };
    let mut items = Vec::new();
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: spec.theme.text_color,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    let features = project_features(data, viewport)?;
    for feature in features {
        let fill = feature.fill.or(data.style.fill);
        for geometry in feature.geometries {
            match geometry {
                ProjectedGeometry::Path { d, fillable } if !d.is_empty() => {
                    let fill = fillable.then_some(fill).flatten();
                    let stroke = data.style.stroke;
                    if let Some(clip) = feature.clip {
                        items.push(Prim::ClippedPath {
                            d,
                            fill,
                            stroke,
                            stroke_width: data.style.stroke_width,
                            clip: Box::new(clip),
                        });
                    } else {
                        items.push(Prim::Path {
                            d,
                            fill,
                            stroke,
                            stroke_width: data.style.stroke_width,
                        });
                    }
                }
                ProjectedGeometry::Point { x, y } => {
                    let clip_radius =
                        data.projection.point_radius + data.style.stroke_width.max(0.0) / 2.0;
                    if feature.clip.is_some_and(|clip| {
                        x + clip_radius < clip.x
                            || x - clip_radius > clip.x + clip.w
                            || y + clip_radius < clip.y
                            || y - clip_radius > clip.y + clip.h
                    }) {
                        continue;
                    }
                    let fill = fill.unwrap_or(crate::ir::Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0.0,
                    });
                    let stroke = data.style.stroke.unwrap_or(fill);
                    if let Some(clip) = feature.clip {
                        items.push(Prim::ClippedCircle {
                            cx: x,
                            cy: y,
                            r: data.projection.point_radius,
                            fill,
                            stroke,
                            stroke_width: data.style.stroke_width,
                            clip: Box::new(clip),
                        });
                    } else {
                        items.push(Prim::Circle {
                            cx: x,
                            cy: y,
                            r: data.projection.point_radius,
                            fill,
                            stroke,
                            stroke_width: data.style.stroke_width,
                        });
                    }
                }
                ProjectedGeometry::Path { .. } => {}
            }
        }
    }

    Ok(Scene {
        width: spec.width,
        height: spec.height,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Color, GeoFeature, GeoGeometry, GeoProjection, GeoProjectionType, GeoShape, GeoShapeStyle,
    };

    fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> GeoGeometry {
        GeoGeometry::Polygon(vec![vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]]])
    }

    fn color(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    fn projected_coordinates(path: &str) -> Vec<Vec<[f64; 2]>> {
        let mut paths = Vec::new();
        let mut current = Vec::new();
        let mut pair = Vec::new();
        for token in path.split_whitespace() {
            if matches!(token, "M" | "L" | "Z") {
                if pair.len() == 2 {
                    current.push([pair[0], pair[1]]);
                    pair.clear();
                }
                if (token == "M" || token == "Z") && !current.is_empty() {
                    paths.push(std::mem::take(&mut current));
                }
            } else {
                pair.push(token.parse::<f64>().unwrap());
            }
        }
        if pair.len() == 2 {
            current.push([pair[0], pair[1]]);
        }
        if !current.is_empty() {
            paths.push(current);
        }
        paths
    }

    fn area(ring: &[[f64; 2]]) -> f64 {
        ring.iter()
            .zip(ring.iter().cycle().skip(1))
            .take(ring.len())
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum()
    }

    #[test]
    fn auto_fit_contains_all_features_with_margin() {
        let shape = GeoShape {
            features: vec![
                GeoFeature {
                    geometry: Some(rectangle(-80.0, -20.0, -60.0, 0.0)),
                    fill: None,
                },
                GeoFeature {
                    geometry: Some(rectangle(40.0, 15.0, 70.0, 40.0)),
                    fill: None,
                },
            ],
            projection: GeoProjection::default(),
            style: GeoShapeStyle::default(),
        };
        let viewport = ClipRect {
            x: 10.0,
            y: 20.0,
            w: 300.0,
            h: 200.0,
        };
        let projected = project_features(&shape, viewport).unwrap();
        let coordinates = projected
            .iter()
            .flat_map(|feature| &feature.geometries)
            .filter_map(|geometry| match geometry {
                ProjectedGeometry::Path { d, .. } => Some(projected_coordinates(d)),
                ProjectedGeometry::Point { .. } => None,
            })
            .flatten()
            .flatten()
            .collect::<Vec<_>>();
        assert!(!coordinates.is_empty());
        assert!(
            coordinates.iter().all(|[x, y]| {
                *x >= viewport.x + 8.0 - 1e-6
                    && *x <= viewport.x + viewport.w - 8.0 + 1e-6
                    && *y >= viewport.y + 8.0 - 1e-6
                    && *y <= viewport.y + viewport.h - 8.0 + 1e-6
            }),
            "projected bounds: {:?}",
            coordinates
        );
    }

    #[test]
    fn explicit_translate_keeps_automatic_scale_fit() {
        let shape = GeoShape {
            features: vec![GeoFeature {
                geometry: Some(rectangle(-40.0, -15.0, 40.0, 15.0)),
                fill: None,
            }],
            projection: GeoProjection {
                projection_type: GeoProjectionType::Equirectangular,
                ..GeoProjection::default()
            },
            style: GeoShapeStyle::default(),
        };
        let viewport = ClipRect {
            x: 0.0,
            y: 0.0,
            w: 320.0,
            h: 200.0,
        };
        let default_fit = project_features(&shape, viewport).unwrap();
        let mut translated_shape = shape;
        translated_shape.projection.translate = Some([120.0, 70.0]);
        let explicit_translate = project_features(&translated_shape, viewport).unwrap();
        let bounds = |features: &[crate::geoshape::ProjectedFeature]| {
            let points = features
                .iter()
                .flat_map(|feature| &feature.geometries)
                .filter_map(|geometry| match geometry {
                    ProjectedGeometry::Path { d, .. } => Some(projected_coordinates(d)),
                    ProjectedGeometry::Point { .. } => None,
                })
                .flatten()
                .flatten()
                .collect::<Vec<_>>();
            let min_x = points
                .iter()
                .map(|point| point[0])
                .fold(f64::INFINITY, f64::min);
            let max_x = points
                .iter()
                .map(|point| point[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = points
                .iter()
                .map(|point| point[1])
                .fold(f64::INFINITY, f64::min);
            let max_y = points
                .iter()
                .map(|point| point[1])
                .fold(f64::NEG_INFINITY, f64::max);
            [min_x, min_y, max_x, max_y]
        };
        let default_bounds = bounds(&default_fit);
        let translated_bounds = bounds(&explicit_translate);
        assert!(
            (default_bounds[2] - default_bounds[0] - (translated_bounds[2] - translated_bounds[0]))
                .abs()
                < 1e-6
        );
        assert!(
            (default_bounds[3] - default_bounds[1] - (translated_bounds[3] - translated_bounds[1]))
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn normalizes_polygon_holes_and_preserves_feature_order() {
        let outer = vec![
            [-5.0, -5.0],
            [5.0, -5.0],
            [5.0, 5.0],
            [-5.0, 5.0],
            [-5.0, -5.0],
        ];
        let hole = vec![
            [-2.0, -2.0],
            [2.0, -2.0],
            [2.0, 2.0],
            [-2.0, 2.0],
            [-2.0, -2.0],
        ];
        let shape = GeoShape {
            features: vec![
                GeoFeature {
                    geometry: Some(GeoGeometry::Polygon(vec![outer, hole])),
                    fill: Some(color(220, 10, 10)),
                },
                GeoFeature {
                    geometry: Some(rectangle(10.0, 0.0, 15.0, 5.0)),
                    fill: Some(color(10, 20, 220)),
                },
            ],
            projection: GeoProjection {
                projection_type: GeoProjectionType::EqualEarth,
                ..GeoProjection::default()
            },
            style: GeoShapeStyle::default(),
        };
        let projected = project_features(
            &shape,
            ClipRect {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 200.0,
            },
        )
        .unwrap();
        assert_eq!(
            projected
                .iter()
                .map(|feature| feature.fill)
                .collect::<Vec<_>>(),
            [Some(color(220, 10, 10)), Some(color(10, 20, 220))]
        );
        let ProjectedGeometry::Path { d, fillable: true } = &projected[0].geometries[0] else {
            panic!("first feature should be a filled polygon path");
        };
        let rings = projected_coordinates(d);
        assert_eq!(rings.len(), 2, "outer ring and hole should remain separate");
        assert_ne!(
            area(&rings[0]).is_sign_positive(),
            area(&rings[1]).is_sign_positive()
        );
    }

    #[test]
    fn projects_every_geojson_geometry_kind() {
        let line = vec![[-2.0, -2.0], [2.0, 2.0]];
        let polygon = vec![vec![[-3.0, -3.0], [3.0, -3.0], [3.0, 3.0], [-3.0, -3.0]]];
        let shape = GeoShape {
            features: vec![GeoFeature {
                geometry: Some(GeoGeometry::GeometryCollection(vec![
                    GeoGeometry::Point([0.0, 0.0]),
                    GeoGeometry::MultiPoint(vec![[1.0, 0.0], [2.0, 0.0]]),
                    GeoGeometry::LineString(line.clone()),
                    GeoGeometry::MultiLineString(vec![line]),
                    GeoGeometry::Polygon(polygon.clone()),
                    GeoGeometry::MultiPolygon(vec![polygon]),
                    GeoGeometry::GeometryCollection(vec![GeoGeometry::Point([-1.0, 0.0])]),
                ])),
                fill: None,
            }],
            projection: GeoProjection::default(),
            style: GeoShapeStyle::default(),
        };
        let projected = project_features(
            &shape,
            ClipRect {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 200.0,
            },
        )
        .unwrap();
        let geometries = &projected[0].geometries;
        let point_count = geometries
            .iter()
            .filter(|geometry| matches!(geometry, ProjectedGeometry::Point { .. }))
            .count();
        assert!(point_count >= 4, "expected all points, got {geometries:?}");
        assert!(
            geometries
                .iter()
                .filter(|geometry| matches!(
                    geometry,
                    ProjectedGeometry::Path {
                        fillable: false,
                        ..
                    }
                ))
                .count()
                >= 2
        );
        assert!(
            geometries
                .iter()
                .filter(|geometry| matches!(
                    geometry,
                    ProjectedGeometry::Path { fillable: true, .. }
                ))
                .count()
                >= 2
        );
    }

    #[test]
    fn builds_no_axis_scene_with_title_paths_and_points() {
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        spec.kind = ChartKind::GeoShape {
            data: Box::new(GeoShape {
                features: vec![
                    GeoFeature {
                        geometry: Some(rectangle(-5.0, -5.0, 5.0, 5.0)),
                        fill: Some(color(20, 120, 210)),
                    },
                    GeoFeature {
                        geometry: Some(GeoGeometry::Point([10.0, 0.0])),
                        fill: None,
                    },
                    GeoFeature {
                        geometry: Some(GeoGeometry::LineString(vec![
                            [-10.0, 0.0],
                            [0.0, 5.0],
                            [10.0, 0.0],
                        ])),
                        fill: Some(color(200, 20, 20)),
                    },
                ],
                projection: GeoProjection::default(),
                style: GeoShapeStyle {
                    fill: Some(color(100, 160, 210)),
                    stroke: Some(color(20, 20, 20)),
                    stroke_width: 1.0,
                },
            }),
        };
        spec.series.clear();
        spec.categories.clear();
        spec.title = Some("Map".to_string());
        spec.width = 320.0;
        spec.height = 200.0;
        let measurer = TextMeasurer::new(crate::font::TEST_FONT).unwrap();
        let scene = build(&spec, &measurer).unwrap();
        assert_eq!((scene.width, scene.height), (320.0, 200.0));
        assert!(scene.items.iter().any(|item| matches!(
            item,
            Prim::Text { content, .. } if content == "Map"
        )));
        assert!(
            scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::Path { .. }))
        );
        assert!(
            scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::Circle { .. }))
        );
        assert!(scene.items.iter().any(|item| matches!(
            item,
            Prim::Path { d, fill: None, .. } if !d.split_whitespace().any(|token| token == "Z")
        )));
        assert!(
            !scene
                .items
                .iter()
                .any(|item| matches!(item, Prim::Line { .. }))
        );
    }

    #[test]
    fn missing_point_fill_is_transparent_and_keeps_the_feature_stroke() {
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        let stroke = color(10, 20, 30);
        spec.kind = ChartKind::GeoShape {
            data: Box::new(GeoShape {
                features: vec![GeoFeature {
                    geometry: Some(GeoGeometry::Point([0.0, 0.0])),
                    fill: None,
                }],
                projection: GeoProjection::default(),
                style: GeoShapeStyle {
                    fill: None,
                    stroke: Some(stroke),
                    stroke_width: 1.0,
                },
            }),
        };
        spec.series.clear();
        spec.categories.clear();
        let measurer = TextMeasurer::new(crate::font::TEST_FONT).unwrap();
        let scene = build(&spec, &measurer).unwrap();
        assert!(scene.items.iter().any(|item| matches!(
            item,
            Prim::Circle { fill, stroke: actual_stroke, .. }
                if fill.a == 0.0 && *actual_stroke == stroke
        )));
    }

    #[test]
    fn point_radius_is_clipped_to_projection_clip_extent() {
        let mut spec = crate::frontend::chartjs::parse(
            r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
            false,
        )
        .unwrap();
        spec.kind = ChartKind::GeoShape {
            data: Box::new(GeoShape {
                features: vec![GeoFeature {
                    geometry: Some(GeoGeometry::Point([0.0, 0.0])),
                    fill: Some(color(20, 120, 210)),
                }],
                projection: GeoProjection {
                    clip_extent: Some([[160.0, 90.0], [180.0, 110.0]]),
                    ..GeoProjection::default()
                },
                style: GeoShapeStyle::default(),
            }),
        };
        spec.series.clear();
        spec.categories.clear();
        spec.width = 320.0;
        spec.height = 200.0;
        let measurer = TextMeasurer::new(crate::font::TEST_FONT).unwrap();
        let scene = build(&spec, &measurer).unwrap();
        let svg = crate::svg::render_svg(&scene, "sans-serif");
        assert!(
            svg.contains("clip-path=\"url(#clip0)\""),
            "a point radius crossing clipExtent must be clipped in the renderer: {svg}"
        );
        let png =
            crate::raster_direct::render_chart_to_png(&spec, 1.0, crate::font::TEST_FONT).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut bytes = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut bytes).unwrap();
        let alpha_at = |x: usize, y: usize| bytes[(y * info.width as usize + x) * 4 + 3];
        assert_eq!(alpha_at(159, 100), 0);
        assert!(alpha_at(161, 100) > 0);
    }
}
