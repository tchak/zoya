use std::collections::HashMap;

use http::request::Parts;
use zoya_package::QualifiedPath;
use zoya_value::{Value, ValueData};

/// Module path for `std::http` types.
fn http_module() -> QualifiedPath {
    QualifiedPath::new(vec!["std".into(), "http".into()])
}

/// Module path for `std::option` types.
fn option_module() -> QualifiedPath {
    QualifiedPath::new(vec!["std".into(), "option".into()])
}

/// Build `Option::None` as a Zoya Value.
fn option_none() -> Value {
    Value::EnumVariant {
        enum_name: "Option".into(),
        variant_name: "None".into(),
        module: option_module(),
        data: ValueData::Unit,
    }
}

/// Build `Option::Some(value)` as a Zoya Value.
fn option_some(value: Value) -> Value {
    Value::EnumVariant {
        enum_name: "Option".into(),
        variant_name: "Some".into(),
        module: option_module(),
        data: ValueData::Tuple(vec![value]),
    }
}

/// Build a `Body::Text(string)` as a Zoya Value.
fn body_text(text: String) -> Value {
    Value::EnumVariant {
        enum_name: "Body".into(),
        variant_name: "Text".into(),
        module: http_module(),
        data: ValueData::Tuple(vec![Value::String(text)]),
    }
}

/// Map an HTTP method string to its Zoya `Method` enum variant name.
fn method_variant_name(method: &http::Method) -> &'static str {
    match *method {
        http::Method::GET => "Get",
        http::Method::POST => "Post",
        http::Method::PUT => "Put",
        http::Method::PATCH => "Patch",
        http::Method::DELETE => "Delete",
        http::Method::HEAD => "Head",
        http::Method::OPTIONS => "Options",
        _ => "Get",
    }
}

/// Convert an Axum request (parts + body bytes) into a Zoya `Request` Value.
pub(crate) fn axum_request_to_value(parts: &Parts, body_bytes: &[u8]) -> Value {
    let url = Value::String(parts.uri.path().to_string());

    let method = Value::EnumVariant {
        enum_name: "Method".into(),
        variant_name: method_variant_name(&parts.method).into(),
        module: http_module(),
        data: ValueData::Unit,
    };

    let mut headers = HashMap::new();
    for (name, value) in &parts.headers {
        if let Ok(v) = value.to_str() {
            headers.insert(
                Value::String(name.as_str().to_string()),
                Value::String(v.to_string()),
            );
        }
    }
    let headers_val = Value::Dict(headers);

    let body = if body_bytes.is_empty() {
        option_none()
    } else {
        let text = String::from_utf8_lossy(body_bytes).into_owned();
        option_some(body_text(text))
    };

    let mut fields = HashMap::new();
    fields.insert("url".into(), url);
    fields.insert("method".into(), method);
    fields.insert("headers".into(), headers_val);
    fields.insert("body".into(), body);

    Value::Struct {
        name: "Request".into(),
        module: http_module(),
        data: ValueData::Struct(fields),
    }
}

/// Error type for response conversion failures.
#[derive(Debug)]
pub(crate) struct ConvertError(pub(crate) String);

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Convert a Zoya `Response` Value into an Axum HTTP response.
pub(crate) fn value_to_axum_response(
    value: Value,
) -> Result<axum::response::Response, ConvertError> {
    let Value::Struct { name, data, .. } = value else {
        return Err(ConvertError(format!(
            "expected Response struct, got {}",
            value.type_name()
        )));
    };
    if name != "Response" {
        return Err(ConvertError(format!(
            "expected Response struct, got {}",
            name
        )));
    }
    let ValueData::Struct(fields) = data else {
        return Err(ConvertError("expected Response with named fields".into()));
    };

    let status = match fields.get("status") {
        Some(Value::Int(n)) => http::StatusCode::from_u16(*n as u16)
            .map_err(|e| ConvertError(format!("invalid status code: {e}")))?,
        _ => {
            return Err(ConvertError("missing or invalid status field".into()));
        }
    };

    let body_string = match fields.get("body") {
        Some(Value::EnumVariant {
            variant_name, data, ..
        }) => match variant_name.as_str() {
            "None" => String::new(),
            "Some" => {
                let ValueData::Tuple(inner) = data else {
                    return Err(ConvertError("invalid Option::Some data".into()));
                };
                match inner.first() {
                    Some(Value::EnumVariant {
                        variant_name: body_variant,
                        data: body_data,
                        ..
                    }) => match body_variant.as_str() {
                        "Text" => {
                            let ValueData::Tuple(text_inner) = body_data else {
                                return Err(ConvertError("invalid Body::Text data".into()));
                            };
                            match text_inner.first() {
                                Some(Value::String(s)) => s.clone(),
                                _ => {
                                    return Err(ConvertError(
                                        "Body::Text should contain a String".into(),
                                    ));
                                }
                            }
                        }
                        "Json" => {
                            let ValueData::Tuple(json_inner) = body_data else {
                                return Err(ConvertError("invalid Body::Json data".into()));
                            };
                            match json_inner.first() {
                                Some(val) => val.to_json(),
                                None => {
                                    return Err(ConvertError(
                                        "Body::Json should contain a value".into(),
                                    ));
                                }
                            }
                        }
                        other => {
                            return Err(ConvertError(format!("unknown Body variant: {other}")));
                        }
                    },
                    _ => {
                        return Err(ConvertError("Option::Some should contain a Body".into()));
                    }
                }
            }
            other => {
                return Err(ConvertError(format!("unknown Option variant: {other}")));
            }
        },
        _ => String::new(),
    };

    let mut builder = axum::response::Response::builder().status(status);

    if let Some(Value::Dict(header_map)) = fields.get("headers") {
        for (k, v) in header_map {
            if let (Value::String(name), Value::String(val)) = (k, v) {
                let header_name = http::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|e| ConvertError(format!("invalid header name: {e}")))?;
                let header_value = http::header::HeaderValue::from_str(val)
                    .map_err(|e| ConvertError(format!("invalid header value: {e}")))?;
                builder = builder.header(header_name, header_value);
            }
        }
    }

    builder
        .body(axum::body::Body::from(body_string))
        .map_err(|e| ConvertError(format!("failed to build response: {e}")))
}

/// Convert a Zoya-style pathname (`:param`) to Axum-style (`{param}`).
pub(crate) fn convert_pathname(pathname: &str) -> String {
    pathname
        .split('/')
        .map(|segment| {
            if let Some(param) = segment.strip_prefix(':') {
                format!("{{{param}}}")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_pathname_static() {
        assert_eq!(convert_pathname("/hello"), "/hello");
        assert_eq!(convert_pathname("/a/b/c"), "/a/b/c");
    }

    #[test]
    fn convert_pathname_with_params() {
        assert_eq!(convert_pathname("/users/:id"), "/users/{id}");
        assert_eq!(
            convert_pathname("/users/:id/posts/:post_id"),
            "/users/{id}/posts/{post_id}"
        );
    }

    #[test]
    fn convert_pathname_root() {
        assert_eq!(convert_pathname("/"), "/");
    }
}
