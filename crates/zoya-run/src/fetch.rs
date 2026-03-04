use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rquickjs::class::Class;
use rquickjs::function::Opt;
use rquickjs::{Ctx, FromJs, Object, Value};

use crate::headers::Headers;
use crate::request::Request;
use crate::response::Response;

/// Parsed fetch inputs extracted from JS arguments.
#[derive(Clone)]
pub struct FetchInput {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Structured output from a fetch operation.
pub struct FetchOutput {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Default for FetchOutput {
    fn default() -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
}

/// Result type returned by a `FetchHandler`.
pub type FetchResult = Result<FetchOutput, String>;

/// Handler closure for intercepting `zoya://` URLs in `fetch()`.
pub type FetchHandler =
    Arc<dyn Fn(FetchInput) -> Pin<Box<dyn Future<Output = FetchResult> + Send>> + Send + Sync>;

fn parse_fetch_args<'js>(
    ctx: &Ctx<'js>,
    input: Value<'js>,
    init: Opt<Object<'js>>,
) -> rquickjs::Result<FetchInput> {
    // First arg can be a string URL or a Request object
    if let Ok(req_cls) = Class::<Request>::from_value(&input) {
        let req = req_cls.borrow();
        let url = req.url_str().to_string();
        let method = req.method_str().to_string();
        let body = req.body_bytes().map(|b| b.to_vec());
        let headers = req.headers().borrow().entries().to_vec();
        drop(req);

        // init overrides Request fields
        if let Some(init_obj) = init.0 {
            return apply_init(url, method, headers, body, init_obj, ctx);
        }

        return Ok(FetchInput {
            url,
            method,
            headers,
            body,
        });
    }

    // String URL
    let url = String::from_js(ctx, input)?;
    let method = "GET".to_string();
    let headers = Vec::new();
    let body = None;

    if let Some(init_obj) = init.0 {
        return apply_init(url, method, headers, body, init_obj, ctx);
    }

    Ok(FetchInput {
        url,
        method,
        headers,
        body,
    })
}

fn apply_init<'js>(
    mut url: String,
    mut method: String,
    mut headers: Vec<(String, String)>,
    mut body: Option<Vec<u8>>,
    init: Object<'js>,
    ctx: &Ctx<'js>,
) -> rquickjs::Result<FetchInput> {
    if let Some(m) = init.get::<_, Option<String>>("method")? {
        method = m;
    }
    if let Some(u) = init.get::<_, Option<String>>("url")? {
        url = u;
    }
    if let Some(h_val) = init.get::<_, Option<Value<'js>>>("headers")? {
        if let Ok(cls) = Class::<Headers>::from_value(&h_val) {
            headers = cls.borrow().entries().to_vec();
        } else {
            let obj: Object = FromJs::from_js(ctx, h_val)?;
            headers = Vec::new();
            for prop in obj.props::<String, String>() {
                let (k, v) = prop?;
                headers.push((k.to_lowercase(), v));
            }
        }
    }
    if let Some(b) = init.get::<_, Option<String>>("body")? {
        body = Some(b.into_bytes());
    }

    Ok(FetchInput {
        url,
        method,
        headers,
        body,
    })
}

/// Perform the HTTP request using reqwest. Pure async Rust, no JS objects.
async fn execute_fetch(input: FetchInput) -> FetchResult {
    let client = reqwest::Client::new();

    let reqwest_method = input
        .method
        .parse::<reqwest::Method>()
        .map_err(|e| format!("invalid HTTP method: {e}"))?;

    let mut builder = client.request(reqwest_method, &input.url);

    for (key, value) in &input.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if let Some(body) = input.body {
        builder = builder.body(body);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;

    let status = response.status().as_u16();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = response
        .bytes()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?
        .to_vec();

    Ok(FetchOutput {
        status,
        headers,
        body,
    })
}

/// The `fetch()` global function exposed to QuickJS.
///
/// When `handler` is `Some`, requests to `zoya://` URLs are routed to the handler.
/// When `handler` is `None`, `zoya://` URLs produce an "unsupported URL scheme" error.
/// All other URLs are fetched via `reqwest`.
pub fn fetch<'js>(
    ctx: Ctx<'js>,
    input: Value<'js>,
    init: Opt<Object<'js>>,
    handler: Option<FetchHandler>,
) -> rquickjs::Result<rquickjs::Promise<'js>> {
    let fetch_input = parse_fetch_args(&ctx, input, init)?;

    let (promise, resolve, reject) = ctx.promise()?;

    ctx.spawn(async move {
        let result = if fetch_input.url.starts_with("zoya://") {
            match handler {
                Some(h) => h(fetch_input).await,
                None => Err("unsupported URL scheme: zoya://".to_string()),
            }
        } else {
            execute_fetch(fetch_input).await
        };

        match result {
            Ok(output) => {
                let response = Response::from_parts(
                    resolve.ctx().clone(),
                    output.status,
                    output.headers,
                    output.body,
                );
                match response {
                    Ok(resp) => match Class::instance(resolve.ctx().clone(), resp) {
                        Ok(cls) => {
                            let _ = resolve.call::<_, ()>((cls,));
                        }
                        Err(e) => {
                            let _ = reject.call::<_, ()>((e.to_string(),));
                        }
                    },
                    Err(e) => {
                        let _ = reject.call::<_, ()>((e.to_string(),));
                    }
                }
            }
            Err(e) => {
                let _ = reject.call::<_, ()>((e,));
            }
        }
    });

    Ok(promise)
}

