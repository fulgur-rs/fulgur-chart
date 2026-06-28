use crate::{
    render::{self, OutputFormat},
    state::AppState,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// JSON-RPC 2.0 リクエスト
#[derive(Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

// JSON-RPC 2.0 レスポンス
#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

pub async fn mcp_handler(
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    // MCP notification は id フィールドが存在しない（JSON デシリアライズ後 None）。
    // 仕様上 notification への返信は禁止されているため 200 empty を返す。
    if req.id.is_none() {
        return StatusCode::OK.into_response();
    }

    if req.jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(
                req.id,
                -32600,
                "Invalid Request".into(),
            )),
        )
            .into_response();
    }

    let result = match req.method.as_str() {
        "initialize" => handle_initialize(),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(req.params, state).await,
        _ => Err((-32601, "Method not found".to_string())),
    };

    match result {
        Ok(v) => (StatusCode::OK, Json(JsonRpcResponse::success(req.id, v))).into_response(),
        Err((code, msg)) => (
            StatusCode::OK,
            Json(JsonRpcResponse::error(req.id, code, msg)),
        )
            .into_response(),
    }
}

fn handle_initialize() -> Result<Value, (i64, String)> {
    Ok(json!({
        "protocolVersion": "2025-03-26",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "fulgur-chart-server",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

fn handle_tools_list() -> Result<Value, (i64, String)> {
    Ok(json!({
        "tools": [
            {
                "name": "generate_chart",
                "description": "Render a Chart.js v4 spec to SVG or PNG. Returns SVG string (format=svg) or base64 data URI (format=png/webp/data-uri).",
                "inputSchema": {
                    "type": "object",
                    "required": ["chart"],
                    "properties": {
                        "chart": {
                            "type": "object",
                            "description": "Chart.js v4 spec (type, data, options)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["svg", "png", "webp", "data-uri"],
                            "default": "png",
                            "description": "Output format"
                        },
                        "width": { "type": "integer", "description": "Width in px" },
                        "height": { "type": "integer", "description": "Height in px" }
                    }
                }
            }
        ]
    }))
}

async fn handle_tools_call(params: Option<Value>, state: AppState) -> Result<Value, (i64, String)> {
    let params = params.ok_or((-32602, "Missing params".to_string()))?;
    let tool_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "Missing tool name".to_string()))?;

    if tool_name != "generate_chart" {
        return Err((-32602, format!("Unknown tool: {tool_name}")));
    }

    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let chart_spec = args
        .get("chart")
        .ok_or((-32602, "Missing required argument: chart".to_string()))?
        .clone();

    let format_str = args.get("format").and_then(|v| v.as_str()).unwrap_or("png");
    let format: OutputFormat = match format_str {
        "svg" => OutputFormat::Svg,
        "webp" => OutputFormat::Webp,
        "data-uri" => OutputFormat::DataUri,
        _ => OutputFormat::Png,
    };

    let width = args.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
    let height = args
        .get("height")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let json_str = super::chart::apply_overrides_value(chart_spec, width, height, None).to_string();

    let permit = state
        .semaphore
        .try_acquire_owned()
        .map_err(|_| (-32000, "Server busy".to_string()))?;

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(state.render_timeout_ms),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let spec = render::parse_and_validate(&json_str, "chartjs", false)?;
            render::render(&spec, format, 1.0)
        }),
    )
    .await;

    match result {
        Err(_) => Err((-32000, "Render timeout".to_string())),
        Ok(Err(_)) => Err((-32000, "Render task panicked".to_string())),
        // エラー種別ごとに JSON-RPC エラーコードを使い分ける
        Ok(Ok(Err(e))) => {
            let code = match &e {
                // -32700 は JSON-RPC プロトコルレベルのパースエラー専用。
                // chart spec のパース失敗はツール引数の問題なので -32602。
                render::RenderError::Parse(_) => -32602, // Invalid params
                render::RenderError::Validate(_) => -32602, // Invalid params
                render::RenderError::Render(_) => -32603, // Internal error
            };
            Err((code, e.message().to_string()))
        }
        Ok(Ok(Ok(bytes))) => {
            use base64::{Engine, engine::general_purpose::STANDARD};
            let content = match format {
                OutputFormat::DataUri => {
                    // SVG を base64 エンコードして data URI に変換
                    format!("data:image/svg+xml;base64,{}", STANDARD.encode(&bytes))
                }
                OutputFormat::Svg => String::from_utf8_lossy(&bytes).into_owned(),
                OutputFormat::Png | OutputFormat::Webp => {
                    let mime = if format == OutputFormat::Webp {
                        "image/webp"
                    } else {
                        "image/png"
                    };
                    format!("data:{mime};base64,{}", STANDARD.encode(&bytes))
                }
            };
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": content
                    }
                ]
            }))
        }
    }
}
