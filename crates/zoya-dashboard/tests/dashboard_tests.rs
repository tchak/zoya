use axum::body::Body;
use http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;
use zoya_check::check;
use zoya_dashboard::dashboard;
use zoya_loader::{MemorySource, Mode, load_memory_package};
use zoya_std::std as zoya_std;

/// Helper: compile a Zoya source string and build a dashboard router from it.
fn build_dashboard(source: &str) -> axum::Router {
    build_dashboard_with_mode(source, Mode::Dev)
}

/// Helper: compile with a specific mode.
fn build_dashboard_with_mode(source: &str, mode: Mode) -> axum::Router {
    let std = zoya_std();
    let mem = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem, mode).unwrap();
    let checked = check(&package, &[std]).unwrap();
    dashboard(&checked, &[std], "")
}

/// Helper: send a GET request to a path and return (status, body string).
async fn get_request(app: axum::Router, uri: &str) -> (http::StatusCode, String) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    (status, body_str)
}

#[tokio::test]
async fn test_dashboard_returns_200() {
    let app = build_dashboard(
        r#"
pub fn add(x: Int, y: Int) -> Int x + y
"#,
    );
    let (status, body) = get_request(app, "/").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(body.contains("<!doctype html>"));
    assert!(body.contains("<div id=\"root\">"));
}

#[tokio::test]
async fn test_dashboard_api_returns_json() {
    let app = build_dashboard(
        r#"
pub fn add(x: Int, y: Int) -> Int x + y
pub fn greet(name: String) -> String name
"#,
    );
    let (status, body) = get_request(app, "/api/data").await;
    assert_eq!(status, http::StatusCode::OK);
    let data: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(data["functions"].is_array());
    let functions = data["functions"].as_array().unwrap();
    assert_eq!(functions.len(), 2);
    assert!(functions.iter().any(|f| f["name"] == "add"));
    assert!(functions.iter().any(|f| f["name"] == "greet"));
}

#[tokio::test]
async fn test_dashboard_api_shows_tests() {
    // Use Mode::Test so #[test] functions are included in the package
    let app = build_dashboard_with_mode(
        r#"
fn add(x: Int, y: Int) -> Int x + y

#[test]
fn test_add() -> () assert_eq(add(1, 2), 3)
"#,
        Mode::Test,
    );
    let (status, body) = get_request(app, "/api/data").await;
    assert_eq!(status, http::StatusCode::OK);
    let data: serde_json::Value = serde_json::from_str(&body).unwrap();
    let tests = data["tests"].as_array().unwrap();
    assert!(tests.iter().any(|t| t["name"] == "test_add"));
}

#[tokio::test]
async fn test_dashboard_api_shows_tasks() {
    let app = build_dashboard(
        r#"
#[task]
pub fn deploy() -> () ()
"#,
    );
    let (status, body) = get_request(app, "/api/data").await;
    assert_eq!(status, http::StatusCode::OK);
    let data: serde_json::Value = serde_json::from_str(&body).unwrap();
    let tasks = data["tasks"].as_array().unwrap();
    assert!(tasks.iter().any(|t| t["name"] == "deploy"));
}

#[tokio::test]
async fn test_dashboard_api_shows_routes() {
    let app = build_dashboard(
        r#"
use std::http::Response
use std::option::Option

#[get("/hello")]
pub fn hello() -> Response {
  Response::ok(Option::None)
}
"#,
    );
    let (status, body) = get_request(app, "/api/data").await;
    assert_eq!(status, http::StatusCode::OK);
    let data: serde_json::Value = serde_json::from_str(&body).unwrap();
    let routes = data["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0]["method"], "GET");
    assert_eq!(routes[0]["pathname"], "/hello");
}

#[tokio::test]
async fn test_dashboard_api_empty_package() {
    let app = build_dashboard("");
    let (status, body) = get_request(app, "/api/data").await;
    assert_eq!(status, http::StatusCode::OK);
    let data: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(data["functions"].as_array().unwrap().len(), 0);
    assert_eq!(data["tests"].as_array().unwrap().len(), 0);
    assert_eq!(data["tasks"].as_array().unwrap().len(), 0);
    assert_eq!(data["routes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_dashboard_serves_js() {
    let app = build_dashboard("");
    let (status, _body) = get_request(app, "/assets/main.js").await;
    assert_eq!(status, http::StatusCode::OK);
}

#[tokio::test]
async fn test_dashboard_serves_css() {
    let app = build_dashboard("");
    let (status, _body) = get_request(app, "/assets/main.css").await;
    assert_eq!(status, http::StatusCode::OK);
}
