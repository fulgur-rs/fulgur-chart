//! Vega-Lite spec の最小サブセットを IR(ChartSpec) へ変換する。
//!
//! 対応する上位形:
//! `{ "mark": ..., "data": {"values": [ {..}, .. ]}, "encoding": {..} }`
//!
//! - `mark`: 文字列 `"bar"|"line"|"point"|"arc"` または `{"type": "<同左>"}`。
//! - `data.values`: インラインのレコード配列（JSON オブジェクトの配列）のみ対応。
//!   `data.url` や values 欠落は明確なメッセージで Err。
//! - `encoding`: `x`/`y`/`color`/`theta`。各チャネルは `{ "field", "type"? }`。
//!
//! すべて決定的（distinct 値の抽出は first-seen 順、HashMap 不使用）でパニックしない。

use crate::ir::*;
use crate::palette::vegalite_theme;
use serde_json::{Map, Value};

/// Vega-Lite サブセットを [`ChartSpec`] へ変換する。
///
/// `strict` が真のとき、上位/encoding/各チャネルのキーをホワイトリストで検査し、
/// 最初の未知キーをそのパス付きで Err にする。非 strict は未知キーを無視する。
pub fn parse(json: &str, strict: bool) -> Result<ChartSpec, String> {
    if strict {
        check_unknown_keys(json)?;
    }

    let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let top = value
        .as_object()
        .ok_or_else(|| "トップレベルは object でなければなりません".to_string())?;

    let kind = parse_mark(top.get("mark"))?;
    let records = parse_data_values(top.get("data"))?;
    let encoding = top
        .get("encoding")
        .and_then(Value::as_object)
        .ok_or_else(|| "encoding がありません".to_string())?;

    let x_field = channel_field(encoding, "x");
    let y_field = channel_field(encoding, "y");
    let color_field = channel_field(encoding, "color");
    let theta_field = channel_field(encoding, "theta");

    // 必須 encoding field の指定・存在・型を検証する。これを通せば field_f64/
    // field_category が 0/空へ黙って丸めることはなくなる(typo・欠損・型違いを明示エラーに)。
    match &kind {
        ChartKind::Bar { .. } | ChartKind::Line => {
            let xf = require_field(&x_field, "x")?;
            let yf = require_field(&y_field, "y")?;
            validate_category(&records, xf)?;
            validate_numeric(&records, yf)?;
            if let Some(cf) = color_field.as_deref() {
                validate_category(&records, cf)?;
            }
        }
        ChartKind::Scatter => {
            let xf = require_field(&x_field, "x")?;
            let yf = require_field(&y_field, "y")?;
            validate_numeric(&records, xf)?;
            validate_numeric(&records, yf)?;
            if let Some(cf) = color_field.as_deref() {
                validate_category(&records, cf)?;
            }
        }
        ChartKind::Pie { .. } => {
            // 値は theta(無ければ y)、カテゴリは color(無ければ x)。
            let vf = theta_field
                .as_deref()
                .or(y_field.as_deref())
                .ok_or_else(|| {
                    "arc には encoding.theta.field または y.field が必要です".to_string()
                })?;
            validate_numeric(&records, vf)?;
            let cf = color_field
                .as_deref()
                .or(x_field.as_deref())
                .ok_or_else(|| {
                    "arc には encoding.color.field または x.field が必要です".to_string()
                })?;
            validate_category(&records, cf)?;
        }
        // Bubble/Radar/Mixed は Vega-Lite mark から生成されない。
        _ => {}
    }

    // 色分け line で疎なカテゴリ(一部 (category,color) 組が欠落)は、欠損を 0 埋めすると
    // 実在しないゼロ点へ折れ線が接続され誤りになる。IR は欠損表現を持たないため拒否する。
    if matches!(kind, ChartKind::Line) && color_field.is_some() {
        let cats = distinct_categories(&records, x_field.as_deref());
        let groups = distinct_categories(&records, color_field.as_deref());
        for group in &groups {
            for cat in &cats {
                let present = records.iter().any(|r| {
                    &field_category(r, x_field.as_deref()) == cat
                        && &field_category(r, color_field.as_deref()) == group
                });
                if !present {
                    return Err(
                        "色分け折れ線(line + color)は全カテゴリに値が揃ったデータのみ対応です(疎なデータは未対応)"
                            .to_string(),
                    );
                }
            }
        }
    }

    let theme = vegalite_theme();

    let series = match &kind {
        ChartKind::Pie { .. } => build_pie(
            &records,
            &x_field,
            &y_field,
            &color_field,
            &theta_field,
            &theme,
        ),
        ChartKind::Scatter => build_scatter(&records, &x_field, &y_field, &color_field, &theme),
        _ => build_categorical(&kind, &records, &x_field, &y_field, &color_field, &theme),
    };

    let categories = match &kind {
        ChartKind::Pie { .. } => {
            // pie のカテゴリは color.field 優先、なければ x.field。
            let cat_field = color_field.as_deref().or(x_field.as_deref());
            distinct_categories(&records, cat_field)
        }
        ChartKind::Scatter => vec![],
        _ => distinct_categories(&records, x_field.as_deref()),
    };

    // scatter は両軸ゼロ起点を強制しない。bar/line/pie は y のみゼロ起点（chartjs と一致）。
    let y_begin_at_zero = !matches!(kind, ChartKind::Scatter);

    // VL トップレベルの width/height/title を反映する(無ければ既定 800x450・無題)。
    // title は文字列、または `{"text": "..."}` オブジェクトを受ける。
    let width = top
        .get("width")
        .and_then(Value::as_f64)
        .filter(|w| w.is_finite() && *w > 0.0)
        .unwrap_or(800.0);
    let height = top
        .get("height")
        .and_then(Value::as_f64)
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(450.0);
    let title = match top.get("title") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Object(o)) => o
            .get("text")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_string),
        _ => None,
    };

    Ok(ChartSpec {
        kind,
        series,
        categories,
        x_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None, // Vega-Lite の scale.domainMin は未実装
            suggested_max: None, // Vega-Lite の scale.domainMax は未実装
            begin_at_zero: false,
            offset: false,
            grid: true,
        },
        y_axis: AxisSpec {
            title: None,
            min: None,
            max: None,
            suggested_min: None, // Vega-Lite の scale.domainMin は未実装
            suggested_max: None, // Vega-Lite の scale.domainMax は未実装
            begin_at_zero: y_begin_at_zero,
            offset: false,
            grid: true,
        },
        legend: LegendPos::Top,
        title,
        width,
        height,
        data_labels: false,
        theme,
        decimation: Decimation::default(),
    })
}

