mod convert;

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::routing;
use zoya_ir::{CheckedPackage, HttpMethod};
use zoya_package::QualifiedPath;
use zoya_run::Runner;

use convert::{axum_request_to_value, convert_pathname, value_to_axum_response};

/// Pre-computed metadata about a single route handler.
struct RouteInfo {
    /// Qualified path to the function in the package.
    path: QualifiedPath,
    /// Whether the handler takes a `Request` parameter.
    has_request_param: bool,
}

/// Shared state accessible from all route handlers.
struct AppState {
    checked: CheckedPackage,
    deps: Vec<CheckedPackage>,
    routes: Vec<RouteInfo>,
}

/// Build an Axum `Router` from a checked Zoya package.
///
/// Each function annotated with `#[get("/path")]`, `#[post("/path")]`, etc.
/// becomes an HTTP route. Handlers are executed in a fresh QuickJS runtime
/// per request.
pub fn router(checked: &CheckedPackage, deps: &[&CheckedPackage]) -> Router {
    let routes_meta = checked.routes();

    let mut route_infos = Vec::with_capacity(routes_meta.len());
    for (path, _, _) in &routes_meta {
        let func = checked
            .items
            .get(path)
            .expect("route function must exist in items");
        let has_request_param = !func.params.is_empty();
        route_infos.push(RouteInfo {
            path: path.clone(),
            has_request_param,
        });
    }

    let state = Arc::new(AppState {
        checked: checked.clone(),
        deps: deps.iter().map(|d| (*d).clone()).collect(),
        routes: route_infos,
    });

    let mut app = Router::new();

    for (i, (_, method, pathname)) in routes_meta.iter().enumerate() {
        let axum_path = convert_pathname(pathname.as_str());
        let handler = move |State(state): State<Arc<AppState>>, parts: Parts, body: Bytes| async move {
            handle_request(state, i, parts, body)
        };

        app = match method {
            HttpMethod::Get => app.route(&axum_path, routing::get(handler)),
            HttpMethod::Post => app.route(&axum_path, routing::post(handler)),
            HttpMethod::Put => app.route(&axum_path, routing::put(handler)),
            HttpMethod::Patch => app.route(&axum_path, routing::patch(handler)),
            HttpMethod::Delete => app.route(&axum_path, routing::delete(handler)),
        };
    }

    app.with_state(state)
}

/// Handle a single HTTP request by running the corresponding Zoya function.
fn handle_request(
    state: Arc<AppState>,
    route_index: usize,
    parts: Parts,
    body: Bytes,
) -> axum::response::Response {
    let info = &state.routes[route_index];
    let deps: Vec<&CheckedPackage> = state.deps.iter().collect();

    let args = if info.has_request_param {
        vec![axum_request_to_value(&parts, &body)]
    } else {
        vec![]
    };

    let result = Runner::new()
        .package(&state.checked, deps)
        .entry(info.path.clone(), args)
        .run();

    match result {
        Ok(value) => match value_to_axum_response(value) {
            Ok(response) => response,
            Err(e) => internal_error(format!("response conversion error: {e}")),
        },
        Err(e) => internal_error(e.to_string()),
    }
}

/// Build a 500 Internal Server Error response with a text body.
fn internal_error(message: String) -> axum::response::Response {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
}
