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
    dashboard(&checked, &[std])
}

/// Helper: send a GET / request and return (status, body string).
async fn get_dashboard(app: axum::Router) -> (http::StatusCode, String) {
    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
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
    let (status, body) = get_dashboard(app).await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(body.contains("<!DOCTYPE html>"));
}

#[tokio::test]
async fn test_dashboard_shows_functions() {
    let app = build_dashboard(
        r#"
pub fn add(x: Int, y: Int) -> Int x + y
pub fn greet(name: String) -> String name
"#,
    );
    let (_, body) = get_dashboard(app).await;
    assert!(body.contains("add"));
    assert!(body.contains("greet"));
    assert!(body.contains("Functions"));
}

#[tokio::test]
async fn test_dashboard_shows_tests() {
    // Use Mode::Test so #[test] functions are included in the package
    let app = build_dashboard_with_mode(
        r#"
fn add(x: Int, y: Int) -> Int x + y

#[test]
fn test_add() -> () assert_eq(add(1, 2), 3)
"#,
        Mode::Test,
    );
    let (_, body) = get_dashboard(app).await;
    assert!(body.contains("test_add"));
    assert!(body.contains("Tests"));
}

#[tokio::test]
async fn test_dashboard_shows_tasks() {
    let app = build_dashboard(
        r#"
#[task]
pub fn deploy() -> () ()
"#,
    );
    let (_, body) = get_dashboard(app).await;
    assert!(body.contains("deploy"));
    assert!(body.contains("Tasks"));
}

#[tokio::test]
async fn test_dashboard_shows_routes() {
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
    let (_, body) = get_dashboard(app).await;
    assert!(body.contains("GET"));
    assert!(body.contains("/hello"));
    assert!(body.contains("Routes"));
}

#[tokio::test]
async fn test_dashboard_empty_package() {
    let app = build_dashboard("");
    let (status, body) = get_dashboard(app).await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(body.contains("No functions"));
    assert!(body.contains("No tests"));
    assert!(body.contains("No tasks"));
    assert!(body.contains("No routes"));
}