/// `mark` を [`ChartKind`] へ。文字列または `{"type": "<同左>"}` を受理する。
fn parse_mark(mark: Option<&Value>) -> Result<ChartKind, String> {
    let mark = mark.ok_or_else(|| "mark がありません".to_string())?;
    let name = match mark {
        Value::String(s) => s.as_str(),
        Value::Object(o) => o
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "mark.type がありません".to_string())?,
        _ => return Err("mark は文字列または object でなければなりません".to_string()),
    };
    match name {
        "bar" => Ok(ChartKind::Bar {
            horizontal: false,
            placement_stacked: false,
            value_stacked: false,
        }),
        "line" => Ok(ChartKind::Line),
        "point" => Ok(ChartKind::Scatter),
        "arc" => Ok(ChartKind::Pie { donut_ratio: 0.0 }),
        other => Err(format!("未対応の mark: {other}")),
    }
}

/// `data.values` をレコード配列として取り出す。URL データや values 欠落は Err。
fn parse_data_values(data: Option<&Value>) -> Result<Vec<Map<String, Value>>, String> {
    let data = data
        .and_then(Value::as_object)
        .ok_or_else(|| "data がありません".to_string())?;
    if data.contains_key("url") {
        return Err("data.url(URL データ)は未対応です。data.values を使ってください".to_string());
    }
    let values = data
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "data.values(インライン配列)がありません".to_string())?;
    let mut records = Vec::with_capacity(values.len());
    for v in values {
        let obj = v
            .as_object()
            .ok_or_else(|| "data.values の各要素は object でなければなりません".to_string())?;
        records.push(obj.clone());
    }
    Ok(records)
}

