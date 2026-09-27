use crate::guard::InputLimits;
use crate::ir::{GeoGeometry, GeoProjection, GeoProjectionType, GeoShape};
use crate::num::fmt_num;
use crate::scene::ClipRect;
use d3_geo_rs::projection::{
    Build, BuilderTrait, CenterSet, ClipAngleAdjust, ClipAngleSet, PrecisionAdjust, Projector,
    RawBase, RotateSet, ScaleGet, ScaleSet, TranslateGet, TranslateSet,
};
use d3_geo_rs::stream::{Stream, Streamable};
use geo_types::{
    Coord, Geometry, GeometryCollection, LineString, MultiLineString, MultiPoint, MultiPolygon,
    Point, Polygon,
};
use serde_json::{Map, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

const MAX_GEOMETRY_COLLECTION_DEPTH: usize = 64;

/// GeoJSON geometry paired with its source record and feature properties.
#[derive(Clone, Debug, PartialEq)]
pub struct RawGeoFeature {
    pub geometry: Option<GeoGeometry>,
    pub record: Arc<Map<String, Value>>,
    pub properties: Map<String, Value>,
}

#[derive(Default)]
struct Counts {
    features: usize,
    vertices: usize,
    primitives: usize,
}

const MAX_PROJECTED_GEO_VERTICES: usize = 1_000_000;
const MAX_PROJECTED_GEO_PRIMITIVES: usize = 1_000_000;
const MAX_RESAMPLED_VERTICES_PER_SEGMENT: usize = 1 << 16;

#[derive(Clone, Debug, PartialEq)]
struct ProjectionBudget {
    vertices: Rc<Cell<usize>>,
    primitives: Rc<Cell<usize>>,
}

impl Default for ProjectionBudget {
    fn default() -> Self {
        Self {
            vertices: Rc::new(Cell::new(0)),
            primitives: Rc::new(Cell::new(0)),
        }
    }
}

impl ProjectionBudget {
    fn add_vertex(&self) -> Result<(), String> {
        let count = self.vertices.get();
        if count >= MAX_PROJECTED_GEO_VERTICES {
            return Err(format!(
                "projected coordinate vertex count exceeds limit {MAX_PROJECTED_GEO_VERTICES}"
            ));
        }
        self.vertices.set(count + 1);
        Ok(())
    }

    fn add_primitive(&self) -> Result<(), String> {
        let count = self.primitives.get();
        if count >= MAX_PROJECTED_GEO_PRIMITIVES {
            return Err(format!(
                "projected primitive count exceeds limit {MAX_PROJECTED_GEO_PRIMITIVES}"
            ));
        }
        self.primitives.set(count + 1);
        Ok(())
    }
}

/// One projected feature. Its subpaths retain polygon fill eligibility for mixed collections.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedFeature {
    pub geometries: Vec<ProjectedGeometry>,
    pub fill: Option<crate::ir::Color>,
    pub clip: Option<ClipRect>,
}

/// A path or point ready for the common Scene layout.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectedGeometry {
    Path { d: String, fillable: bool },
    Point { x: f64, y: f64 },
}

#[derive(Clone, Debug, PartialEq)]
enum RawProjectedGeometry {
    Path {
        subpaths: Vec<Vec<[f64; 2]>>,
        fillable: bool,
    },
    Point([f64; 2]),
}

/// Endpoint for d3-geo's clipping/resampling pipeline.
#[derive(Clone, Debug, Default, PartialEq)]
struct GeoPathEndpoint {
    output: Vec<RawProjectedGeometry>,
    current_line: Vec<[f64; 2]>,
    polygon_rings: Vec<Vec<[f64; 2]>>,
    in_polygon: bool,
    in_line: bool,
    error: Option<String>,
    budget: ProjectionBudget,
}

impl Stream for GeoPathEndpoint {
    type EP = Self;
    type T = f64;

    fn endpoint(&mut self) -> &mut Self::EP {
        self
    }

    fn line_start(&mut self) {
        self.current_line.clear();
        self.in_line = true;
    }

    fn point(&mut self, point: &Coord<f64>, _marker: Option<u8>) {
        if self.error.is_some() {
            return;
        }
        if !point.x.is_finite() || !point.y.is_finite() {
            self.error = Some(format!(
                "projection produced a non-finite coordinate ({}, {})",
                point.x, point.y
            ));
            return;
        }
        if let Err(error) = self.budget.add_vertex() {
            self.error = Some(error);
            return;
        }
        let point = [point.x, point.y];
        if self.in_line {
            self.current_line.push(point);
        } else {
            if let Err(error) = self.budget.add_primitive() {
                self.error = Some(error);
                return;
            }
            self.output.push(RawProjectedGeometry::Point(point));
        }
    }

    fn line_end(&mut self) {
        self.in_line = false;
        let line = std::mem::take(&mut self.current_line);
        if self.error.is_some() {
            return;
        }
        if self.in_polygon {
            if !line.is_empty() {
                self.polygon_rings.push(line);
            }
        } else if line.len() >= 2 {
            if let Err(error) = self.budget.add_primitive() {
                self.error = Some(error);
                return;
            }
            self.output.push(RawProjectedGeometry::Path {
                subpaths: vec![line],
                fillable: false,
            });
        }
    }

    fn polygon_start(&mut self) {
        self.in_polygon = true;
        self.polygon_rings.clear();
    }

    fn polygon_end(&mut self) {
        self.in_polygon = false;
        if self.error.is_some() {
            self.polygon_rings.clear();
            return;
        }
        if self.polygon_rings.is_empty() {
            return;
        }
        if let Err(error) = self.budget.add_primitive() {
            self.error = Some(error);
            return;
        }
        for (index, ring) in self.polygon_rings.iter_mut().enumerate() {
            normalize_ring_winding(ring, index == 0);
        }
        self.output.push(RawProjectedGeometry::Path {
            subpaths: std::mem::take(&mut self.polygon_rings),
            fillable: true,
        });
    }
}

fn normalize_ring_winding(ring: &mut [[f64; 2]], positive: bool) {
    if ring.len() < 3 {
        return;
    }
    let area = ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>();
    if (area > 0.0) != positive {
        ring.reverse();
    }
}

/// Project all features using one fit computed from the complete input collection.
pub fn project_features(
    shape: &GeoShape,
    viewport: ClipRect,
) -> Result<Vec<ProjectedFeature>, String> {
    if !viewport.x.is_finite()
        || !viewport.y.is_finite()
        || !viewport.w.is_finite()
        || !viewport.h.is_finite()
        || viewport.w < 0.0
        || viewport.h < 0.0
    {
        return Err("geoshape viewport must have finite non-negative dimensions".to_string());
    }

    check_zero_precision_resample_budget(shape)?;
    let budget = ProjectionBudget::default();
    let mut projected = Vec::with_capacity(shape.features.len());
    let mut default_scale = None;
    let mut default_translate = None;
    for feature in &shape.features {
        let mut geometries = Vec::new();
        if let Some(geometry) = &feature.geometry {
            let output = project_geometry(&shape.projection, geometry, &budget)?;
            if default_scale.is_none() {
                default_scale = Some(output.default_scale);
                default_translate = Some(output.default_translate);
            }
            geometries = output.geometries;
        }
        projected.push(ProjectedFeature {
            geometries: geometries
                .into_iter()
                .map(ProjectedGeometry::from_raw)
                .collect(),
            fill: feature.fill,
            clip: None,
        });
    }

    let Some(default_scale) = default_scale else {
        return Ok(projected);
    };
    let default_translate = default_translate.unwrap_or([0.0, 0.0]);
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for feature in &projected {
        for geometry in &feature.geometries {
            match geometry {
                ProjectedGeometry::Path { d, .. } => {
                    for (x, y) in path_coordinates(d)? {
                        extend_bounds(&mut bounds, x, y);
                    }
                }
                ProjectedGeometry::Point { x, y } => extend_bounds(&mut bounds, *x, *y),
            }
        }
    }
    if !bounds[0].is_finite() {
        return Ok(projected);
    }

    let auto_scale = shape.projection.scale.is_none();
    let has_points = projected.iter().any(|feature| {
        feature
            .geometries
            .iter()
            .any(|geometry| matches!(geometry, ProjectedGeometry::Point { .. }))
    });
    let fit_margin =
        8.0 + if has_points {
            shape.projection.point_radius
        } else {
            0.0
        } + shape.style.stroke_width.max(0.0) / 2.0;
    let factor = if auto_scale {
        let available_width = (viewport.w - 2.0 * fit_margin).max(0.0);
        let available_height = (viewport.h - 2.0 * fit_margin).max(0.0);
        let width = bounds[2] - bounds[0];
        let height = bounds[3] - bounds[1];
        let x_factor = if width > 0.0 {
            available_width / width
        } else {
            f64::INFINITY
        };
        let y_factor = if height > 0.0 {
            available_height / height
        } else {
            f64::INFINITY
        };
        x_factor.min(y_factor).min(1.0e9)
    } else {
        shape.projection.scale.unwrap_or(default_scale) / default_scale
    };
    if !factor.is_finite() || factor < 0.0 {
        return Err("projection scale must be finite and non-negative".to_string());
    }
    let fitted_translate = if auto_scale && shape.projection.translate.is_none() {
        [
            viewport.x + viewport.w / 2.0
                - ((bounds[0] + bounds[2]) / 2.0 - default_translate[0]) * factor,
            viewport.y + viewport.h / 2.0
                - ((bounds[1] + bounds[3]) / 2.0 - default_translate[1]) * factor,
        ]
    } else {
        shape
            .projection
            .translate
            .unwrap_or([viewport.x + viewport.w / 2.0, viewport.y + viewport.h / 2.0])
    };
    let clip = projection_clip(shape.projection.clip_extent, viewport);
    for feature in &mut projected {
        for geometry in &mut feature.geometries {
            match geometry {
                ProjectedGeometry::Path { d, .. } => {
                    *d = transform_path(d, default_translate, factor, fitted_translate)?;
                }
                ProjectedGeometry::Point { x, y } => {
                    *x = (*x - default_translate[0]) * factor + fitted_translate[0];
                    *y = (*y - default_translate[1]) * factor + fitted_translate[1];
                    if !x.is_finite() || !y.is_finite() {
                        return Err("projection produced a non-finite coordinate".to_string());
                    }
                }
            }
        }
        feature.clip = clip;
    }
    Ok(projected)
}

