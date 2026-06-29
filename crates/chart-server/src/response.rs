use crate::render::{Compression, OutputFormat, RenderError};
use axum::{
    Json,
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use sha2::{Digest, Sha256};

pub fn etag_value(spec_json: &str, format: OutputFormat, compression: Compression) -> String {
    // format と compression を鍵に含める。圧縮プリセットが異なると PNG バイトも
    // 異なるため、これを区別しないと別プリセットのキャッシュで誤った 304 が返る。
    // (compression は PNG のみ有効だが、鍵に常に含めても衝突は起きない)
    let input = format!(
        "{spec_json}\x00{}\x00{}",
        format.as_str(),
        compression.as_str()
    );
    let hash = Sha256::digest(input.as_bytes());
    let short = hex::encode(&hash[..8]);
    format!("\"{short}-v{ver}\"", ver = env!("CARGO_PKG_VERSION"))
}

pub fn cache_headers(etag: &str) -> HeaderMap {
    static X_FULGUR_VERSION: HeaderName = HeaderName::from_static("x-fulgur-version");
    let mut h = HeaderMap::new();
    // immutable は「max-age 内は絶対再検証しない」意味になるため、
    // バージョン更新後に古いキャッシュが残り続ける問題がある。
    // immutable を除去し ETag + If-None-Match によるキャッシュ検証を有効にする。
    h.insert(
        header::CACHE_CONTROL,
        "public, max-age=86400".parse().unwrap(),
    );
    h.insert(header::ETAG, etag.parse().unwrap());
    h.insert(
        X_FULGUR_VERSION.clone(),
        env!("CARGO_PKG_VERSION").parse().unwrap(),
    );
    h.insert(header::VARY, "Accept-Encoding".parse().unwrap());
    h
}

pub fn render_response(bytes: Vec<u8>, format: OutputFormat, etag: &str) -> Response {
    let mut headers = cache_headers(etag);
    match format {
        OutputFormat::Svg => {
            headers.insert(
                header::CONTENT_TYPE,
                "image/svg+xml; charset=utf-8".parse().unwrap(),
            );
            (StatusCode::OK, headers, bytes).into_response()
        }
        OutputFormat::Png => {
            headers.insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
            (StatusCode::OK, headers, bytes).into_response()
        }
        OutputFormat::Webp => {
            headers.insert(header::CONTENT_TYPE, "image/webp".parse().unwrap());
            (StatusCode::OK, headers, bytes).into_response()
        }
        OutputFormat::DataUri => {
            let b64 = STANDARD.encode(&bytes);
            let uri = format!("data:image/svg+xml;base64,{b64}");
            headers.insert(
                header::CONTENT_TYPE,
                "text/plain; charset=utf-8".parse().unwrap(),
            );
            (StatusCode::OK, headers, uri).into_response()
        }
    }
}

pub fn error_response(status: StatusCode, err: &RenderError) -> Response {
    (
        status,
        Json(json!({
            "error": err.message(),
            "code": err.code(),
        })),
    )
        .into_response()
}
