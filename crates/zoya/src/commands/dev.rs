use std::path::Path;

use zoya_check::check;
use zoya_ir::HttpMethod;
use zoya_loader::Mode;

/// Start a development HTTP server from a Zoya package.
pub fn execute(path: &Path, port: u16) -> Result<(), String> {
    let pkg = zoya_loader::load_package(path, Mode::Dev).map_err(|e| e.to_string())?;
    let std = zoya_std::std();
    let checked = check(&pkg, &[std]).map_err(|e| e.to_string())?;

    let routes = checked.routes();
    for (_, method, pathname) in &routes {
        let method_str = match method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
        };
        eprintln!("  {method_str:<6} {pathname}");
    }

    let app = zoya_router::router(&checked, &[std]);

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("failed to start runtime: {e}"))?;
    rt.block_on(async {
        let addr = format!("0.0.0.0:{port}");
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("failed to bind to {addr}: {e}"))?;
        eprintln!("Listening on http://localhost:{port}");
        axum::serve(listener, app)
            .await
            .map_err(|e| format!("server error: {e}"))
    })
}