fn check_zero_precision_resample_budget(shape: &GeoShape) -> Result<(), String> {
    if shape.projection.projection_type == GeoProjectionType::Identity
        || shape.projection.precision != Some(0.0)
    {
        return Ok(());
    }

    let mut source_segments = 0usize;
    let mut pending = shape
        .features
        .iter()
        .filter_map(|feature| feature.geometry.as_ref())
        .collect::<Vec<_>>();
    while let Some(geometry) = pending.pop() {
        let segments = match geometry {
            GeoGeometry::Point(_) | GeoGeometry::MultiPoint(_) => 0,
            GeoGeometry::LineString(points) => points.len().saturating_sub(1),
            GeoGeometry::MultiLineString(lines) | GeoGeometry::Polygon(lines) => {
                lines.iter().map(|line| line.len().saturating_sub(1)).sum()
            }
            GeoGeometry::MultiPolygon(polygons) => polygons
                .iter()
                .flatten()
                .map(|ring| ring.len().saturating_sub(1))
                .sum(),
            GeoGeometry::GeometryCollection(geometries) => {
                pending.extend(geometries);
                0
            }
        };
        source_segments = source_segments.saturating_add(segments);
    }

    let projection_copies = if shape.projection.projection_type == GeoProjectionType::AlbersUsa {
        3
    } else {
        1
    };
    let possible_vertices = source_segments
        .saturating_mul(MAX_RESAMPLED_VERTICES_PER_SEGMENT)
        .saturating_mul(projection_copies);
    if possible_vertices > MAX_PROJECTED_GEO_VERTICES {
        return Err(format!(
            "projection precision 0 may exceed projected coordinate vertex limit {MAX_PROJECTED_GEO_VERTICES}; increase precision or simplify the geometry"
        ));
    }
    Ok(())
}

impl ProjectedGeometry {
    fn from_raw(raw: RawProjectedGeometry) -> Self {
        match raw {
            RawProjectedGeometry::Point([x, y]) => Self::Point { x, y },
            RawProjectedGeometry::Path { subpaths, fillable } => Self::Path {
                d: write_path(&subpaths, fillable),
                fillable,
            },
        }
    }
}

struct GeometryProjection {
    geometries: Vec<RawProjectedGeometry>,
    default_scale: f64,
    default_translate: [f64; 2],
}

fn project_geometry(
    projection: &GeoProjection,
    geometry: &GeoGeometry,
    budget: &ProjectionBudget,
) -> Result<GeometryProjection, String> {
    if projection.projection_type == GeoProjectionType::Identity {
        return Ok(GeometryProjection {
            geometries: project_identity(geometry, projection, budget)?,
            default_scale: 1.0,
            default_translate: [0.0, 0.0],
        });
    }
    if projection.projection_type == GeoProjectionType::AlbersUsa {
        return project_albers_usa(projection, geometry, budget);
    }

    macro_rules! run_projection {
        ($builder:expr) => {{
            let mut builder = $builder;
            configure_builder(&mut builder, projection)?;
            configure_precision(&mut builder, projection)?;
            if let Some(angle) = projection.clip_angle {
                let clipped = ClipAngleSet::clip_angle_set(&builder, angle);
                project_with_builder(&clipped, geometry, budget)
            } else {
                project_with_builder(&builder, geometry, budget)
            }
        }};
    }
    macro_rules! run_circle_projection {
        ($builder:expr) => {{
            let mut builder = $builder;
            configure_builder(&mut builder, projection)?;
            configure_precision(&mut builder, projection)?;
            if let Some(angle) = projection.clip_angle {
                ClipAngleAdjust::clip_angle(&mut builder, angle);
            }
            project_with_builder(&builder, geometry, budget)
        }};
    }
    use GeoProjectionType as P;
    match projection.projection_type {
        P::Albers => project_conic_raw::<d3_geo_rs::projection::equal_area::EqualArea<f64>>(
            geometry,
            projection,
            budget,
            [29.5, 45.5],
            1070.0,
            [-0.6, 38.7],
            [96.0, 0.0],
        ),
        P::AzimuthalEqualArea => run_circle_projection!(
            d3_geo_rs::projection::azimuthal_equal_area::AzimuthalEqualArea::<f64>::builder::<
                GeoPathEndpoint,
            >()
        ),
        P::AzimuthalEquidistant => run_circle_projection!(
            d3_geo_rs::projection::azimuthal_equidistant::AzimuthalEquiDistant::<f64>::builder::<
                GeoPathEndpoint,
            >()
        ),
        P::ConicConformal => project_conic_raw::<d3_geo_rs::projection::conformal::Conformal>(
            geometry,
            projection,
            budget,
            [30.0, 30.0],
            109.5,
            [0.0, 0.0],
            [0.0, 0.0],
        ),
        P::ConicEqualArea => {
            project_conic_raw::<d3_geo_rs::projection::equal_area::EqualArea<f64>>(
                geometry,
                projection,
                budget,
                [0.0, 60.0],
                155.424,
                [0.0, 33.6442],
                [0.0, 0.0],
            )
        }
        P::ConicEquidistant => {
            project_conic_raw::<d3_geo_rs::projection::equidistant::Equidistant>(
                geometry,
                projection,
                budget,
                [0.0, 60.0],
                131.154,
                [0.0, 13.9389],
                [0.0, 0.0],
            )
        }
        P::EqualEarth => run_projection!(
            d3_geo_rs::projection::equal_earth::EqualEarth::<f64>::builder::<GeoPathEndpoint>()
        ),
        P::Equirectangular => run_projection!(
            d3_geo_rs::projection::equirectangular::Equirectangular::<f64>::builder::<
                GeoPathEndpoint,
            >()
        ),
        P::Gnomonic => {
            run_circle_projection!(d3_geo_rs::projection::gnomic::Gnomic::<f64>::builder::<
                GeoPathEndpoint,
            >())
        }
        P::Mercator => run_projection!(d3_geo_rs::projection::mercator::Mercator::builder::<
            GeoPathEndpoint,
        >()),
        P::NaturalEarth1 => run_projection!(NaturalEarth1::builder::<GeoPathEndpoint>()),
        P::Orthographic => run_circle_projection!(
            d3_geo_rs::projection::orthographic::Orthographic::<f64>::builder::<GeoPathEndpoint>()
        ),
        P::Stereographic => run_circle_projection!(
            d3_geo_rs::projection::stereographic::Stereographic::<f64>::builder::<GeoPathEndpoint>(
            )
        ),
        P::TransverseMercator => project_transverse_mercator(geometry, projection, budget),
        P::AlbersUsa | P::Identity => unreachable!("handled above"),
    }
}

fn project_transverse_mercator(
    geometry: &GeoGeometry,
    projection: &GeoProjection,
    budget: &ProjectionBudget,
) -> Result<GeometryProjection, String> {
    let mut builder = d3_geo_rs::projection::mercator_transverse::MercatorTransverse::builder::<
        GeoPathEndpoint,
    >();
    if let Some(angle) = projection.clip_angle {
        let base = builder.base.clip_angle_set(angle);
        let mut clipped_builder =
            d3_geo_rs::projection::builder_mercator_transverse::Builder { base };
        configure_builder(&mut clipped_builder, projection)?;
        configure_precision(&mut clipped_builder.base, projection)?;
        project_with_builder(&clipped_builder, geometry, budget)
    } else {
        configure_builder(&mut builder, projection)?;
        configure_precision(&mut builder.base, projection)?;
        project_with_builder(&builder, geometry, budget)
    }
}

fn configure_builder<B>(builder: &mut B, projection: &GeoProjection) -> Result<(), String>
where
    B: CenterSet<T = f64> + RotateSet<T = f64> + ScaleGet<T = f64> + TranslateGet<T = f64>,
{
    if let Some(center) = projection.center {
        builder.center_set(&Coord {
            x: center[0],
            y: center[1],
        });
    }
    if let Some(rotate) = projection.rotate {
        builder.rotate3_set(&rotate);
    }
    Ok(())
}

