# zoya-router

HTTP router for the Zoya programming language.

Builds an [Axum](https://github.com/tokio-rs/axum) `Router` from a `BuildOutput`. Functions annotated with HTTP method attributes (`#[get("/path")]`, `#[post("/path")]`, etc.) become HTTP route handlers executed in a fresh QuickJS runtime per request.

## Features

- **Attribute-based routing** - Map Zoya functions to HTTP routes via `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]`
- **Automatic request parsing** - Handlers with a `Request` parameter receive the parsed HTTP request
- **Response conversion** - Zoya `Response` values are converted to HTTP responses
- **Isolation** - Each request runs in a fresh QuickJS runtime for safety

## Usage

```rust
use zoya_build::build_from_path;
use zoya_loader::Mode;
use zoya_router::router;
use std::path::Path;

// Build the package
let output = build_from_path(Path::new("my_project"), Mode::Dev)?;

// Build an Axum router from HTTP-annotated functions
let app = router(&output);

// Serve with Axum
let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
axum::serve(listener, app).await?;
```

## Zoya HTTP Handlers

Define HTTP routes in Zoya source:

```zoya
use std::http::{Request, Response, Body}

#[get("/")]
pub fn index() -> Response {
    Response::ok(Option::Some(Body::Text("Hello, world!")))
}

#[post("/echo")]
pub fn echo(request: Request) -> Response {
    Response::ok(request.body)
}

#[get("/health")]
pub fn health() -> Response {
    Response::ok(Option::None)
}
```

Supported HTTP methods: `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]`.

Pathnames must start with `/` and may contain path parameters (e.g., `/users/:id`).

## Public API

```rust
/// Build an Axum `Router` from a Zoya build output.
///
/// Each function annotated with `#[get("/path")]`, `#[post("/path")]`, etc.
/// becomes an HTTP route. Handlers are executed in a fresh QuickJS runtime
/// per request.
pub fn router(output: &BuildOutput) -> Router;
```

## Dependencies

- [zoya-build](../zoya-build) - Build pipeline (for `BuildOutput`)
- [zoya-package](../zoya-package) - Package data structures
- [zoya-run](../zoya-run) - Runtime execution (QuickJS)
- [zoya-value](../zoya-value) - Runtime value types
- [axum](https://github.com/tokio-rs/axum) - HTTP framework
