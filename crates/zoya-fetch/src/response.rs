use rquickjs::class::Class;
use rquickjs::function::Opt;
use rquickjs::promise::Promised;
use rquickjs::{Ctx, FromJs, Object, TypedArray, Value};

use crate::headers::Headers;

#[derive(rquickjs::class::Trace, rquickjs::JsLifetime)]
#[rquickjs::class]
pub struct Response<'js> {
    status: u16,
    headers: Class<'js, Headers>,
    body: Option<Vec<u8>>,
}

impl<'js> Response<'js> {
    pub(crate) fn from_parts(
        ctx: Ctx<'js>,
        status: u16,
        headers_entries: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> rquickjs::Result<Self> {
        let headers = Class::instance(ctx, Headers::from_entries(headers_entries))?;
        Ok(Self {
            status,
            headers,
            body: Some(body),
        })
    }
}

#[rquickjs::methods]
impl<'js> Response<'js> {
    #[qjs(constructor)]
    pub fn new(
        ctx: Ctx<'js>,
        body: Opt<Value<'js>>,
        init: Opt<Object<'js>>,
    ) -> rquickjs::Result<Self> {
        let body_bytes = match body.0 {
            Some(val) if !val.is_null() && !val.is_undefined() => {
                let s = String::from_js(&ctx, val)?;
                Some(s.into_bytes())
            }
            _ => None,
        };

        let mut status: u16 = 200;
        let mut headers_val: Option<Value<'js>> = None;

        if let Some(init) = init.0 {
            if let Some(s) = init.get::<_, Option<u16>>("status")? {
                status = s;
            }
            headers_val = init.get::<_, Option<Value<'js>>>("headers")?;
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
            status,
            headers,
            body: body_bytes,
        })
    }

    #[qjs(get)]
    pub fn status(&self) -> u16 {
        self.status
    }

    #[qjs(get)]
    pub fn ok(&self) -> bool {
        (200..=299).contains(&self.status)
    }

    #[qjs(get)]
    pub fn headers(&self) -> Class<'js, Headers> {
        self.headers.clone()
    }

    #[qjs(rename = "clone")]
    pub fn clone_response(&self, ctx: Ctx<'js>) -> rquickjs::Result<Class<'js, Self>> {
        let new_headers = {
            let h = self.headers.borrow();
            Class::instance(ctx.clone(), h.clone_inner())?
        };
        Class::instance(
            ctx,
            Self {
                status: self.status,
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
            Class::<Response>::define(&ctx.globals()).unwrap();
        })
        .await;
        (runtime, context)
    }

    #[tokio::test]
    async fn test_construct_defaults() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let res = new Response();
                `${res.status},${res.ok}`
            "#).unwrap();
            assert_eq!(result, "200,true");
        })
        .await;
    }

    #[tokio::test]
    async fn test_construct_with_body_and_init() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let res = new Response("hello", {
                    status: 201,
                    headers: {"Content-Type": "text/plain"}
                });
                `${res.status},${res.ok},${res.headers.get("content-type")}`
            "#).unwrap();
            assert_eq!(result, "201,true,text/plain");
        })
        .await;
    }

    #[tokio::test]
    async fn test_not_ok_status() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: bool = ctx.eval(r#"
                let res = new Response(null, { status: 404 });
                res.ok
            "#).unwrap();
            assert!(!result);
        })
        .await;
    }

    #[tokio::test]
    async fn test_headers_mutation_propagates() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let result: String = ctx.eval(r#"
                let res = new Response();
                let h = res.headers;
                h.set("x-custom", "val");
                res.headers.get("x-custom")
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
                let res = new Response("body", {
                    status: 200,
                    headers: {"x-key": "original"}
                });
                let cloned = res.clone();
                cloned.headers.set("x-key", "modified");
                `${res.headers.get("x-key")},${cloned.headers.get("x-key")}`
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
                let res = new Response("hello world");
                res.text()
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
                let res = new Response('{"b":2}');
                res.json()
            "#).unwrap();
            let val: rquickjs::Value = promise.into_future().await.unwrap();
            let obj = val.as_object().unwrap();
            let b: i32 = obj.get("b").unwrap();
            assert_eq!(b, 2);
        })
        .await;
    }

    #[tokio::test]
    async fn test_bytes() {
        let (_runtime, context) = setup().await;
        rquickjs::async_with!(context => |ctx| {
            let promise: Promise = ctx.eval(r#"
                let res = new Response("CD");
                res.bytes()
            "#).unwrap();
            let val: rquickjs::Value = promise.into_future().await.unwrap();
            let arr = TypedArray::<u8>::from_value(val).unwrap();
            let bytes: &[u8] = arr.as_ref();
            assert_eq!(bytes, &[67, 68]);
        })
        .await;
    }
}
