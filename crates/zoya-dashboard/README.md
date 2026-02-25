# zoya-dashboard

Development dashboard for the Zoya programming language.

Serves an embedded React SPA that displays package metadata: functions, tests, jobs, and HTTP routes. Used by the `zoya dev` command to provide a browser-based overview of the current package.

## Features

- **Embedded SPA** - Pre-built React assets included via `include_str!()` at compile time
- **JSON API** - `/api/data` endpoint serves structured package metadata
- **Base path injection** - Supports nested mounting (e.g., under `/_`) via `<base href>` tag
- **Zero runtime dependencies** - No file I/O at runtime; everything is embedded

## Usage

```rust
use zoya_build::build_from_path;
use zoya_dashboard::dashboard;
use zoya_loader::Mode;
use std::path::Path;

// Build a package
let output = build_from_path(Path::new("my_project"), Mode::Dev)?;

// Create the dashboard router, mounted at "/_"
let dashboard_router = dashboard(&output, "/_");

// Nest under your main application router
let app = axum::Router::new()
    .nest("/_", dashboard_router);

// Serve
let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
axum::serve(listener, app).await?;
```

## Routes

| Route | Response |
|-------|----------|
| `GET /` | Dashboard HTML (with injected `<base>` tag) |
| `GET /api/data` | JSON payload with package metadata |
| `GET /assets/main.js` | Bundled JavaScript |
| `GET /assets/main.css` | Bundled CSS |

## API Response

The `GET /api/data` endpoint returns a `DashboardData` JSON object:

```json
{
  "package_name": "my_project",
  "functions": [
    { "name": "main", "module": "", "signature": "() -> Int" }
  ],
  "tests": [
    { "name": "test_add", "module": "math" }
  ],
  "jobs": [
    { "name": "deploy", "module": "", "signature": "(String) -> ()" }
  ],
  "routes": [
    { "method": "GET", "pathname": "/", "handler": "index", "module": "", "signature": "() -> Response" }
  ]
}
```

## Public API

```rust
/// Build an Axum `Router` that serves a dashboard SPA for a Zoya package.
pub fn dashboard(output: &BuildOutput, base_path: &str) -> Router;
```

## Dependencies

- [zoya-build](../zoya-build) - Build pipeline (for `BuildOutput`)
- [zoya-package](../zoya-package) - Package data structures
- [axum](https://github.com/tokio-rs/axum) - HTTP framework
- [serde](https://github.com/serde-rs/serde) / [serde_json](https://github.com/serde-rs/json) - JSON serialization
