use crate::guard::InputLimits;
use crate::ir::GeoGeometry;
use serde_json::{Map, Value};

const MAX_GEOMETRY_COLLECTION_DEPTH: usize = 64;

/// GeoJSON geometry paired with its source record and feature properties.
#[derive(Clone, Debug, PartialEq)]
pub struct RawGeoFeature {
    pub geometry: Option<GeoGeometry>,
    pub record: Map<String, Value>,
    pub properties: Map<String, Value>,
}

#[derive(Default)]
struct Counts {
    features: usize,
    vertices: usize,
    primitives: usize,
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
        Some(_) => Err(at(path, "expected a GeoJSON Feature or a record object")),
        None => {
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
        ) => preflight_geometry(value, limits, counts, path, 0),
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
        Some("Feature") => parse_feature(value, Map::new(), features, path),
        Some("FeatureCollection") => parse_feature_collection(value, Map::new(), features, path),
        Some(_) => Err(at(path, "expected a GeoJSON Feature or a record object")),
        None => {
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
            parse_shape(shape, record, features, &shape_path)
        }
    }
}

fn parse_shape(
    value: &Value,
    record: Map<String, Value>,
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
    record: Map<String, Value>,
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
    record: Map<String, Value>,
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
    use serde_json::json;

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
}
