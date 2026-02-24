use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, FromJs, IntoJs};

use zoya_ir::{DefinitionLookup, Type};
use zoya_package::QualifiedPath;
use zoya_value::{JSValue, Value};

/// Create an async runtime and context for non-blocking script evaluation.
pub(crate) async fn create_async_runtime() -> Result<(AsyncRuntime, AsyncContext), EvalError> {
    let runtime = AsyncRuntime::new()
        .map_err(|e| EvalError::RuntimeError(format!("failed to create runtime: {e}")))?;
    let context = AsyncContext::full(&runtime)
        .await
        .map_err(|e| EvalError::RuntimeError(format!("failed to create context: {e}")))?;
    Ok((runtime, context))
}

/// Inject console and timer globals into the JS context.
pub(crate) fn inject_globals(ctx: &Ctx<'_>) -> Result<(), EvalError> {
    inject_console(ctx)
        .and_then(|()| inject_timers(ctx))
        .map_err(|e| EvalError::RuntimeError(format!("failed to inject globals: {e}")))
}

static NEXT_TIMER_ID: AtomicI32 = AtomicI32::new(1);

/// setTimeout callback — named function to unify `Ctx<'js>` and `Function<'js>` lifetimes.
fn set_timeout<'js>(ctx: Ctx<'js>, callback: rquickjs::Function<'js>, ms: u32) -> i32 {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    ctx.spawn(async move {
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        let _ = callback.call::<_, ()>(());
    });
    id
}

fn inject_timers(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    globals.set(
        "setTimeout",
        rquickjs::Function::new(ctx.clone(), set_timeout)?,
    )?;
    globals.set(
        "clearTimeout",
        rquickjs::Function::new(ctx.clone(), |_id: i32| {})?,
    )?;
    Ok(())
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

/// Validate arguments, create a JS runtime, and execute a function from compiled code.
///
/// This is the implementation backing the public `run()` and `run_async()` functions.
pub(crate) async fn run_code(
    name: &str,
    code: &str,
    definitions: &DefinitionLookup,
    entry: QualifiedPath,
    args: Vec<Value>,
) -> Result<Value, EvalError> {
    // Find the function in the definitions
    let func_def = definitions
        .get_function(&entry)
        .ok_or_else(|| EvalError::RuntimeError(format!("function {} not found", entry)))?;

    // Validate argument count
    if func_def.params.len() != args.len() {
        return Err(EvalError::RuntimeError(format!(
            "{}() expects {} argument(s), got {}",
            entry.last(),
            func_def.params.len(),
            args.len()
        )));
    }

    let return_type = func_def.return_type.clone();

    // Validate each arg's type
    for (i, (arg, param_type)) in args.iter().zip(func_def.params.iter()).enumerate() {
        arg.check_type(param_type, definitions)
            .map_err(|e| EvalError::RuntimeError(format!("argument {} type mismatch: {}", i, e)))?;
    }

    // Build the entry function name using the package name
    let entry_func = zoya_codegen::format_export_path(&entry, name);

    // Create async runtime (no module system needed)
    let (_runtime, context) = create_async_runtime().await?;

    // Evaluate the script inside the async context
    rquickjs::async_with!(context => |ctx| {
        inject_globals(&ctx)?;
        eval_script_async(
            &ctx,
            code,
            &entry_func,
            &args,
            return_type,
            definitions,
        )
        .await
    })
    .await
}

/// Evaluate a plain JS script and call an entry function asynchronously.
///
/// First evaluates `code` to define all functions in the global scope,
/// then calls `entry_func(args...)` and converts the result to a Zoya Value.
/// Uses `Promise::into_future()` to drive both the microtask queue and
/// spawned async tasks (e.g. timers) to completion.
pub(crate) async fn eval_script_async(
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

    // $$zoya_to_js is async, so it always returns a Promise
    let promise_val: rquickjs::Value = ctx
        .eval(format!("$$zoya_to_js({}({}))", entry_func, arg_list))
        .catch(ctx)
        .map_err(map_js_error)?;

    // Drive the Promise to completion — into_future() drives both the microtask
    // queue and spawned async tasks (timers), unlike finish() which only drives microtasks
    let promise = rquickjs::Promise::from_value(promise_val)
        .map_err(|e| EvalError::RuntimeError(format!("expected promise: {e}")))?;
    let resolved = promise
        .into_future()
        .await
        .catch(ctx)
        .map_err(map_js_error)?;

    // Convert resolved JS value to Zoya Value
    let js_val =
        JSValue::from_js(ctx, resolved).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
    Value::from_js_value(js_val, &result_type, type_lookup).map_err(EvalError::from)
}

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("panic: {0}")]
    Panic(String),
    #[error("runtime error: {0}")]
    RuntimeError(String),
    #[error("{0}")]
    LoadError(#[from] zoya_loader::LoaderError<String>),
    #[error("{0}")]
    TypeError(#[from] zoya_ir::TypeError),
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
