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

    let mut kind = parse_mark(top.get("mark"))?;
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
        ChartKind::VegaRect { .. } => {
            let xf = require_field(&x_field, "x")?;
            let yf = require_field(&y_field, "y")?;
            let cf = require_field(&color_field, "color")?;
            validate_category(&records, xf)?;
            validate_category(&records, yf)?;
            // color は数値または文字列のカテゴリ(quantitative/nominal を後段で判定)。
            // 存在だけ確認、型は build_rect で扱う。
            for r in &records {
                match r.get(cf) {
                    Some(Value::Number(_) | Value::String(_) | Value::Bool(_)) => {}
                    Some(Value::Null) | None => {
                        return Err(format!(
                            "フィールド {cf} が見つかりません(typo? または null)"
                        ));
                    }
                    Some(_) => {
                        return Err(format!(
                            "フィールド {cf} は数値/文字列/真偽である必要があります"
                        ));
                    }
                }
            }
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

    // rect ヒートマップの場合、kind に cells を差し替え、series/categories は空。
    if matches!(kind, ChartKind::VegaRect { .. }) {
        let color_type_hint = encoding
            .get("color")
            .and_then(Value::as_object)
            .and_then(|o| o.get("type"))
            .and_then(Value::as_str);
        // color.field は上流 validation で確認済みだが、"パニックしない" invariant を
        // 守るため require_field で Result 伝播する(実質 unreachable)。
        let cf = require_field(&color_field, "color")?;
        let color_type = infer_color_type(&records, cf, color_type_hint);
        // encoding.color.type: "quantitative" が指定されたのに、どのレコードも
        // 有限数値を持たないと build_rect 内で全セル None になり黙って空チャートを
        // 返す。build_rect の push 条件 (`Value::as_f64` && `is_finite()`) を裏返し、
        // 「全 bucket 空になる」ケースだけを明示 Err にする。
        // (validate_numeric ではなく any-finite ガードなのは、
        // Aggregate::None が「非数値/NaN を挟んでも最後の有限数値を残す」
        // 挙動をサポートしている(既存テストで pin 済み)ため。混在データ耐性は
        // 意図的なので、grid が実際にブランクになる場合のみ拒否する。)
        if color_type == ColorType::Quantitative
            && !records.is_empty()
            && !records.iter().any(|r| {
                r.get(cf)
                    .and_then(Value::as_f64)
                    .is_some_and(f64::is_finite)
            })
        {
            return Err(format!("フィールド {cf} は数値である必要があります"));
        }
        let aggregate_hint = encoding
            .get("color")
            .and_then(Value::as_object)
            .and_then(|o| o.get("aggregate"))
            .and_then(Value::as_str);
        let aggregate = match aggregate_hint {
            Some("mean") => Aggregate::Mean,
            Some("sum") => Aggregate::Sum,
            // 非 strict では未対応値 (count / min / max / median 等) も無視して
            // Aggregate::None (last-write-wins) 扱い。strict Err は Task 6 で追加。
            _ => Aggregate::None,
        };
        // strict: aggregate は quantitative color でのみ許容。type 省略時に実データから
        // nominal と推論された場合も aggregate は無効(集計対象がカテゴリ)。explicit
        // nominal + aggregate は check_unknown_keys で既に reject されているが、推論
        // のケースはここで catch する。
        if strict && aggregate != Aggregate::None && color_type == ColorType::Nominal {
            return Err(format!(
                "rect の color は実データから nominal と推論されました。aggregate: \"{}\" は指定できません(集計対象がカテゴリ)",
                aggregate_hint.unwrap_or("")
            ));
        }
        kind = parse_rect_kind(
            &records,
            &x_field,
            &y_field,
            &color_field,
            color_type,
            aggregate,
            &theme.palette,
        )?;
    }

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
        ChartKind::VegaRect { .. } => vec![],
        _ => build_categorical(&kind, &records, &x_field, &y_field, &color_field, &theme),
    };

    let categories = match &kind {
        ChartKind::Pie { .. } => {
            // pie のカテゴリは color.field 優先、なければ x.field。
            let cat_field = color_field.as_deref().or(x_field.as_deref());
            distinct_categories(&records, cat_field)
        }
        ChartKind::Scatter => vec![],
        ChartKind::VegaRect { .. } => vec![],
        _ => distinct_categories(&records, x_field.as_deref()),
    };

    // scatter/rect は両軸ゼロ起点を強制しない。bar/line/pie は y のみゼロ起点（chartjs と一致）。
    let y_begin_at_zero = !matches!(kind, ChartKind::Scatter | ChartKind::VegaRect { .. });

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
        "circle" => Ok(ChartKind::Scatter),
        "rect" => Ok(ChartKind::VegaRect {
            x_labels: Vec::new(),
            y_labels: Vec::new(),
            cells: Vec::new(),
        }),
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
        // mark 別 encoding allow-list を選ぶ。mark 名が読めない/未対応なら
        // 現状挙動(全キー拒否せずスルー)を保つ = 後段パースに委ねる。
        let allowed: &[&str] = match read_mark_name(top) {
            Some("bar" | "line" | "point" | "circle") => &["x", "y", "color"],
            Some("arc") => &["theta", "color", "x", "y"],
            Some("rect") => &["x", "y", "color"],
            _ => return Ok(()),
        };
        check_object(encoding, allowed, "encoding")?;
        for channel in allowed {
            if let Some(ch) = encoding.get(*channel).and_then(Value::as_object) {
                // aggregate は原則未実装(本体は単純合計しかしない)。strict では
                // 誤った集計結果を黙って返さないよう、未対応キーとして拒否する。
                // ただし rect の color チャネルに限り、mean/sum のみ受理する
                // (aggregate の値のバリデーションは下の rect 固有ブロックで行う)。
                let channel_allowed: &[&str] =
                    if matches!(read_mark_name(top), Some("rect")) && *channel == "color" {
                        &["field", "type", "aggregate"]
                    } else {
                        &["field", "type"]
                    };
                check_object(ch, channel_allowed, &format!("encoding.{channel}"))?;
            }
        }

        // rect 固有の strict チェック:
        // - x/y encoding の type: "quantitative" は binned ヒートマップ想定で MVP 外。
        // - color aggregate は "mean"/"sum" のみ受理。
        // - nominal/ordinal color + aggregate は同時指定不可(集計対象がカテゴリ)。
        if matches!(read_mark_name(top), Some("rect")) {
            for axis in ["x", "y"] {
                if let Some(ch) = encoding.get(axis).and_then(Value::as_object)
                    && let Some(t) = ch.get("type").and_then(Value::as_str)
                    && t == "quantitative"
                {
                    return Err(format!(
                        "rect の encoding.{axis}.type: \"quantitative\" は未対応です(binned ヒートマップは別 issue)"
                    ));
                }
            }
            if let Some(color) = encoding.get("color").and_then(Value::as_object) {
                // aggregate は文字列 "mean" | "sum" のみ受理。非文字列(例: 数値)を
                // silently 無視すると集計指定が黙って落ちるため、明示 Err にする。
                let agg = match color.get("aggregate") {
                    None => None,
                    Some(Value::String(s)) if s == "mean" || s == "sum" => Some(s.as_str()),
                    Some(Value::String(s)) => {
                        return Err(format!(
                            "rect の encoding.color.aggregate: \"{s}\" は未対応です(mean/sum のみ)"
                        ));
                    }
                    Some(other) => {
                        return Err(format!(
                            "rect の encoding.color.aggregate は文字列 \"mean\" または \"sum\" のみ受理: {other}"
                        ));
                    }
                };
                let color_type = color.get("type").and_then(Value::as_str);
                if let (Some(ct), Some(a)) = (color_type, agg)
                    && (ct == "nominal" || ct == "ordinal")
                {
                    return Err(format!(
                        "rect の encoding.color.type: \"{ct}\" に aggregate: \"{a}\" は指定できません(集計対象がカテゴリ)"
                    ));
                }
            }
        }
    }

    Ok(())
}

