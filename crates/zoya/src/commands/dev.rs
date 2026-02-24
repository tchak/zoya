use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use console::{Term, style};
use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tower::ServiceExt;
use zoya_check::check;
use zoya_ir::{CheckedPackage, HttpMethod};
use zoya_loader::{LoaderError, Mode};

/// Shared state for the dev server, holding the current active router.
struct DevState {
    router: RwLock<Option<Router>>,
    term: Term,
}

/// Result of a successful build.
struct BuildResult {
    router: Router,
    routes: Vec<(String, String)>,
}

/// Start a development HTTP server with file watching.
pub fn execute(path: &Path, port: u16) -> Result<()> {
    let term = Term::stderr();
    let watch_dir = resolve_watch_dir(path)?;

    // Initial build — only NoPackageToml is fatal
    let initial = initial_build(&term, path)?;
    let state = Arc::new(DevState {
        router: RwLock::new(initial.map(|b| b.router)),
        term: term.clone(),
    });

    let outer = build_outer_router(state.clone());

    let rt = tokio::runtime::Runtime::new().context("failed to start runtime")?;
    rt.block_on(async {
        let addr = format!("0.0.0.0:{port}");
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("failed to bind to {addr}"))?;

        let _ = term.write_line(&format!(
            "  Listening on {}",
            style(format!("http://localhost:{port}")).bold()
        ));
        let _ = term.write_line(&format!(
            "  Dashboard at {}",
            style(format!("http://localhost:{port}/_")).dim()
        ));
        let _ = term.write_line(&format!("  {}", style("Watching for changes...").dim()));

        // Set up file watcher
        let (tx, rx) = mpsc::unbounded_channel();
        let _watcher = setup_watcher(&watch_dir, tx)?;

        // Spawn the watch loop
        let watch_state = state.clone();
        let watch_path = path.to_path_buf();
        let watch_term = term.clone();
        tokio::spawn(async move {
            watch_loop(rx, watch_state, &watch_path, &watch_term).await;
        });

        axum::serve(listener, outer).await.context("server error")
    })
}

/// Resolve the directory to watch for file changes.
fn resolve_watch_dir(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        Ok(path.to_path_buf())
    } else if path.is_file() {
        path.parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow!("cannot determine parent directory of '{}'", path.display()))
    } else {
        // Path doesn't exist yet — use it as-is (it may be created)
        Ok(path.to_path_buf())
    }
}

/// Attempt the initial build. Returns `Err` only for `NoPackageToml` (fatal).
/// Returns `Ok(None)` on recoverable errors (prints the error and continues).
fn initial_build(term: &Term, path: &Path) -> Result<Option<BuildResult>> {
    match try_build(path) {
        Ok(result) => {
            print_routes(term, &result.routes);
            Ok(Some(result))
        }
        Err(BuildError::Fatal(e)) => Err(e),
        Err(BuildError::Recoverable(e)) => {
            let _ = term.write_line(&format!("  {}: {e}", style("error").red().bold()));
            Ok(None)
        }
    }
}

enum BuildError {
    Fatal(anyhow::Error),
    Recoverable(anyhow::Error),
}

/// Try to build the package. Classifies errors as fatal or recoverable.
fn try_build(path: &Path) -> Result<BuildResult, BuildError> {
    let pkg = zoya_loader::load_package(path, Mode::Test).map_err(|e| {
        if matches!(e, LoaderError::NoPackageToml { .. }) {
            BuildError::Fatal(e.into())
        } else {
            BuildError::Recoverable(e.into())
        }
    })?;

    let std = zoya_std::std();
    let checked = check(&pkg, &[std]).map_err(|e| BuildError::Recoverable(e.into()))?;

    let routes = extract_routes(&checked);
    let app_router = zoya_router::router(&checked, &[std]);
    let dashboard_router = zoya_dashboard::dashboard(&checked, &[std], "/_");
    let router = app_router.nest("/_", dashboard_router);

    Ok(BuildResult { router, routes })
}

