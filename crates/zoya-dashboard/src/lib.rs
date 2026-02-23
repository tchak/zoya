mod components;
mod data;

use axum::Router;
use axum::response::Html;
use axum::routing::get;
use zoya_ir::CheckedPackage;

use components::render_page;
use data::DashboardData;

/// Build an Axum `Router` that serves an SSR HTML dashboard for a Zoya package.
///
/// The dashboard displays functions, tests, tasks, and HTTP routes from the
/// checked package. HTML is pre-rendered once at construction time.
pub fn dashboard(checked: &CheckedPackage, deps: &[&CheckedPackage]) -> Router {
    let _ = deps; // reserved for future use
    let data = DashboardData::from_package(checked);
    let html = render_page(&data);

    Router::new().route("/", get(move || async move { Html(html) }))
}