#[cfg(test)]
mod tests {
    use rquickjs::{AsyncContext, AsyncRuntime, Class, Promise};

    use super::*;

    async fn setup() -> (AsyncRuntime, AsyncContext) {
        setup_with_handler(None).await
    }

    async fn setup_with_handler(handler: Option<FetchHandler>) -> (AsyncRuntime, AsyncContext) {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();
            Class::<Request>::define(&ctx.globals()).unwrap();
            Class::<Response>::define(&ctx.globals()).unwrap();
            let h = handler;
            ctx.globals().set("fetch", rquickjs::Function::new(ctx.clone(), move |ctx, input, init| {
                fetch(ctx, input, init, h.clone())
            }).unwrap()).unwrap();
        })
        .await;
        (runtime, context)
    }

    #[tokio::test]
    async fn test_fetch_returns_promise() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let val: Value = ctx.eval(r#"
                fetch("https://httpbin.org/get")
            "#).unwrap();
            assert!(val.as_promise().is_some());
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_get() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://httpbin.org/get")
            "#).unwrap();
            let val: Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let status: u16 = obj.get("status").unwrap();
            assert_eq!(status, 200);
            let ok: bool = obj.get("ok").unwrap();
            assert!(ok);
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_post_with_body() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://httpbin.org/post", {
                    method: "POST",
                    headers: {"Content-Type": "application/json"},
                    body: '{"hello":"world"}'
                })
            "#).unwrap();
            let val: Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let status: u16 = obj.get("status").unwrap();
            assert_eq!(status, 200);
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_with_request_object() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                let req = new Request("https://httpbin.org/get");
                fetch(req)
            "#).unwrap();
            let val: Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let status: u16 = obj.get("status").unwrap();
            assert_eq!(status, 200);
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_404_resolves_not_rejects() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://httpbin.org/status/404")
            "#).unwrap();
            let val: Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let status: u16 = obj.get("status").unwrap();
            assert_eq!(status, 404);
            let ok: bool = obj.get("ok").unwrap();
            assert!(!ok);
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_response_text() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://httpbin.org/get").then(r => r.text())
            "#).unwrap();
            let val: String = promise.into_future().await.unwrap();
            assert!(val.contains("httpbin.org"));
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_response_json() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://httpbin.org/get").then(r => r.json())
            "#).unwrap();
            let val: Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let url: String = obj.get("url").unwrap();
            assert_eq!(url, "https://httpbin.org/get");
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_network_error_rejects() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://this-domain-definitely-does-not-exist-zoya.invalid/path")
            "#).unwrap();
            let result: Result<Value, rquickjs::Error> = promise.into_future().await;
            assert!(result.is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_zoya_url_with_handler() {
        let handler: FetchHandler = Arc::new(|input| {
            Box::pin(async move {
                assert!(input.url.starts_with("zoya://"));
                let body = b"hello from handler".to_vec();
                Ok(FetchOutput {
                    status: 200,
                    headers: vec![("content-type".to_string(), "text/plain".to_string())],
                    body,
                })
            })
        });
        let (_runtime, context) = setup_with_handler(Some(handler)).await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("zoya://my-route").then(r => r.text())
            "#).unwrap();
            let val: String = promise.into_future().await.unwrap();
            assert_eq!(val, "hello from handler");
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_zoya_url_without_handler_rejects() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("zoya://my-route")
            "#).unwrap();
            let result: Result<Value, rquickjs::Error> = promise.into_future().await;
            assert!(result.is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn test_fetch_https_url_ignores_handler() {
        let handler: FetchHandler = Arc::new(|_input| {
            Box::pin(async move {
                panic!("handler should not be called for https URLs");
            })
        });
        let (_runtime, context) = setup_with_handler(Some(handler)).await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                fetch("https://httpbin.org/get")
            "#).unwrap();
            let val: Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let status: u16 = obj.get("status").unwrap();
            assert_eq!(status, 200);
        })
        .await;
    }
}
