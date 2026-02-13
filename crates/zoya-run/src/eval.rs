use std::collections::HashMap;
use std::fmt;

use rquickjs::{BigInt, CatchResultExt, Context, Ctx, Runtime};

use zoya_ir::{EnumVariantType, Type, TypeVarId};

type EnumLookup = HashMap<String, (Vec<TypeVarId>, Vec<(String, EnumVariantType)>)>;
type StructLookup = HashMap<String, (Vec<TypeVarId>, Vec<(String, Type)>)>;

/// Lookup table for resolving recursive type stubs.
pub(crate) struct TypeLookup {
    pub(crate) enums: EnumLookup,
    pub(crate) structs: StructLookup,
}

impl TypeLookup {
    fn resolve_enum_variants<'a>(
        &'a self,
        name: &str,
        variants: &'a [(String, EnumVariantType)],
        type_args: &[Type],
    ) -> Vec<(String, EnumVariantType)> {
        if !variants.is_empty() {
            return variants.to_vec();
        }
        if let Some((type_var_ids, real_variants)) = self.enums.get(name) {
            if type_args.is_empty() || type_var_ids.is_empty() {
                return real_variants.clone();
            }
            let mapping: HashMap<TypeVarId, Type> = type_var_ids
                .iter()
                .zip(type_args.iter())
                .map(|(id, ty)| (*id, ty.clone()))
                .collect();
            real_variants
                .iter()
                .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, &mapping)))
                .collect()
        } else {
            variants.to_vec()
        }
    }

    fn resolve_struct_fields<'a>(
        &'a self,
        name: &str,
        fields: &'a [(String, Type)],
        type_args: &[Type],
    ) -> Vec<(String, Type)> {
        if !fields.is_empty() {
            return fields.to_vec();
        }
        if let Some((type_var_ids, real_fields)) = self.structs.get(name) {
            if type_args.is_empty() || type_var_ids.is_empty() {
                return real_fields.clone();
            }
            let mapping: HashMap<TypeVarId, Type> = type_var_ids
                .iter()
                .zip(type_args.iter())
                .map(|(id, ty)| (*id, ty.clone()))
                .collect();
            real_fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, &mapping)))
                .collect()
        } else {
            fields.to_vec()
        }
    }
}

fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(elem) => Type::List(Box::new(substitute_type_vars(elem, mapping))),
        Type::Tuple(elems) => Type::Tuple(
            elems
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::Struct {
            name,
            type_args,
            fields,
        } => Type::Struct {
            name: name.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        },
        Type::Enum {
            name,
            type_args,
            variants,
        } => Type::Enum {
            name: name.clone(),
            type_args: type_args
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
            variants: variants
                .iter()
                .map(|(n, vt)| (n.clone(), substitute_variant_type_vars(vt, mapping)))
                .collect(),
        },
        Type::Int | Type::BigInt | Type::Float | Type::Bool | Type::String => ty.clone(),
    }
}

fn substitute_variant_type_vars(
    vt: &EnumVariantType,
    mapping: &HashMap<TypeVarId, Type>,
) -> EnumVariantType {
    match vt {
        EnumVariantType::Unit => EnumVariantType::Unit,
        EnumVariantType::Tuple(types) => EnumVariantType::Tuple(
            types
                .iter()
                .map(|t| substitute_type_vars(t, mapping))
                .collect(),
        ),
        EnumVariantType::Struct(fields) => EnumVariantType::Struct(
            fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type_vars(t, mapping)))
                .collect(),
        ),
    }
}

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
/// then calls `entry_func()` and converts the result to a Zoya Value.
pub(crate) fn eval_script(
    ctx: &Ctx<'_>,
    code: &str,
    entry_func: &str,
    result_type: Type,
    type_lookup: &TypeLookup,
) -> Result<Value, EvalError> {
    // Define all functions in global scope
    let _: rquickjs::Value = ctx.eval(code).catch(ctx).map_err(map_js_error)?;
    // Call entry function and get result
    let js_val: rquickjs::Value = ctx
        .eval(format!("{}()", entry_func))
        .catch(ctx)
        .map_err(map_js_error)?;
    js_value_to_value(js_val, &result_type, type_lookup)
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

fn write_fields(f: &mut fmt::Formatter<'_>, fields: &[(String, Value)]) -> fmt::Result {
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
                } else if fields[0].0.starts_with('$') {
                    write!(f, "{}(", name)?;
                    write_comma_separated(f, fields.iter().map(|(_, v)| v))?;
                    write!(f, ")")
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
    #[error("panic: {0}")]
    Panic(String),
    #[error("runtime error: {0}")]
    RuntimeError(String),
}

/// Convert a JavaScript value to a Zoya Value based on expected type
fn js_value_to_value(
    js_val: rquickjs::Value<'_>,
    expected_type: &Type,
    type_lookup: &TypeLookup,
) -> Result<Value, EvalError> {
    match expected_type {
        Type::Int => {
            let val: f64 = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            if !val.is_finite() {
                return Err(EvalError::Panic("division by zero".to_string()));
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
                return Err(EvalError::Panic("division by zero".to_string()));
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
                let elem_value = js_value_to_value(elem_js, elem_type, type_lookup)?;
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
                let elem_value = js_value_to_value(elem_js, elem_type, type_lookup)?;
                values.push(elem_value);
            }
            Ok(Value::Tuple(values))
        }
        Type::Struct {
            name,
            type_args,
            fields,
        } => {
            let resolved_fields = type_lookup.resolve_struct_fields(name, fields, type_args);
            let obj: rquickjs::Object = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let mut field_values = Vec::new();
            for (field_name, field_type) in &resolved_fields {
                let field_js: rquickjs::Value = obj
                    .get(field_name.as_str())
                    .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                let field_value = js_value_to_value(field_js, field_type, type_lookup)?;
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
            type_args,
            variants,
        } => {
            let resolved_variants =
                type_lookup.resolve_enum_variants(enum_name, variants, type_args);
            let obj: rquickjs::Object = js_val
                .get()
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
            let tag: String = obj
                .get("$tag")
                .map_err(|e| EvalError::RuntimeError(e.to_string()))?;

            let variant_type = resolved_variants
                .iter()
                .find(|(vname, _)| vname == &tag)
                .map(|(_, vt)| vt)
                .ok_or_else(|| EvalError::RuntimeError(format!("unknown enum variant: {}", tag)))?;

            let fields = match variant_type {
                EnumVariantType::Unit => EnumValueFields::Unit,
                EnumVariantType::Tuple(field_types) => {
                    let mut values = Vec::new();
                    for (i, field_type) in field_types.iter().enumerate() {
                        let field_js: rquickjs::Value = obj
                            .get(format!("${}", i))
                            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
                        let field_value = js_value_to_value(field_js, field_type, type_lookup)?;
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
                        let field_value = js_value_to_value(field_js, field_type, type_lookup)?;
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