fn project_conic_raw<PR>(
    geometry: &GeoGeometry,
    projection: &GeoProjection,
    budget: &ProjectionBudget,
    default_parallels: [f64; 2],
    default_scale: f64,
    default_center: [f64; 2],
    default_rotate: [f64; 2],
) -> Result<GeometryProjection, String>
where
    PR: d3_geo_rs::projection::builder_conic::PRConic<T = f64> + Default + Clone,
    d3_geo_rs::projection::builder::types::BuilderAntimeridianResampleNoClip<
        GeoPathEndpoint,
        PR,
        f64,
    >: CenterSet<T = f64>
        + RotateSet<T = f64>
        + ScaleSet<T = f64>
        + ScaleGet<T = f64>
        + TranslateGet<T = f64>
        + PrecisionAdjust<T = f64>
        + Build,
    <d3_geo_rs::projection::builder::types::BuilderAntimeridianResampleNoClip<
        GeoPathEndpoint,
        PR,
        f64,
    > as Build>::Projector: Projector<EP = GeoPathEndpoint>,
    <<d3_geo_rs::projection::builder::types::BuilderAntimeridianResampleNoClip<
        GeoPathEndpoint,
        PR,
        f64,
    > as Build>::Projector as Projector>::Transformer: Stream<EP = GeoPathEndpoint, T = f64>,
{
    let parallels = projection.parallels.unwrap_or(default_parallels);
    let raw = PR::default().generate(parallels[0].to_radians(), parallels[1].to_radians());
    let mut builder = <d3_geo_rs::projection::builder::types::BuilderAntimeridianResampleNoClip<
        GeoPathEndpoint,
        PR,
        f64,
    > as BuilderTrait>::new(raw);
    builder.scale_set(default_scale);
    builder.center_set(&Coord {
        x: default_center[0],
        y: default_center[1],
    });
    builder.rotate2_set(&default_rotate);
    configure_builder(&mut builder, projection)?;
    configure_precision(&mut builder, projection)?;
    if let Some(angle) = projection.clip_angle {
        let clipped = ClipAngleSet::clip_angle_set(&builder, angle);
        project_with_builder(&clipped, geometry, budget)
    } else {
        project_with_builder(&builder, geometry, budget)
    }
}

fn configure_precision<B>(builder: &mut B, projection: &GeoProjection) -> Result<(), String>
where
    B: PrecisionAdjust<T = f64>,
{
    if let Some(precision) = projection.precision {
        if !precision.is_finite() || precision < 0.0 {
            return Err("projection precision must be finite and non-negative".to_string());
        }
        builder.precision_set(&precision);
    }
    Ok(())
}

fn project_with_builder<B>(
    builder: &B,
    geometry: &GeoGeometry,
    budget: &ProjectionBudget,
) -> Result<GeometryProjection, String>
where
    B: Build + ScaleGet<T = f64> + TranslateGet<T = f64>,
    B::Projector: Projector<EP = GeoPathEndpoint>,
    <B::Projector as Projector>::Transformer: Stream<EP = GeoPathEndpoint, T = f64>,
{
    let scale = builder.scale();
    let translate = builder.translate();
    if !scale.is_finite() || scale <= 0.0 {
        return Err("projection default scale must be finite and positive".to_string());
    }
    let geos = to_geo_geometries(geometry);
    let mut geometries = Vec::new();
    for geo in &geos {
        // The projection stream keeps clipping state between streamed objects. Give each
        // GeoJSON geometry its own stream so a preceding polygon cannot affect a later
        // point or line in the same GeometryCollection.
        let mut projector = builder.build();
        let endpoint = GeoPathEndpoint {
            budget: budget.clone(),
            ..GeoPathEndpoint::default()
        };
        let mut stream = projector.stream(&endpoint);
        geo.to_stream(&mut stream);
        let endpoint = stream.endpoint();
        if let Some(error) = endpoint.error.take() {
            return Err(error);
        }
        geometries.extend(std::mem::take(&mut endpoint.output));
    }
    Ok(GeometryProjection {
        geometries,
        default_scale: scale,
        default_translate: [translate.x, translate.y],
    })
}

fn to_geo_geometry(geometry: &GeoGeometry) -> Geometry<f64> {
    let line = |coords: &[[f64; 2]]| {
        LineString(
            coords
                .iter()
                .map(|xy| Coord { x: xy[0], y: xy[1] })
                .collect(),
        )
    };
    match geometry {
        GeoGeometry::Point(xy) => Geometry::Point(Point::new(xy[0], xy[1])),
        GeoGeometry::MultiPoint(points) => Geometry::MultiPoint(MultiPoint(
            points.iter().map(|xy| Point::new(xy[0], xy[1])).collect(),
        )),
        GeoGeometry::LineString(points) => Geometry::LineString(line(points)),
        GeoGeometry::MultiLineString(lines) => Geometry::MultiLineString(MultiLineString(
            lines.iter().map(|coords| line(coords)).collect(),
        )),
        GeoGeometry::Polygon(rings) if rings.is_empty() => {
            Geometry::GeometryCollection(GeometryCollection(Vec::new()))
        }
        GeoGeometry::Polygon(rings) => Geometry::Polygon(Polygon::new(
            line(&rings[0]),
            rings[1..].iter().map(|ring| line(ring)).collect(),
        )),
        GeoGeometry::MultiPolygon(polygons) => Geometry::MultiPolygon(MultiPolygon(
            polygons
                .iter()
                .filter(|rings| !rings.is_empty())
                .map(|rings| {
                    Polygon::new(
                        line(&rings[0]),
                        rings[1..].iter().map(|ring| line(ring)).collect(),
                    )
                })
                .collect(),
        )),
        GeoGeometry::GeometryCollection(geometries) => Geometry::GeometryCollection(
            GeometryCollection(geometries.iter().map(to_geo_geometry).collect()),
        ),
    }
}

fn to_geo_geometries(geometry: &GeoGeometry) -> Vec<Geometry<f64>> {
    match geometry {
        GeoGeometry::GeometryCollection(geometries) => {
            geometries.iter().flat_map(to_geo_geometries).collect()
        }
        _ => vec![to_geo_geometry(geometry)],
    }
}

fn geometry_leaves<'a>(geometry: &'a GeoGeometry, output: &mut Vec<&'a GeoGeometry>) {
    if let GeoGeometry::GeometryCollection(geometries) = geometry {
        for geometry in geometries {
            geometry_leaves(geometry, output);
        }
    } else {
        output.push(geometry);
    }
}

fn geo_bounds(geometry: &GeoGeometry) -> Option<[f64; 4]> {
    fn extend(bounds: &mut [f64; 4], point: [f64; 2]) {
        bounds[0] = bounds[0].min(point[0]);
        bounds[1] = bounds[1].min(point[1]);
        bounds[2] = bounds[2].max(point[0]);
        bounds[3] = bounds[3].max(point[1]);
    }

    fn visit(geometry: &GeoGeometry, bounds: &mut [f64; 4]) {
        let mut extend_points = |points: &[[f64; 2]]| {
            for point in points {
                extend(bounds, *point);
            }
        };
        match geometry {
            GeoGeometry::Point(point) => extend(bounds, *point),
            GeoGeometry::MultiPoint(points) | GeoGeometry::LineString(points) => {
                extend_points(points)
            }
            GeoGeometry::MultiLineString(lines) | GeoGeometry::Polygon(lines) => {
                for line in lines {
                    extend_points(line);
                }
            }
            GeoGeometry::MultiPolygon(polygons) => {
                for polygon in polygons {
                    for ring in polygon {
                        extend_points(ring);
                    }
                }
            }
            GeoGeometry::GeometryCollection(geometries) => {
                for geometry in geometries {
                    visit(geometry, bounds);
                }
            }
        }
    }

    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    visit(geometry, &mut bounds);
    bounds[0].is_finite().then_some(bounds)
}

fn project_albers_component<B>(
    builder: &B,
    geometry: &GeoGeometry,
    region: [f64; 4],
    budget: &ProjectionBudget,
) -> Result<Vec<RawProjectedGeometry>, String>
where
    B: Build + ScaleGet<T = f64> + TranslateGet<T = f64>,
    B::Projector: Projector<EP = GeoPathEndpoint>,
    <B::Projector as Projector>::Transformer: Stream<EP = GeoPathEndpoint, T = f64>,
{
    let mut output = Vec::new();
    let mut leaves = Vec::new();
    geometry_leaves(geometry, &mut leaves);
    for leaf in leaves {
        let Some(bounds) = geo_bounds(leaf) else {
            continue;
        };
        if bounds[2] < region[0]
            || bounds[0] > region[2]
            || bounds[3] < region[1]
            || bounds[1] > region[3]
        {
            continue;
        }
        output.extend(project_with_builder(builder, leaf, budget)?.geometries);
    }
    Ok(output)
}

