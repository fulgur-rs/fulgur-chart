//! DSL フロントエンド。各 DSL を IR(ChartSpec) に変換する。
pub mod chartjs;
pub mod vegalite;
mod vegalite_boxplot;
mod vegalite_composition;
mod vegalite_error;

/// Reject oversized dash arrays before a Vega-Lite parser copies their numeric entries.
pub(crate) fn preflight_vegalite_stroke_dash_lengths(
    mark: Option<&serde_json::Value>,
    parts: &[&str],
    mark_name: &str,
) -> Result<(), String> {
    let Some(mark) = mark.and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    for part in parts {
        let Some(pattern) = mark
            .get(*part)
            .and_then(serde_json::Value::as_object)
            .and_then(|style| style.get("strokeDash"))
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        if pattern.len() > crate::guard::MAX_BORDER_DASH_ELEMENTS {
            return Err(format!(
                "{mark_name} {part}.strokeDash must contain at most {} entries",
                crate::guard::MAX_BORDER_DASH_ELEMENTS
            ));
        }
    }
    Ok(())
}

/// Distinguishes invalid input from strict-only validation failures for language bindings.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    Parse(String),
    Strict(String),
}

/// Parse once on successful strict input, retaining the bindings' parse-error precedence.
/// An invalid strict input is retried without strict checking only to classify its failure;
/// malformed data remains a parse error even if it also contains unknown keys.
pub fn parse_with_error_kind(
    json: &str,
    dsl: &str,
    strict: bool,
) -> Result<crate::ir::ChartSpec, ParseError> {
    let parse = |strict| match dsl {
        "vegalite" => vegalite::parse(json, strict),
        _ => chartjs::parse(json, strict),
    };
    match parse(strict) {
        Ok(spec) => Ok(spec),
        Err(error) if strict => match parse(false) {
            Err(error) => Err(ParseError::Parse(error)),
            Ok(_) => Err(ParseError::Strict(error)),
        },
        Err(error) => Err(ParseError::Parse(error)),
    }
}

/// Detect the input DSL by top-level key presence, with mark taking precedence over type.
/// Values (including null) do not affect detection; invalid JSON or absent keys return None.
pub fn detect_dsl(json: &str) -> Option<&'static str> {
    use serde::Deserializer;
    use serde::de::{MapAccess, Visitor};
    struct KeysVisitor;
    impl<'de> Visitor<'de> for KeysVisitor {
        type Value = Option<&'static str>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a chart object")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut has_mark = false;
            let mut has_type = false;
            while let Some(key) = map.next_key::<String>()? {
                has_mark |= key == "mark";
                has_type |= key == "type";
                map.next_value::<ValidatedIgnored>()?;
            }
            Ok(if has_mark {
                Some("vegalite")
            } else if has_type {
                Some("chartjs")
            } else {
                None
            })
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let dsl = deserializer.deserialize_map(KeysVisitor).ok()?;
    deserializer.end().ok()?;
    dsl
}

/// Discard JSON values while preserving Value's number-range and recursion checks.
/// deserialize_ignored_any skips these checks in serde_json, so scanners must use
/// deserialize_any and recursively consume containers instead.
struct ValidatedIgnored;

impl<'de> serde::Deserialize<'de> for ValidatedIgnored {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DiscardVisitor;
        impl<'de> serde::de::Visitor<'de> for DiscardVisitor {
            type Value = ValidatedIgnored;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON value")
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(ValidatedIgnored)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(ValidatedIgnored)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(ValidatedIgnored)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(ValidatedIgnored)
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(ValidatedIgnored)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(ValidatedIgnored)
            }
            fn visit_seq<S: serde::de::SeqAccess<'de>>(
                self,
                mut seq: S,
            ) -> Result<Self::Value, S::Error> {
                while seq.next_element::<ValidatedIgnored>()?.is_some() {}
                Ok(ValidatedIgnored)
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                while map.next_key::<ValidatedIgnored>()?.is_some() {
                    map.next_value::<ValidatedIgnored>()?;
                }
                Ok(ValidatedIgnored)
            }
        }
        deserializer.deserialize_any(DiscardVisitor)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn discarded_json_values_reject_non_json_binary_values() {
        let deserializer =
            serde::de::value::BytesDeserializer::<serde::de::value::Error>::new(b"data");
        let result: Result<super::ValidatedIgnored, _> =
            serde::Deserialize::deserialize(deserializer);
        assert_eq!(
            result.err().unwrap().to_string(),
            "invalid type: byte array, expected a JSON value"
        );
    }
}
