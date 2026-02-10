use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rquickjs::loader::{BuiltinResolver, Loader, ModuleLoader, Resolver};
use rquickjs::{BigInt, CatchResultExt, Context, Ctx, Module, Result as QjsResult, Runtime};

use zoya_ir::{EnumVariantType, Type};

/// Virtual module storage - maps module names to source code
#[derive(Clone)]
pub(crate) struct VirtualModules {
    modules: Arc<HashMap<String, String>>,
}

impl VirtualModules {
    pub fn new(modules: HashMap<String, String>) -> Self {
        Self {
            modules: Arc::new(modules),
        }
    }

    /// Get source code for a module
    pub fn get(&self, name: &str) -> Option<String> {
        self.modules.get(name).cloned()
    }
}

/// Resolver for virtual modules
#[derive(Clone)]
pub(crate) struct VirtualResolver {
    modules: VirtualModules,
}

impl VirtualResolver {
    pub fn new(modules: VirtualModules) -> Self {
        Self { modules }
    }
}

impl Resolver for VirtualResolver {
    fn resolve(&mut self, _ctx: &Ctx<'_>, base: &str, name: &str) -> QjsResult<String> {
        // Check if we have this module registered
        if self.modules.get(name).is_some() {
            Ok(name.to_string())
        } else {
            Err(rquickjs::Error::new_resolving(base, name))
        }
    }
}

/// Loader for virtual modules
#[derive(Clone)]
pub(crate) struct VirtualLoader {
    modules: VirtualModules,
}

impl VirtualLoader {
    pub fn new(modules: VirtualModules) -> Self {
        Self { modules }
    }
}

impl Loader for VirtualLoader {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str) -> QjsResult<Module<'js>> {
        if let Some(source) = self.modules.get(name) {
            Module::declare(ctx.clone(), name, source)
        } else {
            Err(rquickjs::Error::new_loading(name))
        }
    }
}

/// Create a runtime and context configured for ESM module loading
pub(crate) fn create_module_runtime(
    virtual_modules: VirtualModules,
) -> Result<(Runtime, Context), String> {
    let runtime = Runtime::new().map_err(|e| e.to_string())?;

    let resolver = (
        VirtualResolver::new(virtual_modules.clone()),
        BuiltinResolver::default(),
    );
    let loader = (
        VirtualLoader::new(virtual_modules),
        ModuleLoader::default(),
    );

    runtime.set_loader(resolver, loader);

    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    Ok((runtime, context))
}

fn map_js_error(e: rquickjs::CaughtError<'_>) -> EvalError {
    let msg = e.to_string();
    if msg.contains("division by zero") {
        EvalError::DivisionByZero
    } else {
        EvalError::RuntimeError(msg)
    }
}

/// Evaluate an ESM module and get the result
///
/// The module_source should be ESM code that exports functions.
/// The entry_point is the function to call (e.g., "$root$main").
/// The result is retrieved from the module's "$result" export.
pub(crate) fn eval_module(
    ctx: &Ctx<'_>,
    module_name: &str,
    entry_func: &str,
    result_type: Type,
) -> Result<Value, EvalError> {
    // Create entry point script that imports the module and calls the function
    let entry_script = format!(
        r#"import {{ {} }} from '{}'; export const $result = {}();"#,
        entry_func, module_name, entry_func
    );

    // Declare and evaluate the entry module
    let entry_module = Module::declare(ctx.clone(), "__entry__", entry_script)
        .catch(ctx)
        .map_err(map_js_error)?;

    // Evaluate the module - eval() takes ownership and returns (Module<Evaluated>, Promise)
    let (evaluated_module, promise) = entry_module.eval().catch(ctx).map_err(map_js_error)?;

    // Check if the promise was rejected (module threw an error)
    // The result() method returns Err(Error::Exception) if rejected
    let _: () = promise.finish().catch(ctx).map_err(map_js_error)?;

    // Get the result from the module's exports
    let js_val: rquickjs::Value = evaluated_module.get("$result").catch(ctx).map_err(|e| {
        EvalError::RuntimeError(format!("failed to get result: {}", e))
    })?;

    js_value_to_value(ctx, js_val, &result_type)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Enum {
        enum_name: String,
        variant_name: String,
        fields: EnumValueFields,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnumValueFields {
    Unit,
    Tuple(Vec<Value>),
    Struct(Vec<(String, Value)>),
}

fn write_comma_separated(
    f: &mut fmt::Formatter<'_>,
    items: impl IntoIterator<Item = impl fmt::Display>,
) -> fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{}", item)?;
    }
    Ok(())
}