fn project_identity(
    geometry: &GeoGeometry,
    projection: &GeoProjection,
    budget: &ProjectionBudget,
) -> Result<Vec<RawProjectedGeometry>, String> {
    fn count_geometry(geometry: &GeoGeometry, budget: &ProjectionBudget) -> Result<(), String> {
        let add_path = |points: usize| -> Result<(), String> {
            for _ in 0..points {
                budget.add_vertex()?;
            }
            budget.add_primitive()
        };
        match geometry {
            GeoGeometry::Point(_) => {
                budget.add_vertex()?;
                budget.add_primitive()
            }
            GeoGeometry::MultiPoint(points) => {
                for _ in points {
                    budget.add_vertex()?;
                    budget.add_primitive()?;
                }
                Ok(())
            }
            GeoGeometry::LineString(points) if points.len() >= 2 => add_path(points.len()),
            GeoGeometry::MultiLineString(lines) => {
                for line in lines.iter().filter(|line| line.len() >= 2) {
                    add_path(line.len())?;
                }
                Ok(())
            }
            GeoGeometry::Polygon(rings) if !rings.is_empty() => {
                add_path(rings.iter().map(Vec::len).sum())
            }
            GeoGeometry::MultiPolygon(polygons) => {
                for polygon in polygons.iter().filter(|polygon| !polygon.is_empty()) {
                    add_path(polygon.iter().map(Vec::len).sum())?;
                }
                Ok(())
            }
            GeoGeometry::GeometryCollection(geometries) => {
                for geometry in geometries {
                    count_geometry(geometry, budget)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn visit(
        geometry: &GeoGeometry,
        projection: &GeoProjection,
        output: &mut Vec<RawProjectedGeometry>,
    ) {
        let point = |xy: [f64; 2]| {
            [
                if projection.reflect_x { -xy[0] } else { xy[0] },
                if projection.reflect_y { -xy[1] } else { xy[1] },
            ]
        };
        let path = |paths: Vec<Vec<[f64; 2]>>, fillable| {
            let mut subpaths = paths
                .into_iter()
                .map(|p| p.into_iter().map(point).collect())
                .collect::<Vec<Vec<[f64; 2]>>>();
            if fillable {
                for (index, ring) in subpaths.iter_mut().enumerate() {
                    normalize_ring_winding(ring, index == 0);
                }
            }
            RawProjectedGeometry::Path { subpaths, fillable }
        };
        match geometry {
            GeoGeometry::Point(xy) => output.push(RawProjectedGeometry::Point(point(*xy))),
            GeoGeometry::MultiPoint(points) => output.extend(
                points
                    .iter()
                    .map(|xy| RawProjectedGeometry::Point(point(*xy))),
            ),
            GeoGeometry::LineString(points) if points.len() >= 2 => {
                output.push(path(vec![points.clone()], false));
            }
            GeoGeometry::MultiLineString(lines) => {
                for line in lines.iter().filter(|line| line.len() >= 2) {
                    output.push(path(vec![line.clone()], false));
                }
            }
            GeoGeometry::Polygon(rings) if !rings.is_empty() => {
                output.push(path(rings.clone(), true));
            }
            GeoGeometry::MultiPolygon(polygons) => {
                for polygon in polygons.iter().filter(|polygon| !polygon.is_empty()) {
                    output.push(path(polygon.clone(), true));
                }
            }
            GeoGeometry::GeometryCollection(geometries) => {
                for geometry in geometries {
                    visit(geometry, projection, output);
                }
            }
            _ => {}
        }
    }
    let mut output = Vec::new();
    count_geometry(geometry, budget)?;
    visit(geometry, projection, &mut output);
    if output.iter().any(|geometry| match geometry {
        RawProjectedGeometry::Point([x, y]) => !x.is_finite() || !y.is_finite(),
        RawProjectedGeometry::Path { subpaths, .. } => subpaths
            .iter()
            .flatten()
            .any(|point| !point[0].is_finite() || !point[1].is_finite()),
    }) {
        return Err("identity projection produced a non-finite coordinate".to_string());
    }
    Ok(output)
}

fn project_albers_usa(
    projection: &GeoProjection,
    geometry: &GeoGeometry,
    budget: &ProjectionBudget,
) -> Result<GeometryProjection, String> {
    use d3_geo_rs::projection::ClipExtentSet;
    use d3_geo_rs::projection::builder_conic::PRConic;

    let scale = 1070.0;
    let translate = [480.0, 250.0];
    let coord = |x, y| Coord { x, y };
    let extent = |x0, y0, x1, y1| [coord(x0, y0), coord(x1, y1)];
    type ComponentBuilder =
        d3_geo_rs::projection::builder::types::BuilderAntimeridianResampleNoClip<
            GeoPathEndpoint,
            d3_geo_rs::projection::equal_area::EqualArea<f64>,
            f64,
        >;
    let component = |parallels: [f64; 2],
                     component_scale: f64,
                     center: [f64; 2],
                     rotate: [f64; 2],
                     component_translate: [f64; 2]| {
        let raw = d3_geo_rs::projection::equal_area::EqualArea::<f64>::default()
            .generate(parallels[0].to_radians(), parallels[1].to_radians());
        let mut builder = <ComponentBuilder as BuilderTrait>::new(raw);
        builder.scale_set(component_scale);
        builder.center_set(&coord(center[0], center[1]));
        builder.rotate2_set(&rotate);
        builder.translate_set(&coord(component_translate[0], component_translate[1]));
        builder
    };
    let mut geometries = Vec::new();

    let mut lower48 = component([29.5, 45.5], scale, [-0.6, 38.7], [96.0, 0.0], translate);
    configure_builder(&mut lower48, projection)?;
    configure_precision(&mut lower48, projection)?;
    let lower48_clip = ClipExtentSet::clip_extent_set(
        &lower48,
        &extent(
            translate[0] - 0.455 * scale,
            translate[1],
            translate[0] + 0.455 * scale,
            translate[1] + 0.234 * scale,
        ),
    );
    geometries.extend(project_albers_component(
        &lower48_clip,
        geometry,
        [-130.0, 20.0, -60.0, 60.0],
        budget,
    )?);

    let mut alaska = component(
        [55.0, 65.0],
        0.35 * scale,
        [-2.0, 58.5],
        [154.0, 0.0],
        [translate[0] - 0.307 * scale, translate[1] + 0.201 * scale],
    );
    configure_builder(&mut alaska, projection)?;
    configure_precision(&mut alaska, projection)?;
    let alaska_clip = ClipExtentSet::clip_extent_set(
        &alaska,
        &extent(
            translate[0] - 0.425 * scale,
            translate[1] + 0.120 * scale,
            translate[0] - 0.214 * scale,
            translate[1] + 0.234 * scale,
        ),
    );
    geometries.extend(project_albers_component(
        &alaska_clip,
        geometry,
        [-180.0, 48.0, -128.0, 75.0],
        budget,
    )?);

    let mut hawaii = component(
        [8.0, 18.0],
        scale,
        [-3.0, 19.9],
        [157.0, 0.0],
        [translate[0] - 0.205 * scale, translate[1] + 0.212 * scale],
    );
    configure_builder(&mut hawaii, projection)?;
    configure_precision(&mut hawaii, projection)?;
    let hawaii_clip = ClipExtentSet::clip_extent_set(
        &hawaii,
        &extent(
            translate[0] - 0.214 * scale,
            translate[1] + 0.166 * scale,
            translate[0] - 0.115 * scale,
            translate[1] + 0.234 * scale,
        ),
    );
    geometries.extend(project_albers_component(
        &hawaii_clip,
        geometry,
        [-163.0, 18.0, -152.0, 25.0],
        budget,
    )?);

    Ok(GeometryProjection {
        geometries,
        default_scale: scale,
        default_translate: translate,
    })
}

fn write_path(paths: &[Vec<[f64; 2]>], closed: bool) -> String {
    let mut output = String::new();
    for points in paths.iter().filter(|points| !points.is_empty()) {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str("M ");
        output.push_str(&fmt_num(points[0][0]));
        output.push(' ');
        output.push_str(&fmt_num(points[0][1]));
        for point in &points[1..] {
            output.push_str(" L ");
            output.push_str(&fmt_num(point[0]));
            output.push(' ');
            output.push_str(&fmt_num(point[1]));
        }
        if closed && points.len() >= 3 {
            output.push_str(" Z");
        }
    }
    output
}

fn path_coordinates(path: &str) -> Result<Vec<(f64, f64)>, String> {
    let mut output = Vec::new();
    let mut numbers = Vec::new();
    for token in path.split_whitespace() {
        if matches!(token, "M" | "L" | "Z") {
            if numbers.len() == 2 {
                output.push((numbers[0], numbers[1]));
                numbers.clear();
            }
        } else {
            let value = token
                .parse::<f64>()
                .map_err(|_| "projected path contains an invalid number".to_string())?;
            if !value.is_finite() {
                return Err("projection produced a non-finite coordinate".to_string());
            }
            numbers.push(value);
        }
    }
    if numbers.len() == 2 {
        output.push((numbers[0], numbers[1]));
    }
    if numbers.len() > 2 {
        return Err("projected path contains an incomplete coordinate".to_string());
    }
    Ok(output)
}

fn transform_path(
    path: &str,
    default_translate: [f64; 2],
    factor: f64,
    translate: [f64; 2],
) -> Result<String, String> {
    let mut output = String::new();
    let mut numbers: Vec<f64> = Vec::new();
    for token in path.split_whitespace() {
        if matches!(token, "M" | "L" | "Z") {
            if numbers.len() == 2 {
                let x = (numbers[0] - default_translate[0]) * factor + translate[0];
                let y = (numbers[1] - default_translate[1]) * factor + translate[1];
                if !x.is_finite() || !y.is_finite() {
                    return Err("projection produced a non-finite coordinate".to_string());
                }
                output.push_str(&fmt_num(x));
                output.push(' ');
                output.push_str(&fmt_num(y));
                output.push(' ');
                numbers.clear();
            }
            if !output.is_empty() {
                output.push(' ');
            }
            output.push_str(token);
            output.push(' ');
        } else {
            let value = token
                .parse::<f64>()
                .map_err(|_| "projected path contains an invalid number".to_string())?;
            if !value.is_finite() {
                return Err("projection produced a non-finite coordinate".to_string());
            }
            numbers.push(value);
        }
    }
    if numbers.len() == 2 {
        let x = (numbers[0] - default_translate[0]) * factor + translate[0];
        let y = (numbers[1] - default_translate[1]) * factor + translate[1];
        if !x.is_finite() || !y.is_finite() {
            return Err("projection produced a non-finite coordinate".to_string());
        }
        output.push_str(&fmt_num(x));
        output.push(' ');
        output.push_str(&fmt_num(y));
    } else if !numbers.is_empty() {
        return Err("projected path contains an incomplete coordinate".to_string());
    }
    Ok(output)
}

fn extend_bounds(bounds: &mut [f64; 4], x: f64, y: f64) {
    bounds[0] = bounds[0].min(x);
    bounds[1] = bounds[1].min(y);
    bounds[2] = bounds[2].max(x);
    bounds[3] = bounds[3].max(y);
}

fn projection_clip(extent: Option<[[f64; 2]; 2]>, viewport: ClipRect) -> Option<ClipRect> {
    let [[x0, y0], [x1, y1]] = extent?;
    let left = viewport.x.max(x0);
    let top = viewport.y.max(y0);
    let right = (viewport.x + viewport.w).min(x1);
    let bottom = (viewport.y + viewport.h).min(y1);
    Some(ClipRect {
        x: left,
        y: top,
        w: (right - left).max(0.0),
        h: (bottom - top).max(0.0),
    })
}

/// d3-geo's v3 port omits Natural Earth 1; this is the standard raw formula.
#[derive(Clone, Copy, Debug, Default)]
struct NaturalEarth1;

impl d3_geo_rs::Transform for NaturalEarth1 {
    type T = f64;

    fn transform(&self, point: &Coord<f64>) -> Coord<f64> {
        let y2 = point.y * point.y;
        let y6 = y2 * y2 * y2;
        let x = point.x
            * (0.8707 + y2 * (-0.131_979 + y6 * (-0.013_791 + y2 * (0.003_971 - 0.001_529 * y2))));
        let y = point.y
            * (1.007_226
                + y2 * (0.015_085 + y2 * (-0.044_475 + y2 * (0.028_874 - 0.005_916 * y2))));
        Coord { x, y }
    }

    fn invert(&self, point: &Coord<f64>) -> Coord<f64> {
        let mut y = point.y;
        for _ in 0..12 {
            let projected = self.transform(&Coord { x: 0.0, y }).y;
            let step = 1.0e-6 * y.abs().max(1.0);
            let derivative = (self
                .transform(&Coord {
                    x: 0.0,
                    y: y + step,
                })
                .y
                - self
                    .transform(&Coord {
                        x: 0.0,
                        y: y - step,
                    })
                    .y)
                / (2.0 * step);
            let delta = (projected - point.y) / derivative;
            y -= delta;
            if delta.abs() < 1.0e-12 {
                break;
            }
        }
        let y2 = y * y;
        let y6 = y2 * y2 * y2;
        let factor =
            0.8707 + y2 * (-0.131_979 + y6 * (-0.013_791 + y2 * (0.003_971 - 0.001_529 * y2)));
        Coord {
            x: point.x / factor,
            y,
        }
    }
}

impl RawBase for NaturalEarth1 {
    type Builder<DRAIN: Clone> =
        d3_geo_rs::projection::builder::types::BuilderAntimeridianResampleNoClip<DRAIN, Self, f64>;

    fn builder<DRAIN: Clone>() -> Self::Builder<DRAIN> {
        <Self::Builder<DRAIN> as BuilderTrait>::new(Self)
    }
}

/// Parse inline GeoJSON or rows containing a GeoJSON field, checking resource limits first.
pub fn parse_geojson(
    data: &Value,
    shape_field: Option<&str>,
    limits: &InputLimits,
) -> Result<Vec<RawGeoFeature>, String> {
    let mut counts = Counts::default();
    preflight_root(data, shape_field, limits, &mut counts, "$ ".trim_end())?;

    let mut features = Vec::with_capacity(counts.features);
    parse_root(data, shape_field, &mut features, "$ ".trim_end())?;
    Ok(features)
}

fn preflight_root(
    value: &Value,
    shape_field: Option<&str>,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    if let Some(values) = value.as_array() {
        for (index, value) in values.iter().enumerate() {
            let item_path = format!("{path}[{index}]");
            preflight_root_item(value, shape_field, limits, counts, &item_path)?;
        }
        return Ok(());
    }
    preflight_root_item(value, shape_field, limits, counts, path)
}

fn preflight_root_item(
    value: &Value,
    shape_field: Option<&str>,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    match value.get("type").and_then(Value::as_str) {
        Some("Feature") => preflight_feature(value, limits, counts, path),
        Some("FeatureCollection") => preflight_feature_collection(value, limits, counts, path),
        Some(_) if shape_field.is_none() => {
            Err(at(path, "expected a GeoJSON Feature or a record object"))
        }
        Some(_) | None => {
            let record = value
                .as_object()
                .ok_or_else(|| at(path, "expected a record object"))?;
            let field = shape_field.ok_or_else(|| {
                at(
                    path,
                    "record input requires encoding.shape.field with inline GeoJSON",
                )
            })?;
            let shape_path = format!("{path}.{field}");
            let shape = record
                .get(field)
                .ok_or_else(|| at(&shape_path, "shape field is missing"))?;
            preflight_shape(shape, limits, counts, &shape_path)
        }
    }
}

fn preflight_shape(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    match value.get("type").and_then(Value::as_str) {
        Some("Feature") => preflight_feature(value, limits, counts, path),
        Some("FeatureCollection") => preflight_feature_collection(value, limits, counts, path),
        Some(
            "Point" | "MultiPoint" | "LineString" | "MultiLineString" | "Polygon" | "MultiPolygon"
            | "GeometryCollection",
        ) => {
            add_limit(
                &mut counts.features,
                1,
                limits.max_geo_features,
                "feature count",
                path,
            )?;
            preflight_geometry(value, limits, counts, path, 0)
        }
        Some(kind) => Err(at(path, &format!("unsupported GeoJSON type {kind:?}"))),
        None => Err(at(
            path,
            "expected a GeoJSON Geometry, Feature, or FeatureCollection",
        )),
    }
}

fn preflight_feature_collection(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| at(&format!("{path}.features"), "expected an array"))?;
    for (index, feature) in features.iter().enumerate() {
        let feature_path = format!("{path}.features[{index}]");
        preflight_feature(feature, limits, counts, &feature_path)?;
    }
    Ok(())
}

fn preflight_feature(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| at(path, "expected a Feature object"))?;
    if object.get("type").and_then(Value::as_str) != Some("Feature") {
        return Err(at(path, "expected type \"Feature\""));
    }
    let properties_path = format!("{path}.properties");
    match object.get("properties") {
        Some(Value::Object(_)) | Some(Value::Null) => {}
        Some(_) => return Err(at(&properties_path, "expected an object or null")),
        None => return Err(at(&properties_path, "required property is missing")),
    }
    add_limit(
        &mut counts.features,
        1,
        limits.max_geo_features,
        "feature count",
        path,
    )?;
    let geometry_path = format!("{path}.geometry");
    let geometry = object
        .get("geometry")
        .ok_or_else(|| at(&geometry_path, "required property is missing"))?;
    if !geometry.is_null() {
        preflight_geometry(geometry, limits, counts, &geometry_path, 0)?;
    }
    Ok(())
}

fn preflight_geometry(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| at(path, "expected a GeoJSON Geometry object"))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| at(&format!("{path}.type"), "expected a string"))?;
    if kind == "GeometryCollection" {
        if depth >= MAX_GEOMETRY_COLLECTION_DEPTH {
            return Err(at(
                path,
                &format!("GeometryCollection nesting exceeds {MAX_GEOMETRY_COLLECTION_DEPTH}"),
            ));
        }
        let geometries = object
            .get("geometries")
            .and_then(Value::as_array)
            .ok_or_else(|| at(&format!("{path}.geometries"), "expected an array"))?;
        for (index, geometry) in geometries.iter().enumerate() {
            preflight_geometry(
                geometry,
                limits,
                counts,
                &format!("{path}.geometries[{index}]"),
                depth + 1,
            )?;
        }
        return Ok(());
    }

    let coordinates_path = format!("{path}.coordinates");
    let coordinates = object
        .get("coordinates")
        .ok_or_else(|| at(&coordinates_path, "required property is missing"))?;
    match kind {
        "Point" => {
            preflight_position(coordinates, limits, counts, &coordinates_path)?;
            add_limit(
                &mut counts.primitives,
                1,
                limits.max_geo_primitives,
                "primitive count",
                path,
            )?;
        }
        "MultiPoint" => {
            let points = coordinate_array(coordinates, &coordinates_path)?;
            for (index, point) in points.iter().enumerate() {
                preflight_position(
                    point,
                    limits,
                    counts,
                    &format!("{coordinates_path}[{index}]"),
                )?;
            }
            add_limit(
                &mut counts.primitives,
                points.len(),
                limits.max_geo_primitives,
                "primitive count",
                path,
            )?;
        }
        "LineString" => {
            let point_count =
                preflight_line_string(coordinates, limits, counts, &coordinates_path)?;
            add_limit(
                &mut counts.primitives,
                usize::from(point_count > 0),
                limits.max_geo_primitives,
                "primitive count",
                path,
            )?;
        }
        "MultiLineString" => {
            let lines = coordinate_array(coordinates, &coordinates_path)?;
            for (index, line) in lines.iter().enumerate() {
                preflight_line_string(
                    line,
                    limits,
                    counts,
                    &format!("{coordinates_path}[{index}]"),
                )?;
            }
            add_limit(
                &mut counts.primitives,
                lines.len(),
                limits.max_geo_primitives,
                "primitive count",
                path,
            )?;
        }
        "Polygon" => {
            let rings = coordinate_array(coordinates, &coordinates_path)?;
            for (index, ring) in rings.iter().enumerate() {
                preflight_ring(
                    ring,
                    limits,
                    counts,
                    &format!("{coordinates_path}[{index}]"),
                )?;
            }
            add_limit(
                &mut counts.primitives,
                usize::from(!rings.is_empty()),
                limits.max_geo_primitives,
                "primitive count",
                path,
            )?;
        }
        "MultiPolygon" => {
            let polygons = coordinate_array(coordinates, &coordinates_path)?;
            for (polygon_index, polygon) in polygons.iter().enumerate() {
                let rings =
                    coordinate_array(polygon, &format!("{coordinates_path}[{polygon_index}]"))?;
                for (ring_index, ring) in rings.iter().enumerate() {
                    preflight_ring(
                        ring,
                        limits,
                        counts,
                        &format!("{coordinates_path}[{polygon_index}][{ring_index}]"),
                    )?;
                }
            }
            add_limit(
                &mut counts.primitives,
                polygons.len(),
                limits.max_geo_primitives,
                "primitive count",
                path,
            )?;
        }
        other => return Err(at(path, &format!("unsupported GeoJSON geometry {other:?}"))),
    }
    Ok(())
}