/// top.mark の名前を string / object 両形で取り出す。取れなければ None。
fn read_mark_name(top: &Map<String, Value>) -> Option<&str> {
    match top.get("mark")? {
        Value::String(s) => Some(s.as_str()),
        Value::Object(o) => o.get("type").and_then(Value::as_str),
        _ => None,
    }
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

/// rect ヒートマップ用の 2 色補間の endpoint。
/// HI は Vega-Lite テーマの palette[0] (Tableau10 steel-blue #4c78a8) と揃える。
/// nominal 経路も同じパレットを使うため、quantitative と nominal で色系統が一貫する。
/// (chart.js matrix の #36A2EB 定数は Chart.js DSL 経路 (`ChartKind::Matrix`) 側で
/// 独立に保持されており、Vega-Lite rect とは意図的に別テーマとする。)
const RECT_COLOR_LO: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 1.0,
};
const RECT_COLOR_HI: Color = Color {
    r: 76,
    g: 120,
    b: 168,
    a: 1.0,
};

fn lerp_rect_color(t: f64) -> Color {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    Color {
        r: (RECT_COLOR_LO.r as f64 + (RECT_COLOR_HI.r as f64 - RECT_COLOR_LO.r as f64) * t).round()
            as u8,
        g: (RECT_COLOR_LO.g as f64 + (RECT_COLOR_HI.g as f64 - RECT_COLOR_LO.g as f64) * t).round()
            as u8,
        b: (RECT_COLOR_LO.b as f64 + (RECT_COLOR_HI.b as f64 - RECT_COLOR_LO.b as f64) * t).round()
            as u8,
        a: 1.0, // both endpoints are opaque
    }
}