fn write_fields(
    f: &mut fmt::Formatter<'_>,
    fields: &[(String, Value)],
) -> fmt::Result {
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{}: {}", k, v)?;
    }
    Ok(())
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::BigInt(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::List(elements) => {
                write!(f, "[")?;
                write_comma_separated(f, elements)?;
                write!(f, "]")
            }
            Value::Tuple(elements) => {
                write!(f, "(")?;
                write_comma_separated(f, elements)?;
                if elements.len() == 1 {
                    write!(f, ",)")
                } else {
                    write!(f, ")")
                }
            }
            Value::Struct { name, fields } => {
                if fields.is_empty() {
                    write!(f, "{} {{}}", name)
                } else {
                    write!(f, "{} {{ ", name)?;
                    write_fields(f, fields)?;
                    write!(f, " }}")
                }
            }
            Value::Fn { params, ret } => {
                if params.is_empty() {
                    write!(f, "<fn() -> {}>", ret)
                } else if params.len() == 1 {
                    write!(f, "<fn({}) -> {}>", params[0], ret)
                } else {
                    write!(f, "<fn(")?;
                    write_comma_separated(f, params)?;
                    write!(f, ") -> {}>", ret)
                }
            }
            Value::Enum {
                enum_name,
                variant_name,
                fields,
            } => {
                let path = format!("{}::{}", enum_name, variant_name);
                match fields {
                    EnumValueFields::Unit => write!(f, "{}", path),
                    EnumValueFields::Tuple(values) => {
                        write!(f, "{}(", path)?;
                        write_comma_separated(f, values)?;
                        write!(f, ")")
                    }
                    EnumValueFields::Struct(field_values) => {
                        if field_values.is_empty() {
                            write!(f, "{} {{}}", path)
                        } else {
                            write!(f, "{} {{ ", path)?;
                            write_fields(f, field_values)?;
                            write!(f, " }}")
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EvalError {
    #[error("division by zero")]
    DivisionByZero,
    #[error("runtime error: {0}")]
    RuntimeError(String),
}

/// Convert a JavaScript value to a Zoya Value based on expected type
#[allow(clippy::only_used_in_recursion)]
fn js_value_to_value(
    ctx: &rquickjs::Ctx<'_>,
    js_val: rquickjs::Value<'_>,
    expected_type: &Type,
) -> Result<Value, EvalError> {
    match expected_type {
        Type::Int => {
            let val: f64 = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            if !val.is_finite() {
                return Err(EvalError::DivisionByZero);
            }
            Ok(Value::Int(val as i64))
        }
        Type::BigInt => {
            let val: BigInt = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let value = val.to_i64().map_err(|_| {
                EvalError::RuntimeError("BigInt value too large for i64".to_string())
            })?;
            Ok(Value::BigInt(value))
        }
        Type::Float => {
            let val: f64 = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            if !val.is_finite() {
                return Err(EvalError::DivisionByZero);
            }
            Ok(Value::Float(val))
        }
        Type::Bool => {
            let val: bool = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::Bool(val))
        }
        Type::String => {
            let val: String = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            Ok(Value::String(val))
        }
        Type::List(elem_type) => {
            let array: rquickjs::Array = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut values = Vec::new();
            for i in 0..array.len() {
                let elem_js: rquickjs::Value = array
                    .get(i)
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let elem_value = js_value_to_value(ctx, elem_js, elem_type)?;
                values.push(elem_value);
            }
            Ok(Value::List(values))
        }
        Type::Tuple(elem_types) => {
            let array: rquickjs::Array = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut values = Vec::new();
            for (i, elem_type) in elem_types.iter().enumerate() {
                let elem_js: rquickjs::Value = array
                    .get(i)
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let elem_value = js_value_to_value(ctx, elem_js, elem_type)?;
                values.push(elem_value);
            }
            Ok(Value::Tuple(values))
        }
        Type::Struct { name, fields, .. } => {
            let obj: rquickjs::Object = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut field_values = Vec::new();
            for (field_name, field_type) in fields {
                let field_js: rquickjs::Value = obj
                    .get(field_name.as_str())
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let field_value = js_value_to_value(ctx, field_js, field_type)?;
                field_values.push((field_name.clone(), field_value));
            }
            Ok(Value::Struct {
                name: name.clone(),
                fields: field_values,
            })
        }
        Type::Var(id) => Err(EvalError::RuntimeError(format!(
            "unresolved type variable: {}",
            id
        ))),
        Type::Function { params, ret } => Ok(Value::Fn {
            params: params.clone(),
            ret: ret.clone(),
        }),
        Type::Enum {
            name: enum_name,
            variants,
            ..
        } => {
            let obj: rquickjs::Object = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let tag: String = obj
                .get("$tag")
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            // Find the variant type
            let variant_type = variants
                .iter()
                .find(|(vname, _)| vname == &tag)
                .map(|(_, vt)| vt)
                .ok_or_else(|| {
                    EvalError::RuntimeError(format!("unknown enum variant: {}", tag))
                })?;

            let fields = match variant_type {
                EnumVariantType::Unit => EnumValueFields::Unit,
                EnumVariantType::Tuple(field_types) => {
                    let mut values = Vec::new();
                    for (i, field_type) in field_types.iter().enumerate() {
                        let field_js: rquickjs::Value = obj
                            .get(format!("${}", i))
                            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                        let field_value = js_value_to_value(ctx, field_js, field_type)?;
                        values.push(field_value);
                    }
                    EnumValueFields::Tuple(values)
                }
                EnumVariantType::Struct(field_defs) => {
                    let mut field_values = Vec::new();
                    for (field_name, field_type) in field_defs {
                        let field_js: rquickjs::Value = obj
                            .get(field_name.as_str())
                            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                        let field_value = js_value_to_value(ctx, field_js, field_type)?;
                        field_values.push((field_name.clone(), field_value));
                    }
                    EnumValueFields::Struct(field_values)
                }
            };

            Ok(Value::Enum {
                enum_name: enum_name.clone(),
                variant_name: tag,
                fields,
            })
        }
    }
}
