use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "fulgur chart-server",
        version = env!("CARGO_PKG_VERSION"),
        description = "Chart.js v4 spec → SVG/PNG HTTP renderer"
    ),
    paths(
        crate::handlers::chart::get_chart,
        crate::handlers::chart::post_chart,
        crate::handlers::validate::post_validate,
        crate::handlers::shortlink::post_create,
        crate::handlers::shortlink::get_shortlink,
        crate::handlers::meta::health,
    ),
    components(
        schemas(
            crate::handlers::chart::ChartQuery,
            crate::handlers::chart::ChartRequest,
            crate::handlers::validate::ValidateRequest,
            crate::handlers::shortlink::CreateRequest,
        )
    ),
    tags(
        (name = "chart", description = "Chart rendering endpoints"),
        (name = "meta", description = "Server metadata endpoints"),
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::ApiDoc;
    use serde_json::Value;
    use utoipa::OpenApi;

    #[test]
    fn openapi_documents_shortlink_render_endpoint() {
        let json = ApiDoc::openapi().to_json().unwrap();
        let document: Value = serde_json::from_str(&json).unwrap();
        let operation = &document["paths"]["/chart/s/{id}"]["get"];

        assert!(operation.is_object(), "GET /chart/s/{{id}} is missing");
        assert!(
            operation["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|parameter| {
                    parameter["name"] == "id"
                        && parameter["in"] == "path"
                        && parameter["required"] == true
                })
        );

        let responses = &operation["responses"];
        for status in ["200", "304", "400", "404", "415", "500", "503", "504"] {
            assert!(responses.get(status).is_some(), "missing response {status}");
        }

        let content = &responses["200"]["content"];
        for media_type in ["image/svg+xml", "image/png", "image/webp", "text/plain"] {
            assert!(
                content.get(media_type).is_some(),
                "missing 200 media type {media_type}"
            );
        }
    }
}
