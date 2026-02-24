mod data;

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use zoya_build::BuildOutput;

use data::DashboardData;

const INDEX_HTML: &str = include_str!("../../../packages/zoya-dashboard/dist/index.html");
const MAIN_JS: &str = include_str!("../../../packages/zoya-dashboard/dist/assets/main.js");
const MAIN_CSS: &str = include_str!("../../../packages/zoya-dashboard/dist/assets/main.css");

struct AppState {
    html: String,
    data: DashboardData,
}

/// Build an Axum `Router` that serves a dashboard SPA for a Zoya package.
///
/// The dashboard displays functions, tests, tasks, and HTTP routes from the
/// build output. The SPA fetches data from a JSON API endpoint.
pub fn dashboard(output: &BuildOutput, base_path: &str) -> Router {
    let data = DashboardData::from_output(output);

    // Inject <base> tag into HTML so relative URLs resolve correctly when nested
    let base_tag = format!("<base href=\"{base_path}/\">");
    let html = INDEX_HTML.replacen("<head>", &format!("<head>{base_tag}"), 1);

    let state = Arc::new(AppState { html, data });

    Router::new()
        .route("/", get(index_handler))
        .route("/api/data", get(api_data_handler))
        .route("/assets/main.js", get(js_handler))
        .route("/assets/main.css", get(css_handler))
        .with_state(state)
}

async fn index_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(state.html.clone())
}

async fn api_data_handler(State(state): State<Arc<AppState>>) -> Json<DashboardData> {
    Json(state.data.clone())
}

async fn js_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], MAIN_JS)
}

async fn css_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], MAIN_CSS)
}