/// rect の color エンコードの型。quantitative は 2 色補間、
/// nominal はパレットのラウンドロビン割当。
#[derive(Clone, Copy, Debug, PartialEq)]
enum ColorType {
    Quantitative,
    Nominal,
}

/// rect color の集約方式。
/// - `None`: encoding.color.aggregate が省略時。同一 (x,y) セルの「最後の有限数値」を採用。
///   非数値/NaN は push 時にフィルタされるため、悪いデータで良いデータが上書きされない。
///   (Task 3/4 の raw last-write-wins からのドリフト。試験で pin 済み。)
/// - `Mean` / `Sum`: encoding.color.aggregate で指定されたときのみ選択。
#[derive(Clone, Copy, Debug, PartialEq)]
enum Aggregate {
    None,
    Mean,
    Sum,
}

/// encoding.color.type と実データから color 型を判定する。
/// - "quantitative" → Quantitative
/// - "nominal" / "ordinal" → Nominal
/// - 省略時 → 全レコードの color 値が数値なら Quantitative、それ以外は Nominal
fn infer_color_type(
    records: &[Map<String, Value>],
    color_field: &str,
    explicit: Option<&str>,
) -> ColorType {
    match explicit {
        Some("quantitative") => ColorType::Quantitative,
        // 注: Vega-Lite 本家では ordinal + rect は単一 hue の sequential scale だが、
        // MVP では nominal と同じカテゴリパレット扱い。sequential 対応は将来の拡張。
        Some("nominal" | "ordinal") => ColorType::Nominal,
        _ => {
            // 空データでは all() が vacuously true → Quantitative になるが、
            // x_labels / y_labels も空なので cells も空、選択は観測不能。
            let all_numeric = records
                .iter()
                .all(|r| matches!(r.get(color_field), Some(Value::Number(_))));
            if all_numeric {
                ColorType::Quantitative
            } else {
                ColorType::Nominal
            }
        }
    }
}

/// Vega-Lite rect の encoding とデータから `ChartKind::VegaRect` を構築する。
/// `parse_mark` は sentinel を返し、この関数が実体で置き換える。
/// x/y/color フィールドは `parse` の validation で確認済みだが、"パニックしない"
/// invariant を守るため require_field で Result 伝播する(実質 unreachable の Err)。
fn parse_rect_kind(
    records: &[Map<String, Value>],
    x_field: &Option<String>,
    y_field: &Option<String>,
    color_field: &Option<String>,
    color_type: ColorType,
    aggregate: Aggregate,
    palette: &[Color],
) -> Result<ChartKind, String> {
    let xf = require_field(x_field, "x")?;
    let yf = require_field(y_field, "y")?;
    let cf = require_field(color_field, "color")?;
    let (x_labels, y_labels, cells) =
        build_rect(records, xf, yf, cf, color_type, aggregate, palette);
    Ok(ChartKind::VegaRect {
        x_labels,
        y_labels,
        cells,
    })
}

