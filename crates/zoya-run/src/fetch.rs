use rquickjs::class::Class;
use rquickjs::function::Opt;
use rquickjs::{Ctx, FromJs, Object, Value};

use crate::headers::Headers;
use crate::request::Request;
use crate::response::Response;

/// Extract fetch inputs from JS arguments into pure Rust data.
struct FetchInput {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

fn parse_fetch_args<'js>(ctx: &Ctx<'js>, input: Value<'js>, init: Opt<Object<'js>>) -> rquickjs::Result<FetchInput> {
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
async fn execute_fetch(
    input: FetchInput,
) -> Result<(u16, Vec<(String, String)>, Vec<u8>), String> {
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

    let response = builder.send().await.map_err(|e| format!("fetch failed: {e}"))?;

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

    Ok((status, headers, body))
}

/// The `fetch()` global function exposed to QuickJS.
pub fn fetch<'js>(ctx: Ctx<'js>, input: Value<'js>, init: Opt<Object<'js>>) -> rquickjs::Result<rquickjs::Promise<'js>> {
    let fetch_input = parse_fetch_args(&ctx, input, init)?;

    let (promise, resolve, reject) = ctx.promise()?;

    ctx.spawn(async move {
        match execute_fetch(fetch_input).await {
            Ok((status, headers, body)) => {
                let response = Response::from_parts(
                    resolve.ctx().clone(),
                    status,
                    headers,
                    body,
                );
                match response {
                    Ok(resp) => {
                        match Class::instance(resolve.ctx().clone(), resp) {
                            Ok(cls) => {
                                let _ = resolve.call::<_, ()>((cls,));
                            }
                            Err(e) => {
                                let _ = reject.call::<_, ()>((e.to_string(),));
                            }
                        }
                    }
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
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();
            Class::<Request>::define(&ctx.globals()).unwrap();
            Class::<Response>::define(&ctx.globals()).unwrap();
            ctx.globals().set("fetch", rquickjs::Function::new(ctx.clone(), fetch).unwrap()).unwrap();
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
}
