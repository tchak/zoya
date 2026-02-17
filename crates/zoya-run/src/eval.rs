use rquickjs::{CatchResultExt, Context, Ctx, IntoJs, Runtime};

use zoya_ir::{DefinitionLookup, Type};
use zoya_value::{JSValue, Value};

/// Create a runtime and context for plain script evaluation (no module system).
pub(crate) fn create_runtime() -> Result<(Runtime, Context), String> {
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    context
        .with(|ctx| inject_console(&ctx))
        .map_err(|e| format!("failed to inject console: {e}"))?;
    Ok((runtime, context))
}

fn inject_console(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    let console = rquickjs::Object::new(ctx.clone())?;
    console.set(
        "log",
        rquickjs::Function::new(ctx.clone(), |msg: String| {
            println!("{}", msg);
        })?,
    )?;
    globals.set("console", console)?;
    Ok(())
}

fn map_js_error(e: rquickjs::CaughtError<'_>) -> EvalError {
    let msg = e.to_string();
    if let Some((code, detail)) = parse_zoya_error(&msg) {
        match code {
            zoya_codegen::error_codes::PANIC => {
                EvalError::Panic(detail.unwrap_or("explicit panic").to_string())
            }
            _ => EvalError::Panic(format!("unknown error code: {code}")),
        }
    } else {
        EvalError::Panic(msg)
    }
}

fn parse_zoya_error(msg: &str) -> Option<(&str, Option<&str>)> {
    let idx = msg.find(zoya_codegen::error_codes::MARKER)?;
    let rest = &msg[idx + zoya_codegen::error_codes::MARKER.len()..];
    let first_line = rest.lines().next().unwrap_or(rest);
    match first_line.split_once(':') {
        Some((code, detail)) => Some((code, Some(detail))),
        None => Some((first_line, None)),
    }
}

/// Evaluate a plain JS script and call an entry function.
///
/// First evaluates `code` to define all functions in the global scope,
/// then calls `entry_func(args...)` and converts the result to a Zoya Value.
pub(crate) fn eval_script(
    ctx: &Ctx<'_>,
    code: &str,
    entry_func: &str,
    args: &[Value],
    result_type: Type,
    type_lookup: &DefinitionLookup,
) -> Result<Value, EvalError> {
    // Define all functions in global scope
    let _: rquickjs::Value = ctx.eval(code).catch(ctx).map_err(map_js_error)?;

    // Inject args as global variables
    let globals = ctx.globals();
    for (i, arg) in args.iter().enumerate() {
        let js_arg: rquickjs::Value = JSValue::from(arg.clone())
            .into_js(ctx)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        globals
            .set(format!("__zoya_arg{i}"), js_arg)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    }

    // Build call expression — wrap each arg in $$js_to_zoya() to convert
    // JSValue representation back to internal Zoya representation (HAMT sets/dicts)
    let arg_list = (0..args.len())
        .map(|i| format!("$$js_to_zoya(__zoya_arg{i})"))
        .collect::<Vec<_>>()
        .join(",");

    let js_val: JSValue = ctx
        .eval(format!("$$zoya_to_js({}({}))", entry_func, arg_list))
        .catch(ctx)
        .map_err(map_js_error)?;
    Value::from_js_value(js_val, &result_type, type_lookup).map_err(EvalError::from)
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EvalError {
    #[error("panic: {0}")]
    Panic(String),
    #[error("runtime error: {0}")]
    RuntimeError(String),
}

impl From<zoya_value::Error> for EvalError {
    fn from(e: zoya_value::Error) -> Self {
        match e {
            zoya_value::Error::Panic(msg) => EvalError::Panic(msg),
            other => EvalError::RuntimeError(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_zoya_error_panic_with_detail() {
        let msg = "Error: $$zoya:PANIC:division by zero";
        let (code, detail) = parse_zoya_error(msg).unwrap();
        assert_eq!(code, "PANIC");
        assert_eq!(detail, Some("division by zero"));
    }

    #[test]
    fn test_parse_zoya_error_panic_without_detail() {
        let msg = "Error: $$zoya:PANIC";
        let (code, detail) = parse_zoya_error(msg).unwrap();
        assert_eq!(code, "PANIC");
        assert_eq!(detail, None);
    }

    #[test]
    fn test_parse_zoya_error_no_marker() {
        let msg = "TypeError: undefined is not a function";
        assert!(parse_zoya_error(msg).is_none());
    }

    #[test]
    fn test_parse_zoya_error_with_multiline() {
        let msg = "Error: $$zoya:PANIC:something bad\n    at main (entry:1:1)";
        let (code, detail) = parse_zoya_error(msg).unwrap();
        assert_eq!(code, "PANIC");
        assert_eq!(detail, Some("something bad"));
    }

    #[test]
    fn test_parse_zoya_error_detail_with_colons() {
        let msg = "$$zoya:PANIC:value: 42";
        let (code, detail) = parse_zoya_error(msg).unwrap();
        assert_eq!(code, "PANIC");
        assert_eq!(detail, Some("value: 42"));
    }
}