/// encoding チャネルの `field` 文字列を取り出す（なければ None）。
fn channel_field(encoding: &Map<String, Value>, channel: &str) -> Option<String> {
    encoding
        .get(channel)
        .and_then(Value::as_object)
        .and_then(|o| o.get("field"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// 必須チャネルの field 指定を取り出す。未指定なら Err。
fn require_field<'a>(field: &'a Option<String>, channel: &str) -> Result<&'a str, String> {
    field
        .as_deref()
        .ok_or_else(|| format!("encoding.{channel}.field が必要です"))
}

/// 参照フィールドが全レコードに存在し、かつカテゴリ値(文字列/数値/真偽)であることを
/// 検証する。null/object/array は field_category が "" に丸めて別カテゴリを空へ統合する
/// 誤りを生むため、欠落・非カテゴリ型は明示エラーにする。
fn validate_category(records: &[Map<String, Value>], field: &str) -> Result<(), String> {
    for r in records {
        match r.get(field) {
            Some(Value::String(_) | Value::Number(_) | Value::Bool(_)) => {}
            Some(_) => {
                return Err(format!(
                    "フィールド {field} はカテゴリ値(文字列/数値/真偽)である必要があります"
                ));
            }
            None => return Err(format!("フィールド {field} が見つかりません(typo?)")),
        }
    }
    Ok(())
}

/// 参照フィールドが全レコードに存在し、かつ数値であることを検証する。
/// 欠落・非数値は 0.0 へ黙って丸めず明示エラーにする。
fn validate_numeric(records: &[Map<String, Value>], field: &str) -> Result<(), String> {
    for r in records {
        match r.get(field) {
            Some(Value::Number(_)) => {}
            Some(_) => return Err(format!("フィールド {field} は数値である必要があります")),
            None => return Err(format!("フィールド {field} が見つかりません(typo?)")),
        }
    }
    Ok(())
}

/// レコードの指定フィールドを f64 として読む。数値でない/欠落は 0.0。
fn field_f64(record: &Map<String, Value>, field: Option<&str>) -> f64 {
    field
        .and_then(|f| record.get(f))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

/// レコードの指定フィールドをカテゴリ文字列として読む。
/// 文字列はそのまま、数値はその文字列表現、それ以外/欠落は ""。
fn field_category(record: &Map<String, Value>, field: Option<&str>) -> String {
    match field.and_then(|f| record.get(f)) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// 指定フィールドの distinct 値を first-seen 順で返す（Vec ベースで決定的）。
fn distinct_categories(records: &[Map<String, Value>], field: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in records {
        let v = field_category(r, field);
        if !out.iter().any(|x| x == &v) {
            out.push(v);
        }
    }
    out
}

/// パレットから i 番目の色を巡回で取り出す。
fn palette_pick(palette: &[Color], i: usize) -> Color {
    palette[i % palette.len()]
}

/// bar/line（カテゴリ系）の系列を組む。
/// color.field があれば色値ごとに 1 系列、なければ単一系列。
/// `series.values[k]` = x==categories[k] かつ color が一致するレコードの y 合計。
fn build_categorical(
    kind: &ChartKind,
    records: &[Map<String, Value>],
    x_field: &Option<String>,
    y_field: &Option<String>,
    color_field: &Option<String>,
    theme: &Theme,
) -> Vec<Series> {
    let categories = distinct_categories(records, x_field.as_deref());
    let series_type = match kind {
        ChartKind::Line => SeriesType::Line,
        _ => SeriesType::Bar,
    };
    let stroke_width = match series_type {
        SeriesType::Line => 3.0,
        SeriesType::Bar => 1.0,
    };

    // 系列名（= color 値）の集合。color なしなら y.field 名（なければ ""）の単一系列。
    let group_names: Vec<String> = match color_field {
        Some(_) => distinct_categories(records, color_field.as_deref()),
        None => vec![y_field.clone().unwrap_or_default()],
    };

    group_names
        .iter()
        .enumerate()
        .map(|(si, group)| {
            let values: Vec<f64> = categories
                .iter()
                .map(|cat| {
                    records
                        .iter()
                        .filter(|r| {
                            &field_category(r, x_field.as_deref()) == cat
                                && match color_field {
                                    Some(_) => &field_category(r, color_field.as_deref()) == group,
                                    None => true,
                                }
                        })
                        .map(|r| field_f64(r, y_field.as_deref()))
                        .sum()
                })
                .collect();
            let color = palette_pick(&theme.palette, si);
            Series {
                name: group.clone(),
                values,
                points: vec![],
                fill: vec![color],
                stroke: vec![color],
                stroke_width,
                area: false,
                tension: 0.0,
                series_type,
                point_radius: None,
                box_points: vec![],
                tree: vec![],
                links: vec![],
            }
        })
        .collect()
}

/// point（scatter）の系列を組む。
/// color.field があれば色値ごとに 1 系列、なければ全点を単一系列に。
fn build_scatter(
    records: &[Map<String, Value>],
    x_field: &Option<String>,
    y_field: &Option<String>,
    color_field: &Option<String>,
    theme: &Theme,
) -> Vec<Series> {
    let group_names: Vec<String> = match color_field {
        Some(_) => distinct_categories(records, color_field.as_deref()),
        None => vec![String::new()],
    };

    group_names
        .iter()
        .enumerate()
        .map(|(si, group)| {
            let points: Vec<Point> = records
                .iter()
                .filter(|r| match color_field {
                    Some(_) => &field_category(r, color_field.as_deref()) == group,
                    None => true,
                })
                .map(|r| Point {
                    x: field_f64(r, x_field.as_deref()),
                    y: field_f64(r, y_field.as_deref()),
                    r: None,
                })
                .collect();
            let color = palette_pick(&theme.palette, si);
            Series {
                name: group.clone(),
                values: vec![],
                points,
                fill: vec![color],
                stroke: vec![color],
                stroke_width: 1.0,
                area: false,
                tension: 0.0,
                series_type: SeriesType::Bar,
                point_radius: None,
                box_points: vec![],
                tree: vec![],
                links: vec![],
            }
        })
        .collect()
}

/// arc（pie）の単一系列を組む。カテゴリは color.field 優先（なければ x.field）。
/// 各スライス値 = theta.field の合計（なければ y.field）。色はスライス別パレット。
fn build_pie(
    records: &[Map<String, Value>],
    x_field: &Option<String>,
    y_field: &Option<String>,
    color_field: &Option<String>,
    theta_field: &Option<String>,
    theme: &Theme,
) -> Vec<Series> {
    let cat_field = color_field.as_deref().or(x_field.as_deref());
    let categories = distinct_categories(records, cat_field);
    // スライス値のフィールドは theta 優先、なければ y。
    let value_field = theta_field.as_deref().or(y_field.as_deref());

    let values: Vec<f64> = categories
        .iter()
        .map(|cat| {
            records
                .iter()
                .filter(|r| &field_category(r, cat_field) == cat)
                .map(|r| field_f64(r, value_field))
                .sum()
        })
        .collect();

    let n = categories.len();
    let colors: Vec<Color> = (0..n).map(|i| palette_pick(&theme.palette, i)).collect();

    vec![Series {
        name: String::new(),
        values,
        points: vec![],
        fill: colors.clone(),
        stroke: colors,
        stroke_width: 1.0,
        area: false,
        tension: 0.0,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: vec![],
        links: vec![],
    }]
}

/// strict 用: 既知キーのホワイトリストに照らし、最初の未知キーをパス付き Err で返す。
/// 防御的に走査し、ノードが欠落/想定外の形なら Ok を返す（後段の通常パースに委ねる）。
fn check_unknown_keys(json: &str) -> Result<(), String> {
    let value: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()), // 不正 JSON は後段パースに委ねる
    };
    let Some(top) = value.as_object() else {
        return Ok(()); // object でなければ後段パースに委ねる
    };

    check_object(
        top,
        &[
            "mark", "data", "encoding", "$schema", "width", "height", "title",
        ],
        "",
    )?;

    if let Some(encoding) = top.get("encoding").and_then(Value::as_object) {
        check_object(encoding, &["x", "y", "color", "theta"], "encoding")?;
        for channel in ["x", "y", "color", "theta"] {
            if let Some(ch) = encoding.get(channel).and_then(Value::as_object) {
                // aggregate は未実装(本体は単純合計しかしない)。strict では
                // 誤った集計結果を黙って返さないよう、未対応キーとして拒否する。
                check_object(ch, &["field", "type"], &format!("encoding.{channel}"))?;
            }
        }
    }

    Ok(())
}

/// `obj` のキーを `allowed` に照らし、最初の未知キーを `Err(パス)` で返す。
fn check_object(obj: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<(), String> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            let full = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            return Err(format!("未知のキー: {full}"));
        }
    }
    Ok(())
}
