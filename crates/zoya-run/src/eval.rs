use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx};

use zoya_ir::{DefinitionLookup, Type};
use zoya_package::QualifiedPath;
use zoya_value::{JSValue, Job, Value};

/// Create an async runtime and context for non-blocking script evaluation.
async fn create_async_runtime() -> Result<(AsyncRuntime, AsyncContext), EvalError> {
    let runtime = AsyncRuntime::new()
        .map_err(|e| EvalError::RuntimeError(format!("failed to create runtime: {e}")))?;
    let context = AsyncContext::full(&runtime)
        .await
        .map_err(|e| EvalError::RuntimeError(format!("failed to create context: {e}")))?;
    Ok((runtime, context))
}

/// Inject console and timer globals into the JS context.
fn inject_globals(ctx: &Ctx<'_>) -> Result<(), EvalError> {
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
    entry: &QualifiedPath,
    args: &[Value],
    jobs: &[(QualifiedPath, String)],
) -> Result<(Value, Vec<Job>), EvalError> {
    // Find the function in the definitions
    let func_def = definitions
        .get_function(entry)
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

    // Build the qualified path for $$run: root::main with pkg "myapp" → "myapp::main"
    let run_path = entry.with_root(name).to_string();

    // Create async runtime (no module system needed)
    let (_runtime, context) = create_async_runtime().await?;

    // Evaluate the script inside the async context
    let (value, raw_jobs) = rquickjs::async_with!(context => |ctx| {
        inject_globals(&ctx)?;
        eval_script_async(
            &ctx,
            code,
            &run_path,
            args,
            return_type,
            definitions,
        )
        .await
    })
    .await?;

    // Convert raw jobs to Job structs
    if raw_jobs.is_empty() {
        return Ok((value, vec![]));
    }

    let job_type = Type::Enum {
        module: QualifiedPath::root(),
        name: "Job".to_string(),
        type_args: vec![],
        variants: vec![],
    };

    let mut converted_jobs = Vec::with_capacity(raw_jobs.len());
    for js_val in raw_jobs {
        let job_value =
            Value::from_js_value(js_val, &job_type, definitions).map_err(EvalError::from)?;
        let job = job_value.as_job(jobs).map_err(EvalError::from)?;
        converted_jobs.push(job);
    }

    Ok((value, converted_jobs))
}

/// Evaluate a plain JS script and call an entry function via `$$run`.
///
/// First evaluates `code` to define all functions in the global scope,
/// then calls `$$run(qualified_path, ...args)` which handles JS↔Zoya
/// value conversion internally. Uses `Promise::into_future()` to drive
/// both the microtask queue and spawned async tasks (e.g. timers).
async fn eval_script_async(
    ctx: &Ctx<'_>,
    code: &str,
    qualified_path: &str,
    args: &[Value],
    result_type: Type,
    definitions: &DefinitionLookup,
) -> Result<(Value, Vec<JSValue>), EvalError> {
    // Define all functions in global scope
    let _: rquickjs::Value = ctx.eval(code).catch(ctx).map_err(map_js_error)?;

    // Get the $$run function from globals
    let globals = ctx.globals();
    let run_fn: rquickjs::Function = globals
        .get("$$run")
        .map_err(|e| EvalError::RuntimeError(format!("$$run not found: {e}")))?;

    // Build args: qualified path string followed by each Value converted to JSValue
    let mut js_args = Vec::with_capacity(args.len() + 1);
    js_args.push(
        rquickjs::String::from_str(ctx.clone(), qualified_path)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?
            .into_value(),
    );
    for arg in args {
        let json = serde_json::to_value(arg).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        let js_arg: rquickjs::Value = rquickjs_serde::to_value(ctx.clone(), &json)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        js_args.push(js_arg);
    }

    // Call $$run — returns a Promise<JSValue>
    let promise_val: rquickjs::Value = run_fn
        .call((rquickjs::function::Rest(js_args),))
        .catch(ctx)
        .map_err(map_js_error)?;

    // Drive the Promise to completion — into_future() drives both the microtask
    // queue and spawned async tasks (timers), unlike finish() which only drives microtasks
    let promise = rquickjs::Promise::from_value(promise_val)
        .map_err(|e| EvalError::RuntimeError(format!("expected promise: {e}")))?;
    let resolved: rquickjs::Value = promise
        .into_future()
        .await
        .catch(ctx)
        .map_err(map_js_error)?;

    // $$run returns { value, jobs } — extract via serde roundtrip
    let result_json: serde_json::Value = rquickjs_serde::from_value(resolved)
        .map_err(|e| EvalError::RuntimeError(format!("failed to deserialize result: {e}")))?;

    let result_obj = result_json
        .as_object()
        .ok_or_else(|| EvalError::RuntimeError("$$run returned non-object".into()))?;

    let value_json = result_obj
        .get("value")
        .ok_or_else(|| EvalError::RuntimeError("missing value field".into()))?;
    let js_val: JSValue = serde_json::from_value(value_json.clone())
        .map_err(|e| EvalError::RuntimeError(format!("failed to parse value: {e}")))?;
    let value = Value::from_js_value(js_val, &result_type, definitions).map_err(EvalError::from)?;

    // Extract the jobs array
    let js_jobs = match result_obj.get("jobs") {
        Some(serde_json::Value::Array(arr)) => {
            let mut jobs = Vec::with_capacity(arr.len());
            for item in arr {
                let js_val: JSValue = serde_json::from_value(item.clone())
                    .map_err(|e| EvalError::RuntimeError(format!("failed to parse job: {e}")))?;
                jobs.push(js_val);
            }
            jobs
        }
        _ => vec![],
    };

    Ok((value, js_jobs))
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
