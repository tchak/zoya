use axum::body::Body;
use http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;
use zoya_loader::{MemorySource, load_memory_package};
use zoya_router::router;

/// Helper: compile a Zoya source string and build a router from it.
fn build_router(source: &str) -> axum::Router {
    let mem = MemorySource::new().with_module("root", source);
    let package = load_memory_package(&mem, zoya_loader::Mode::Dev).unwrap();
    let output = zoya_build::build(&package).unwrap();
    router(&output)
}

/// Helper: send a request to a router and return (status, body string).
async fn send(app: axum::Router, req: Request<Body>) -> (http::StatusCode, String) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    (status, body_str)
}

#[tokio::test]
async fn test_get_handler_returns_200() {
    let app = build_router(
        r#"
use std::http::Response
use std::option::Option

#[get("/hello")]
pub fn hello() -> Response {
  Response::ok(Option::None)
}
"#,
    );

    let req = Request::builder()
        .uri("/hello")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(app, req).await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body, "");
}

#[tokio::test]
async fn test_get_handler_returns_text_body() {
    let app = build_router(
        r#"
use std::http::Response
use std::http::Body
use std::option::Option

#[get("/hello")]
pub fn hello() -> Response {
  Response::ok(Option::Some(Body::Text("Hello, World!")))
}
"#,
    );

    let req = Request::builder()
        .uri("/hello")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(app, req).await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn test_post_handler_with_request() {
    let app = build_router(
        r#"
use std::http::Request
use std::http::Response
use std::http::Body
use std::option::Option

#[post("/echo")]
pub fn echo(req: Request) -> Response {
  match req.body {
    Option::Some(Body::Text(text)) => Response::ok(Option::Some(Body::Text(text))),
    _ => Response::ok(Option::None),
  }
}
"#,
    );

    let req = Request::builder()
        .method("POST")
        .uri("/echo")
        .body(Body::from("hello"))
        .unwrap();
    let (status, body) = send(app, req).await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body, "hello");
}

#[tokio::test]
async fn test_handler_panic_returns_500() {
    let app = build_router(
        r#"
use std::http::Response
use std::option::Option

#[get("/boom")]
pub fn boom() -> Response {
  panic("kaboom")
}
"#,
    );

    let req = Request::builder().uri("/boom").body(Body::empty()).unwrap();
    let (status, _body) = send(app, req).await;
    assert_eq!(status, http::StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_multiple_routes() {
    let app = build_router(
        r#"
use std::http::Response
use std::http::Body
use std::option::Option

#[get("/a")]
pub fn route_a() -> Response {
  Response::ok(Option::Some(Body::Text("A")))
}

#[post("/b")]
pub fn route_b() -> Response {
  Response::ok(Option::Some(Body::Text("B")))
}
"#,
    );

    let req_a = Request::builder().uri("/a").body(Body::empty()).unwrap();
    let (status_a, body_a) = send(app.clone(), req_a).await;
    assert_eq!(status_a, http::StatusCode::OK);
    assert_eq!(body_a, "A");

    let req_b = Request::builder()
        .method("POST")
        .uri("/b")
        .body(Body::empty())
        .unwrap();
    let (status_b, body_b) = send(app, req_b).await;
    assert_eq!(status_b, http::StatusCode::OK);
    assert_eq!(body_b, "B");
}

#[tokio::test]
async fn test_404_for_unknown_route() {
    let app = build_router(
        r#"
use std::http::Response
use std::option::Option

#[get("/hello")]
pub fn hello() -> Response {
  Response::ok(Option::None)
}
"#,
    );

    let req = Request::builder()
        .uri("/unknown")
        .body(Body::empty())
        .unwrap();
    let (status, _body) = send(app, req).await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_response_custom_status() {
    let app = build_router(
        r#"
use std::http::Response
use std::http::Body
use std::option::Option
use std::dict::Dict

#[get("/created")]
pub fn created() -> Response {
  Response { body: Option::Some(Body::Text("created")), status: 201, headers: Dict::new() }
}
"#,
    );

    let req = Request::builder()
        .uri("/created")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(app, req).await;
    assert_eq!(status, http::StatusCode::CREATED);
    assert_eq!(body, "created");
}

#[tokio::test]
async fn test_response_with_headers() {
    let app = build_router(
        r#"
use std::http::Response
use std::http::Body
use std::option::Option
use std::dict::Dict

#[get("/with-headers")]
pub fn with_headers() -> Response {
  Response { body: Option::None, status: 200, headers: Dict::new().insert("x-custom", "hello") }
}
"#,
    );

    let req = Request::builder()
        .uri("/with-headers")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-custom")
            .unwrap()
            .to_str()
            .unwrap(),
        "hello"
    );
}