fn preflight_line_string(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<usize, String> {
    let positions = coordinate_array(value, path)?;
    if !positions.is_empty() && positions.len() < 2 {
        return Err(at(
            path,
            "a non-empty LineString requires at least two positions",
        ));
    }
    for (index, position) in positions.iter().enumerate() {
        preflight_position(position, limits, counts, &format!("{path}[{index}]"))?;
    }
    Ok(positions.len())
}

fn preflight_ring(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    let positions = coordinate_array(value, path)?;
    if positions.len() < 4 {
        return Err(at(path, "a polygon ring requires at least four positions"));
    }
    for (index, position) in positions.iter().enumerate() {
        preflight_position(position, limits, counts, &format!("{path}[{index}]"))?;
    }
    if let (Some(first), Some(last)) = (positions.first(), positions.last()) {
        let first = position_xy(first, &format!("{path}[0]"))?;
        let last = position_xy(last, &format!("{path}[{}]", positions.len() - 1))?;
        if first != last {
            return Err(at(path, "polygon ring must be closed"));
        }
    }
    Ok(())
}

fn preflight_position(
    value: &Value,
    limits: &InputLimits,
    counts: &mut Counts,
    path: &str,
) -> Result<(), String> {
    position_xy(value, path)?;
    add_limit(
        &mut counts.vertices,
        1,
        limits.max_geo_vertices,
        "coordinate vertex count",
        path,
    )
}

fn position_xy(value: &Value, path: &str) -> Result<[f64; 2], String> {
    let values = value
        .as_array()
        .ok_or_else(|| at(path, "position must be an array"))?;
    if values.len() < 2 {
        return Err(at(path, "position requires at least two ordinates"));
    }
    let mut xy = [0.0; 2];
    for (index, ordinate) in values.iter().enumerate() {
        let number = ordinate.as_f64().ok_or_else(|| {
            at(
                &format!("{path}[{index}]"),
                "ordinate must be a finite number",
            )
        })?;
        if !number.is_finite() {
            return Err(at(
                &format!("{path}[{index}]"),
                "ordinate must be a finite number",
            ));
        }
        if index < 2 {
            xy[index] = number;
        }
    }
    Ok(xy)
}

fn coordinate_array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| at(path, "expected an array"))
}