/// Extract route metadata for display.
fn extract_routes(checked: &CheckedPackage) -> Vec<(String, String)> {
    checked
        .routes()
        .iter()
        .map(|(_, method, pathname)| {
            let method_str = match method {
                HttpMethod::Get => "GET",
                HttpMethod::Post => "POST",
                HttpMethod::Put => "PUT",
                HttpMethod::Patch => "PATCH",
                HttpMethod::Delete => "DELETE",
            };
            (method_str.to_string(), pathname.to_string())
        })
        .collect()
}

/// Print the route table to the terminal.
fn print_routes(term: &Term, routes: &[(String, String)]) {
    for (method, pathname) in routes {
        let _ = term.write_line(&format!(
            "  {}  {}",
            style(format!("{method:<6}")).bold(),
            pathname
        ));
    }
}

/// Build the outer proxy router that delegates to the current inner router.
fn build_outer_router(state: Arc<DevState>) -> Router {
    Router::new().fallback(proxy_handler).with_state(state)
}

/// Proxy handler that forwards requests to the current inner router.
async fn proxy_handler(
    State(state): State<Arc<DevState>>,
    request: axum::extract::Request,
) -> axum::response::Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let start = Instant::now();

    let inner = state.router.read().unwrap().clone();

    let response = match inner {
        Some(router) => match router.oneshot(request).await {
            Ok(response) => response,
            Err(infallible) => match infallible {},
        },
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Build failed. Fix errors and save to retry.\n",
        )
            .into_response(),
    };

    let status = response.status().as_u16();
    let elapsed = start.elapsed();
    let time_str = if elapsed.as_millis() > 0 {
        format!("{:.2}ms", elapsed.as_secs_f64() * 1000.0)
    } else {
        format!("{}µs", elapsed.as_micros())
    };

    let styled_status = if status < 400 {
        style(status).green()
    } else if status < 500 {
        style(status).yellow()
    } else {
        style(status).red()
    };

    let _ = state.term.write_line(&format!(
        "  {:<6} {path:<30} {styled_status}  {time_str}",
        method.as_str(),
    ));

    response
}

/// Set up a file watcher that sends events for `.zy` file changes.
fn setup_watcher(
    watch_dir: &Path,
    tx: mpsc::UnboundedSender<()>,
) -> Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        if let Ok(event) = res {
            let is_zy = event
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|ext| ext == "zy"));
            if is_zy {
                let _ = tx.send(());
            }
        }
    })
    .context("failed to create file watcher")?;

    watcher
        .watch(watch_dir, RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch '{}'", watch_dir.display()))?;

    Ok(watcher)
}

/// Watch loop that receives file change events, debounces, and rebuilds.
async fn watch_loop(
    mut rx: mpsc::UnboundedReceiver<()>,
    state: Arc<DevState>,
    path: &Path,
    term: &Term,
) {
    loop {
        // Wait for the first event
        if rx.recv().await.is_none() {
            return; // Channel closed
        }

        // Debounce: wait 100ms and drain any additional events
        tokio::time::sleep(Duration::from_millis(100)).await;
        while rx.try_recv().is_ok() {}

        let _ = term.write_line(&format!("  {}", style("File changed, rebuilding...").dim()));

        let had_previous = state.router.read().unwrap().is_some();

        match try_build(path) {
            Ok(result) => {
                print_routes(term, &result.routes);
                *state.router.write().unwrap() = Some(result.router);
                let _ = term.write_line(&format!("  {} Ready", style("✓").green()));
            }
            Err(BuildError::Fatal(msg) | BuildError::Recoverable(msg)) => {
                let _ = term.write_line(&format!("  {}: {msg}", style("error").red().bold()));
                if had_previous {
                    let _ = term.write_line(&format!(
                        "  {}",
                        style("Serving last successful build").dim()
                    ));
                }
            }
        }
    }
}
