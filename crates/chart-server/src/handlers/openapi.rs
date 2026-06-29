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