fn add_limit(
    total: &mut usize,
    amount: usize,
    limit: usize,
    name: &str,
    path: &str,
) -> Result<(), String> {
    *total = total.saturating_add(amount);
    if *total > limit {
        return Err(at(
            path,
            &format!("{name} {} exceeds limit {limit}", *total),
        ));
    }
    Ok(())
}

fn parse_root(
    value: &Value,
    shape_field: Option<&str>,
    features: &mut Vec<RawGeoFeature>,
    path: &str,
) -> Result<(), String> {
    if let Some(values) = value.as_array() {
        for (index, value) in values.iter().enumerate() {
            parse_root_item(value, shape_field, features, &format!("{path}[{index}]"))?;
        }
        return Ok(());
    }
    parse_root_item(value, shape_field, features, path)
}

fn parse_root_item(
    value: &Value,
    shape_field: Option<&str>,
    features: &mut Vec<RawGeoFeature>,
    path: &str,
) -> Result<(), String> {
    match value.get("type").and_then(Value::as_str) {
        Some("Feature") => parse_feature(value, Arc::new(Map::new()), features, path),
        Some("FeatureCollection") => {
            parse_feature_collection(value, Arc::new(Map::new()), features, path)
        }
        Some(_) if shape_field.is_none() => {
            Err(at(path, "expected a GeoJSON Feature or a record object"))
        }
        Some(_) | None => {
            let record = value
                .as_object()
                .ok_or_else(|| at(path, "expected a record object"))?;
            let field = shape_field.ok_or_else(|| {
                at(
                    path,
                    "record input requires encoding.shape.field with inline GeoJSON",
                )
            })?;
            let shape_path = format!("{path}.{field}");
            let shape = record
                .get(field)
                .ok_or_else(|| at(&shape_path, "shape field is missing"))?;
            let record = record.clone();
            parse_shape(shape, Arc::new(record.clone()), features, &shape_path)
        }
    }
}

fn parse_shape(
    value: &Value,
    record: Arc<Map<String, Value>>,
    features: &mut Vec<RawGeoFeature>,
    path: &str,
) -> Result<(), String> {
    match value.get("type").and_then(Value::as_str) {
        Some("Feature") => parse_feature(value, record, features, path),
        Some("FeatureCollection") => parse_feature_collection(value, record, features, path),
        Some(
            "Point" | "MultiPoint" | "LineString" | "MultiLineString" | "Polygon" | "MultiPolygon"
            | "GeometryCollection",
        ) => {
            features.push(RawGeoFeature {
                geometry: Some(parse_geometry(value, path, 0)?),
                record,
                properties: Map::new(),
            });
            Ok(())
        }
        Some(kind) => Err(at(path, &format!("unsupported GeoJSON type {kind:?}"))),
        None => Err(at(
            path,
            "expected a GeoJSON Geometry, Feature, or FeatureCollection",
        )),
    }
}

fn parse_feature_collection(
    value: &Value,
    record: Arc<Map<String, Value>>,
    features: &mut Vec<RawGeoFeature>,
    path: &str,
) -> Result<(), String> {
    let values = value
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| at(&format!("{path}.features"), "expected an array"))?;
    for (index, feature) in values.iter().enumerate() {
        parse_feature(
            feature,
            record.clone(),
            features,
            &format!("{path}.features[{index}]"),
        )?;
    }
    Ok(())
}

fn parse_feature(
    value: &Value,
    record: Arc<Map<String, Value>>,
    features: &mut Vec<RawGeoFeature>,
    path: &str,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| at(path, "expected a Feature object"))?;
    let properties = match object.get("properties") {
        Some(Value::Object(properties)) => properties.clone(),
        Some(Value::Null) => Map::new(),
        _ => {
            return Err(at(
                &format!("{path}.properties"),
                "expected an object or null",
            ));
        }
    };
    let geometry = object
        .get("geometry")
        .ok_or_else(|| at(&format!("{path}.geometry"), "required property is missing"))?;
    features.push(RawGeoFeature {
        geometry: if geometry.is_null() {
            None
        } else {
            Some(parse_geometry(geometry, &format!("{path}.geometry"), 0)?)
        },
        record,
        properties,
    });
    Ok(())
}

