use rquickjs::class::Class;
use rquickjs::function::Opt;
use rquickjs::promise::Promised;
use rquickjs::{Ctx, FromJs, Object, TypedArray, Value};

use crate::headers::Headers;

#[derive(rquickjs::class::Trace, rquickjs::JsLifetime)]
#[rquickjs::class]
pub struct Request<'js> {
    url: String,
    method: String,
    headers: Class<'js, Headers>,
    body: Option<Vec<u8>>,
}

impl Request<'_> {
    pub(crate) fn url_str(&self) -> &str {
        &self.url
    }

    pub(crate) fn method_str(&self) -> &str {
        &self.method
    }

    pub(crate) fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }
}

#[rquickjs::methods]
impl<'js> Request<'js> {
    #[qjs(constructor)]
    pub fn new(ctx: Ctx<'js>, url: String, init: Opt<Object<'js>>) -> rquickjs::Result<Self> {
        let mut method = "GET".to_string();
        let mut headers_val: Option<Value<'js>> = None;
        let mut body: Option<Vec<u8>> = None;

        if let Some(init) = init.0 {
            if let Some(m) = init.get::<_, Option<String>>("method")? {
                method = m;
            }
            headers_val = init.get::<_, Option<Value<'js>>>("headers")?;
            if let Some(b) = init.get::<_, Option<String>>("body")? {
                body = Some(b.into_bytes());
            }
        }

        let headers = if let Some(val) = headers_val {
            if let Ok(cls) = Class::<Headers>::from_value(&val) {
                cls
            } else {
                let obj: Object = FromJs::from_js(&ctx, val)?;
                let mut entries = Vec::new();
                for prop in obj.props::<String, String>() {
                    let (k, v) = prop?;
                    entries.push((k.to_lowercase(), v));
                }
                Class::instance(ctx.clone(), Headers::from_entries(entries))?
            }
        } else {
            Class::instance(ctx.clone(), Headers::from_entries(vec![]))?
        };

        Ok(Self {
            url,
            method,
            headers,
            body,
        })
    }

    #[qjs(get)]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    #[qjs(get)]
    pub fn method(&self) -> String {
        self.method.clone()
    }

    #[qjs(get)]
    pub fn headers(&self) -> Class<'js, Headers> {
        self.headers.clone()
    }

    #[qjs(rename = "clone")]
    pub fn clone_request(&self, ctx: Ctx<'js>) -> rquickjs::Result<Class<'js, Self>> {
        let new_headers = {
            let h = self.headers.borrow();
            Class::instance(ctx.clone(), h.clone_inner())?
        };
        Class::instance(
            ctx,
            Self {
                url: self.url.clone(),
                method: self.method.clone(),
                headers: new_headers,
                body: self.body.clone(),
            },
        )
    }

    pub fn text(&self) -> Promised<std::future::Ready<String>> {
        let s = self
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        Promised(std::future::ready(s))
    }

    pub fn json(
        &self,
        ctx: Ctx<'js>,
    ) -> rquickjs::Result<Promised<std::future::Ready<Value<'js>>>> {
        let body_str = self
            .body
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&body_str).map_err(|e| {
            rquickjs::Exception::throw_message(&ctx, &format!("JSON parse error: {e}"))
        })?;
        let js_val = rquickjs_serde::to_value(ctx.clone(), &parsed).map_err(|e| {
            rquickjs::Exception::throw_message(&ctx, &format!("JSON conversion error: {e}"))
        })?;
        Ok(Promised(std::future::ready(js_val)))
    }

    pub fn bytes(
        &self,
        ctx: Ctx<'js>,
    ) -> rquickjs::Result<Promised<std::future::Ready<Value<'js>>>> {
        let data = self.body.clone().unwrap_or_default();
        let typed_array = TypedArray::<u8>::new(ctx, data)?;
        Ok(Promised(std::future::ready(typed_array.as_value().clone())))
    }
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
        })
        .await;
        (runtime, context)
    }

    #[tokio::test]
    async fn test_construct_url_only() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let req = new Request("http://example.com");
                `${req.url},${req.method}`
            "#).unwrap();
            assert_eq!(result, "http://example.com,GET");
        })
        .await;
    }

    #[tokio::test]
    async fn test_construct_with_init() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let req = new Request("http://example.com", {
                    method: "POST",
                    headers: {"Content-Type": "application/json"},
                    body: '{"key":"value"}'
                });
                `${req.method},${req.headers.get("content-type")}`
            "#).unwrap();
            assert_eq!(result, "POST,application/json");
        })
        .await;
    }

    #[tokio::test]
    async fn test_headers_mutation_propagates() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let req = new Request("http://example.com");
                let h = req.headers;
                h.set("x-custom", "val");
                req.headers.get("x-custom")
            "#).unwrap();
            assert_eq!(result, "val");
        })
        .await;
    }

    #[tokio::test]
    async fn test_clone_is_independent() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let req = new Request("http://example.com", {
                    headers: {"x-key": "original"}
                });
                let cloned = req.clone();
                cloned.headers.set("x-key", "modified");
                `${req.headers.get("x-key")},${cloned.headers.get("x-key")}`
            "#).unwrap();
            assert_eq!(result, "original,modified");
        })
        .await;
    }

    #[tokio::test]
    async fn test_text() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                let req = new Request("http://example.com", { body: "hello world" });
                req.text()
            "#).unwrap();
            let val: String = promise.into_future().await.unwrap();
            assert_eq!(val, "hello world");
        })
        .await;
    }

    #[tokio::test]
    async fn test_json() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                let req = new Request("http://example.com", { body: '{"a":1}' });
                req.json()
            "#).unwrap();
            let val: rquickjs::Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let a: i32 = obj.get("a").unwrap();
            assert_eq!(a, 1);
        })
        .await;
    }

    #[tokio::test]
    async fn test_bytes() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                let req = new Request("http://example.com", { body: "AB" });
                req.bytes()
            "#).unwrap();
            let val: rquickjs::Value = promise.into_future().await.unwrap();
            let arr = TypedArray::<u8>::from_value(val).unwrap();
            let bytes: &[u8] = arr.as_ref();
            assert_eq!(bytes, &[65, 66]);
        })
        .await;
    }
}