/// rect 用の cells / labels を構築する。
/// - x/y の distinct カテゴリを first-seen 順で採取
/// - Quantitative: 各セルの color 値を bucket に蓄積 → `aggregate` 適用 → min/max 2 色補間
/// - Nominal: color カテゴリの first-seen index を palette へラウンドロビン割当
/// - 未出現の (x,y) は None(スキップ)
/// - Quantitative + `Aggregate::None`: 同じ (x,y) は「最後の有限数値」を採用
///   (非数値/NaN は push 時にフィルタ済みなので上書きされない)
/// - Quantitative + `Aggregate::Mean` / `Sum`: 同じ (x,y) の全値を集約
fn build_rect(
    records: &[Map<String, Value>],
    x_field: &str,
    y_field: &str,
    color_field: &str,
    color_type: ColorType,
    aggregate: Aggregate,
    palette: &[Color],
) -> (Vec<String>, Vec<String>, Vec<Vec<Option<Color>>>) {
    let x_labels = distinct_categories(records, Some(x_field));
    let y_labels = distinct_categories(records, Some(y_field));

    match color_type {
        ColorType::Quantitative => {
            // (row, col) → Vec<f64> の bucket に有限な生値を蓄積してから aggregate を適用。
            // `Aggregate::None` は「最後の有限数値」を採用(非数値/NaN は push 時にフィルタ済み)。
            let mut buckets: Vec<Vec<Vec<f64>>> =
                vec![vec![Vec::new(); x_labels.len()]; y_labels.len()];
            for r in records {
                let xk = field_category(r, Some(x_field));
                let yk = field_category(r, Some(y_field));
                let (Some(col), Some(row)) = (
                    x_labels.iter().position(|l| l == &xk),
                    y_labels.iter().position(|l| l == &yk),
                ) else {
                    continue;
                };
                if let Some(v) = r.get(color_field).and_then(Value::as_f64)
                    && v.is_finite()
                {
                    buckets[row][col].push(v);
                }
            }
            // aggregate 適用: 空 bucket は None、そうでなければ mean / sum / last。
            let cell_values: Vec<Vec<Option<f64>>> = buckets
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|b| match (aggregate, b.as_slice()) {
                            (_, []) => None,
                            (Aggregate::Mean, vs) => Some(vs.iter().sum::<f64>() / vs.len() as f64),
                            (Aggregate::Sum, vs) => Some(vs.iter().sum()),
                            // 集約なし: 同一 (x,y) セルの「最後の有限数値」を採用。
                            // Task 3/4 の last-write-wins から微差: 非数値/NaN を挟んだ場合、
                            // 現行は push 時にフィルタするため直前の有限数値が残る
                            // (旧: 非数値/NaN で cell が None にクリアされていた)。
                            // これは望ましい方向のドリフト(悪いデータで良いデータを壊さない)。
                            // 上の [] アームで空を除外済みなので last() は必ず Some。
                            (Aggregate::None, vs) => vs.last().copied(),
                        })
                        .collect()
                })
                .collect();

            // min/max を有限値のみから算出。
            let (mut min_v, mut max_v) = (f64::INFINITY, f64::NEG_INFINITY);
            for row in &cell_values {
                for v in row.iter().flatten() {
                    if v.is_finite() {
                        if *v < min_v {
                            min_v = *v;
                        }
                        if *v > max_v {
                            max_v = *v;
                        }
                    }
                }
            }
            let range = if (max_v - min_v).abs() < f64::EPSILON {
                1.0
            } else {
                max_v - min_v
            };
            let degenerate = !min_v.is_finite() || (max_v - min_v).abs() < f64::EPSILON;

            let cells: Vec<Vec<Option<Color>>> = cell_values
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|v| match v {
                            Some(v) if v.is_finite() => {
                                if degenerate {
                                    // 単一値・全値同一時: 白セルが白背景に埋没しないよう HI を採用。
                                    Some(RECT_COLOR_HI)
                                } else {
                                    Some(lerp_rect_color((*v - min_v) / range))
                                }
                            }
                            _ => None,
                        })
                        .collect()
                })
                .collect();

            (x_labels, y_labels, cells)
        }
        ColorType::Nominal => {
            // 色カテゴリの first-seen 順を採取。cat → palette index (mod len)。
            let color_cats = distinct_categories(records, Some(color_field));
            let mut cells: Vec<Vec<Option<Color>>> =
                vec![vec![None; x_labels.len()]; y_labels.len()];
            for r in records {
                let xk = field_category(r, Some(x_field));
                let yk = field_category(r, Some(y_field));
                let ck = field_category(r, Some(color_field));
                let (Some(col), Some(row), Some(ci)) = (
                    x_labels.iter().position(|l| l == &xk),
                    y_labels.iter().position(|l| l == &yk),
                    color_cats.iter().position(|l| l == &ck),
                ) else {
                    continue;
                };
                cells[row][col] = Some(palette[ci % palette.len()]);
            }
            (x_labels, y_labels, cells)
        }
    }
}