fn parse_geometry(value: &Value, path: &str, depth: usize) -> Result<GeoGeometry, String> {
    let object = value
        .as_object()
        .ok_or_else(|| at(path, "expected a GeoJSON Geometry object"))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| at(&format!("{path}.type"), "expected a string"))?;
    if kind == "GeometryCollection" {
        if depth >= MAX_GEOMETRY_COLLECTION_DEPTH {
            return Err(at(
                path,
                &format!("GeometryCollection nesting exceeds {MAX_GEOMETRY_COLLECTION_DEPTH}"),
            ));
        }
        let geometries = object
            .get("geometries")
            .and_then(Value::as_array)
            .ok_or_else(|| at(&format!("{path}.geometries"), "expected an array"))?;
        let geometries = geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| {
                parse_geometry(geometry, &format!("{path}.geometries[{index}]"), depth + 1)
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(GeoGeometry::GeometryCollection(geometries));
    }
    let coordinates_path = format!("{path}.coordinates");
    let coordinates = object
        .get("coordinates")
        .ok_or_else(|| at(&coordinates_path, "required property is missing"))?;
    match kind {
        "Point" => Ok(GeoGeometry::Point(position_xy(
            coordinates,
            &coordinates_path,
        )?)),
        "MultiPoint" => Ok(GeoGeometry::MultiPoint(parse_positions(
            coordinates,
            &coordinates_path,
        )?)),
        "LineString" => Ok(GeoGeometry::LineString(parse_positions(
            coordinates,
            &coordinates_path,
        )?)),
        "MultiLineString" => Ok(GeoGeometry::MultiLineString(parse_lines(
            coordinates,
            &coordinates_path,
        )?)),
        "Polygon" => Ok(GeoGeometry::Polygon(parse_polygon(
            coordinates,
            &coordinates_path,
        )?)),
        "MultiPolygon" => {
            let polygons = coordinate_array(coordinates, &coordinates_path)?;
            let polygons = polygons
                .iter()
                .enumerate()
                .map(|(index, polygon)| {
                    parse_polygon(polygon, &format!("{coordinates_path}[{index}]"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(GeoGeometry::MultiPolygon(polygons))
        }
        other => Err(at(path, &format!("unsupported GeoJSON geometry {other:?}"))),
    }
}

fn parse_positions(value: &Value, path: &str) -> Result<Vec<[f64; 2]>, String> {
    let positions = coordinate_array(value, path)?;
    positions
        .iter()
        .enumerate()
        .map(|(index, position)| position_xy(position, &format!("{path}[{index}]")))
        .collect()
}

fn parse_lines(value: &Value, path: &str) -> Result<Vec<Vec<[f64; 2]>>, String> {
    let lines = coordinate_array(value, path)?;
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| parse_positions(line, &format!("{path}[{index}]")))
        .collect()
}

fn parse_polygon(value: &Value, path: &str) -> Result<Vec<Vec<[f64; 2]>>, String> {
    let rings = coordinate_array(value, path)?;
    rings
        .iter()
        .enumerate()
        .map(|(index, ring)| parse_positions(ring, &format!("{path}[{index}]")))
        .collect()
}

fn at(path: &str, message: &str) -> String {
    format!("GeoJSON at {path}: {message}")
}

#[cfg(test)]
mod tests {
    use crate::guard::InputLimits;
    use crate::ir::{
        GeoFeature, GeoGeometry, GeoProjection, GeoProjectionType, GeoShape, GeoShapeStyle,
    };
    use serde_json::json;

    trait RecordStorageAddress {
        fn storage_address(&self) -> *const serde_json::Map<String, serde_json::Value>;
    }

    impl RecordStorageAddress for serde_json::Map<String, serde_json::Value> {
        fn storage_address(&self) -> *const serde_json::Map<String, serde_json::Value> {
            self
        }
    }

    impl RecordStorageAddress for std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
        fn storage_address(&self) -> *const serde_json::Map<String, serde_json::Value> {
            std::sync::Arc::as_ptr(self)
        }
    }

    #[test]
    fn parses_every_geometry_and_geojson_wrapper() {
        let records = json!([
            {"id":"point", "shape":{"type":"Point", "coordinates":[1,2]}},
            {"id":"multipoint", "shape":{"type":"MultiPoint", "coordinates":[[1,2],[3,4]]}},
            {"id":"line", "shape":{"type":"LineString", "coordinates":[[1,2],[3,4]]}},
            {"id":"multiline", "shape":{"type":"MultiLineString", "coordinates":[[[1,2],[3,4]]]}},
            {"id":"polygon", "shape":{"type":"Polygon", "coordinates":[[[0,0],[4,0],[4,4],[0,0]]]}},
            {"id":"multipolygon", "shape":{"type":"MultiPolygon", "coordinates":[[[[0,0],[4,0],[4,4],[0,0]]]]}},
            {"id":"collection", "shape":{"type":"GeometryCollection", "geometries":[
                {"type":"Point", "coordinates":[1,2]},
                {"type":"LineString", "coordinates":[[1,2],[3,4]]}
            ]}}
        ]);
        let parsed = super::parse_geojson(&records, Some("shape"), &InputLimits::default())
            .expect("all GeoJSON geometry variants should parse");
        assert_eq!(parsed.len(), 7);
        assert_eq!(
            parsed
                .iter()
                .map(|feature| feature.geometry.as_ref().unwrap().kind_name())
                .collect::<Vec<_>>(),
            [
                "Point",
                "MultiPoint",
                "LineString",
                "MultiLineString",
                "Polygon",
                "MultiPolygon",
                "GeometryCollection"
            ]
        );

        let collection = json!({
            "type":"FeatureCollection",
            "features":[
                {"type":"Feature", "id":"first", "properties":{"label":"first"},
                 "geometry":{"type":"Point", "coordinates":[1,2]}},
                {"type":"Feature", "id":"second", "properties":{"label":"second"},
                 "geometry":null}
            ]
        });
        let parsed = super::parse_geojson(&collection, None, &InputLimits::default())
            .expect("FeatureCollection should expand in source order");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].properties["label"], "first");
        assert_eq!(parsed[1].properties["label"], "second");
        assert!(parsed[1].geometry.is_none());

        let feature_array = json!([
            {"type":"Feature", "properties":{"id":"a"},
             "geometry":{"type":"Point", "coordinates":[0,0]}},
            {"type":"Feature", "properties":{"id":"b"},
             "geometry":{"type":"Point", "coordinates":[1,1]}}
        ]);
        let parsed = super::parse_geojson(&feature_array, None, &InputLimits::default())
            .expect("Feature arrays should parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].properties["id"], "a");
        assert_eq!(parsed[1].properties["id"], "b");

        let single_feature = json!({
            "type":"Feature", "properties":{"id":"single"},
            "geometry":{"type":"Point", "coordinates":[5,6]}
        });
        let parsed = super::parse_geojson(&single_feature, None, &InputLimits::default())
            .expect("A single Feature should parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].properties["id"], "single");
    }

    #[test]
    fn record_shape_feature_collection_shares_its_source_record() {
        let records = json!([{
            "name":"region",
            "shape":{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"id":1},"geometry":{"type":"Point","coordinates":[0,0]}},
                {"type":"Feature","properties":{"id":2},"geometry":{"type":"Point","coordinates":[1,1]}}
            ]}
        }]);
        let parsed = super::parse_geojson(&records, Some("shape"), &InputLimits::default())
            .expect("record shapes can contain a FeatureCollection");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].record.storage_address(),
            parsed[1].record.storage_address(),
            "each expanded Feature must not deep-clone the full source record"
        );
    }

    #[test]
    fn rejects_malformed_coordinates_with_path() {
        let cases = [
            (
                json!({"type":"Point", "coordinates":[1]}),
                "$[0].shape.coordinates",
            ),
            (
                json!({"type":"MultiPoint", "coordinates":[1,2]}),
                "$[0].shape.coordinates[0]",
            ),
            (
                json!({"type":"Polygon", "coordinates":[[[0,0],[2,0],[2,2]]]}),
                "$[0].shape.coordinates[0]",
            ),
            (
                json!({"type":"Polygon", "coordinates":[[[0,0],[2,0],[2,2],[1,1]]]}),
                "$[0].shape.coordinates[0]",
            ),
            (
                json!({"type":"Polygon", "coordinates":[[]]}),
                "$[0].shape.coordinates[0]",
            ),
        ];
        for (shape, expected_path) in cases {
            let record = json!([{"shape":shape}]);
            let error = super::parse_geojson(&record, Some("shape"), &InputLimits::default())
                .expect_err("malformed GeoJSON should be rejected");
            assert!(
                error.contains(expected_path),
                "expected {expected_path:?} in {error:?}"
            );
        }

        let mut nested = json!({"type":"Point", "coordinates":[0,0]});
        for _ in 0..65 {
            nested = json!({"type":"GeometryCollection", "geometries":[nested]});
        }
        let record = json!([{"shape":nested}]);
        let error = super::parse_geojson(&record, Some("shape"), &InputLimits::default())
            .expect_err("deeply nested GeometryCollections should be rejected");
        assert!(
            error.contains("GeometryCollection nesting"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn preflight_rejects_feature_vertex_and_part_limits() {
        let feature_collection = json!({
            "type":"FeatureCollection",
            "features":[
                {"type":"Feature", "properties":{}, "geometry":{"type":"Point", "coordinates":[0,0]}},
                {"type":"Feature", "properties":{}, "geometry":{"type":"Point", "coordinates":[1,1]}}
            ]
        });
        let limits = InputLimits {
            max_geo_features: 1,
            ..InputLimits::default()
        };
        let error = super::parse_geojson(&feature_collection, None, &limits).unwrap_err();
        assert!(error.contains("feature count"), "unexpected error: {error}");

        let two_record_geometries = json!([
            {"shape":{"type":"Point", "coordinates":[0,0]}},
            {"shape":{"type":"Point", "coordinates":[1,1]}}
        ]);
        let error =
            super::parse_geojson(&two_record_geometries, Some("shape"), &limits).unwrap_err();
        assert!(
            error.contains("feature count"),
            "record geometry features must use the same cap: {error}"
        );

        let one_point = json!({
            "type":"Feature", "properties":{},
            "geometry":{"type":"Point", "coordinates":[0,0]}
        });
        let limits = InputLimits {
            max_geo_vertices: 0,
            ..InputLimits::default()
        };
        let error = super::parse_geojson(&one_point, None, &limits).unwrap_err();
        assert!(
            error.contains("coordinate vertex count"),
            "unexpected error: {error}"
        );

        let two_parts = json!([{
            "shape":{"type":"GeometryCollection", "geometries":[
                {"type":"Point", "coordinates":[0,0]},
                {"type":"Point", "coordinates":[1,1]}
            ]}
        }]);
        let limits = InputLimits {
            max_geo_primitives: 1,
            ..InputLimits::default()
        };
        let error = super::parse_geojson(&two_parts, Some("shape"), &limits).unwrap_err();
        assert!(
            error.contains("primitive count"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn all_vl_projection_types_return_finite_coordinates() {
        use GeoProjectionType as P;

        let projections = [
            P::Albers,
            P::AlbersUsa,
            P::AzimuthalEqualArea,
            P::AzimuthalEquidistant,
            P::ConicConformal,
            P::ConicEqualArea,
            P::ConicEquidistant,
            P::EqualEarth,
            P::Equirectangular,
            P::Gnomonic,
            P::Identity,
            P::Mercator,
            P::NaturalEarth1,
            P::Orthographic,
            P::Stereographic,
            P::TransverseMercator,
        ];
        assert_eq!(projections.len(), 16);

        for projection_type in projections {
            let coordinates = if projection_type == P::AlbersUsa {
                vec![
                    [-122.0, 30.0],
                    [-110.0, 30.0],
                    [-110.0, 42.0],
                    [-122.0, 42.0],
                    [-122.0, 30.0],
                ]
            } else {
                vec![
                    [-5.0, -5.0],
                    [5.0, -5.0],
                    [5.0, 5.0],
                    [-5.0, 5.0],
                    [-5.0, -5.0],
                ]
            };
            let shape = GeoShape {
                features: vec![GeoFeature {
                    geometry: Some(GeoGeometry::Polygon(vec![coordinates])),
                    fill: None,
                }],
                projection: GeoProjection {
                    projection_type,
                    ..GeoProjection::default()
                },
                style: GeoShapeStyle::default(),
            };
            let projected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::project_features(
                    &shape,
                    crate::scene::ClipRect {
                        x: 0.0,
                        y: 0.0,
                        w: 320.0,
                        h: 200.0,
                    },
                )
            }))
            .unwrap_or_else(|_| panic!("{projection_type:?} projection panicked"))
            .unwrap_or_else(|error| panic!("{projection_type:?}: {error}"));
            assert!(
                !projected.is_empty(),
                "{projection_type:?} emitted no feature"
            );
            for feature in projected {
                for geometry in feature.geometries {
                    match geometry {
                        super::ProjectedGeometry::Path { d, .. } => {
                            for token in d.split_whitespace() {
                                if matches!(token, "M" | "L" | "Z") {
                                    continue;
                                } else {
                                    let value = token.parse::<f64>().unwrap_or_else(|error| {
                                        panic!("{projection_type:?} path token {token:?} in {d:?}: {error}")
                                    });
                                    assert!(
                                        value.is_finite(),
                                        "{projection_type:?} emitted {value}"
                                    );
                                }
                            }
                        }
                        super::ProjectedGeometry::Point { x, y } => {
                            assert!(x.is_finite() && y.is_finite(), "{projection_type:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn transverse_mercator_applies_clip_angle_and_precision() {
        let shape = GeoShape {
            features: vec![GeoFeature {
                geometry: Some(GeoGeometry::Polygon(vec![vec![
                    [-5.0, -5.0],
                    [5.0, -5.0],
                    [5.0, 5.0],
                    [-5.0, 5.0],
                    [-5.0, -5.0],
                ]])),
                fill: None,
            }],
            projection: GeoProjection {
                projection_type: GeoProjectionType::TransverseMercator,
                clip_angle: Some(60.0),
                precision: Some(0.25),
                ..GeoProjection::default()
            },
            style: GeoShapeStyle::default(),
        };
        let projected = super::project_features(
            &shape,
            crate::scene::ClipRect {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 200.0,
            },
        )
        .expect("transverse Mercator clip and precision settings should be applied");
        assert!(projected.iter().flat_map(|feature| &feature.geometries).any(
            |geometry| matches!(geometry, super::ProjectedGeometry::Path { d, .. } if !d.is_empty())
        ));
    }

    #[test]
    fn clips_projection_horizon_without_non_finite_path() {
        let shape = GeoShape {
            features: vec![GeoFeature {
                geometry: Some(GeoGeometry::Polygon(vec![vec![
                    [30.0, -20.0],
                    [60.0, -20.0],
                    [60.0, 20.0],
                    [30.0, 20.0],
                    [30.0, -20.0],
                ]])),
                fill: None,
            }],
            projection: GeoProjection {
                projection_type: GeoProjectionType::Orthographic,
                clip_angle: Some(45.0),
                ..GeoProjection::default()
            },
            style: GeoShapeStyle::default(),
        };
        let projected = super::project_features(
            &shape,
            crate::scene::ClipRect {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 200.0,
            },
        )
        .expect("horizon clipping must return a finite path");
        let paths = projected
            .iter()
            .flat_map(|feature| &feature.geometries)
            .filter_map(|geometry| match geometry {
                super::ProjectedGeometry::Path { d, .. } => Some(d),
                super::ProjectedGeometry::Point { .. } => None,
            })
            .collect::<Vec<_>>();
        assert!(!paths.is_empty(), "visible polygon portion should remain");
        for path in paths {
            assert!(
                path.split_whitespace()
                    .filter(|token| !matches!(*token, "M" | "L" | "Z"))
                    .all(|token| token.parse::<f64>().is_ok_and(f64::is_finite)),
                "non-finite projected path: {path}"
            );
        }
    }

    #[test]
    fn albers_usa_does_not_join_separate_inset_segments() {
        let shape = GeoShape {
            features: vec![GeoFeature {
                geometry: Some(GeoGeometry::LineString(vec![
                    [-149.0, 61.0],
                    [-157.0, 21.0],
                ])),
                fill: None,
            }],
            projection: GeoProjection {
                projection_type: GeoProjectionType::AlbersUsa,
                ..GeoProjection::default()
            },
            style: GeoShapeStyle::default(),
        };
        let projected = super::project_features(
            &shape,
            crate::scene::ClipRect {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 200.0,
            },
        )
        .expect("albersUsa should clip and project each inset independently");
        let path_count = projected
            .iter()
            .flat_map(|feature| &feature.geometries)
            .filter(|geometry| {
                matches!(
                    geometry,
                    super::ProjectedGeometry::Path { d, .. } if !d.is_empty()
                )
            })
            .count();
        assert!(
            path_count >= 2,
            "the Alaska and Hawaii portions must stay as separate paths, got {path_count}"
        );
    }

    #[test]
    fn precision_zero_is_bounded_by_the_projected_vertex_limit() {
        let line = vec![[-20.0, -80.0], [20.0, 80.0]];
        let shape = GeoShape {
            features: vec![GeoFeature {
                geometry: Some(GeoGeometry::MultiLineString(vec![line; 40])),
                fill: None,
            }],
            projection: GeoProjection {
                projection_type: GeoProjectionType::Equirectangular,
                precision: Some(0.0),
                ..GeoProjection::default()
            },
            style: GeoShapeStyle::default(),
        };
        let error = super::project_features(
            &shape,
            crate::scene::ClipRect {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 200.0,
            },
        )
        .expect_err("zero precision must not permit unbounded resampling output");
        assert!(
            error.contains("projection precision 0 may exceed projected coordinate vertex limit"),
            "{error}"
        );
    }
}
