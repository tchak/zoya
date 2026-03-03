use rquickjs::Object;
use rquickjs::function::Opt;

#[derive(Clone, rquickjs::class::Trace, rquickjs::JsLifetime)]
#[rquickjs::class]
pub struct Headers {
    entries: Vec<(String, String)>,
}

impl Headers {
    pub(crate) fn from_entries(entries: Vec<(String, String)>) -> Self {
        Self { entries }
    }

    pub(crate) fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    pub(crate) fn clone_inner(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

#[rquickjs::methods]
impl Headers {
    #[qjs(constructor)]
    pub fn new(init: Opt<Object<'_>>) -> rquickjs::Result<Self> {
        let mut entries = Vec::new();
        if let Some(obj) = init.0 {
            for prop in obj.props::<String, String>() {
                let (key, value) = prop?;
                entries.push((key.to_lowercase(), value));
            }
        }
        Ok(Self { entries })
    }

    pub fn get(&self, name: String) -> Option<String> {
        let name = name.to_lowercase();
        let values: Vec<&str> = self
            .entries
            .iter()
            .filter(|(k, _)| k == &name)
            .map(|(_, v)| v.as_str())
            .collect();
        if values.is_empty() {
            None
        } else {
            Some(values.join(", "))
        }
    }

    pub fn set(&mut self, name: String, value: String) {
        let name = name.to_lowercase();
        self.entries.retain(|(k, _)| k != &name);
        self.entries.push((name, value));
    }

    pub fn append(&mut self, name: String, value: String) {
        self.entries.push((name.to_lowercase(), value));
    }

    pub fn has(&self, name: String) -> bool {
        let name = name.to_lowercase();
        self.entries.iter().any(|(k, _)| k == &name)
    }

    pub fn delete(&mut self, name: String) {
        let name = name.to_lowercase();
        self.entries.retain(|(k, _)| k != &name);
    }
}

#[cfg(test)]
mod tests {
    use rquickjs::{AsyncContext, AsyncRuntime, Class};

    use super::*;

    #[tokio::test]
    async fn test_construct_with_init_and_get() {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();

            let result: String = ctx.eval(r#"
                let h = new Headers({"Content-Type": "text/html"});
                h.get("content-type")
            "#).unwrap();
            assert_eq!(result, "text/html");
        })
        .await;
    }

    #[tokio::test]
    async fn test_case_insensitive_get() {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();

            let result: String = ctx.eval(r#"
                let h = new Headers({"X-Custom": "value"});
                h.get("x-custom")
            "#).unwrap();
            assert_eq!(result, "value");
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();

            let result: String = ctx.eval(r#"
                let h = new Headers();
                h.set("content-type", "application/json");
                h.get("content-type")
            "#).unwrap();
            assert_eq!(result, "application/json");
        })
        .await;
    }

    #[tokio::test]
    async fn test_append_joins_values() {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();

            let result: String = ctx.eval(r#"
                let h = new Headers();
                h.append("accept", "text/html");
                h.append("accept", "application/json");
                h.get("accept")
            "#).unwrap();
            assert_eq!(result, "text/html, application/json");
        })
        .await;
    }

    #[tokio::test]
    async fn test_has_and_delete() {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();

            let result: String = ctx.eval(r#"
                let h = new Headers({"X-Key": "val"});
                let before = h.has("x-key");
                h.delete("x-key");
                let after = h.has("x-key");
                `${before},${after}`
            "#).unwrap();
            assert_eq!(result, "true,false");
        })
        .await;
    }

    #[tokio::test]
    async fn test_get_missing_returns_null() {
        let runtime = AsyncRuntime::new().unwrap();
        let context = AsyncContext::full(&runtime).await.unwrap();
        rquickjs::async_with!(context => |ctx| {
            Class::<Headers>::define(&ctx.globals()).unwrap();

            let result: rquickjs::Value = ctx.eval(r#"
                let h = new Headers();
                h.get("missing")
            "#).unwrap();
            assert!(result.is_null() || result.is_undefined());
        })
        .await;
    }
}
