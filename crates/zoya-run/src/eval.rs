use std::collections::HashMap;
use std::fmt;

use rquickjs::{BigInt, CatchResultExt, Context, Ctx, FromJs, Runtime};

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
        Type::Set(elem) => Type::Set(Box::new(substitute_type_vars(elem, mapping))),
        Type::Dict(key, val) => Type::Dict(
            Box::new(substitute_type_vars(key, mapping)),
            Box::new(substitute_type_vars(val, mapping)),
        ),
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
            module,
            name,
            type_args,
            fields,
        } => Type::Struct {
            module: module.clone(),
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
            module,
            name,
            type_args,
            variants,
        } => Type::Enum {
            module: module.clone(),
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
    // Call entry function, extract into JSValue via FromJs, then convert to Value
    let js_val: JSValue = ctx
        .eval(format!("$$zoya_to_js({}())", entry_func))
        .catch(ctx)
        .map_err(map_js_error)?;
    Value::from_js_value(js_val, &result_type, type_lookup)
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
    Set(Vec<Value>),
    Dict(Vec<(Value, Value)>),
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
            Value::Set(items) => {
                write!(f, "{{")?;
                write_comma_separated(f, items)?;
                write!(f, "}}")
            }
            Value::Dict(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
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

/// Intermediate representation of a JavaScript value, decoupled from QuickJS runtime.
#[derive(Debug, Clone)]
pub(crate) enum JSValue {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array {
        tag: Option<String>,
        items: Vec<JSValue>,
    },
    Object {
        tag: Option<String>,
        fields: HashMap<String, JSValue>,
    },
}

impl<'js> FromJs<'js> for JSValue {
    #[allow(clippy::only_used_in_recursion)]
    fn from_js(ctx: &Ctx<'js>, value: rquickjs::Value<'js>) -> rquickjs::Result<Self> {
        if value.is_bool() {
            return Ok(JSValue::Bool(value.as_bool().unwrap()));
        }
        if value.is_int() {
            return Ok(JSValue::Int(value.as_int().unwrap() as i64));
        }
        if value.type_of() == rquickjs::Type::BigInt {
            let big: BigInt = value.get()?;
            let n = big
                .to_i64()
                .map_err(|_| rquickjs::Error::new_from_js("bigint", "i64"))?;
            return Ok(JSValue::BigInt(n));
        }
        if value.is_float() {
            return Ok(JSValue::Float(value.as_float().unwrap()));
        }
        if value.is_string() {
            let s: String = value.get()?;
            return Ok(JSValue::String(s));
        }
        if value.is_array() {
            let array: rquickjs::Array = value.get()?;
            let mut items = Vec::with_capacity(array.len());
            for i in 0..array.len() {
                let elem: rquickjs::Value = array.get(i)?;
                items.push(JSValue::from_js(ctx, elem)?);
            }
            // Read $tag property from the array-as-object
            let obj: rquickjs::Object = array.into_object();
            let tag: Option<String> = obj.get("$tag")?;
            return Ok(JSValue::Array { tag, items });
        }
        if value.is_object() {
            let obj: rquickjs::Object = value.get()?;
            let mut tag = None;
            let mut fields = HashMap::new();
            for result in obj.props::<String, rquickjs::Value>() {
                let (key, val) = result?;
                if key == "$tag" {
                    tag = Some(val.get::<String>()?);
                } else {
                    fields.insert(key, JSValue::from_js(ctx, val)?);
                }
            }
            return Ok(JSValue::Object { tag, fields });
        }
        Err(rquickjs::Error::new_from_js(value.type_name(), "JSValue"))
    }
}

impl Value {
    /// Convert a `JSValue` to a Zoya `Value` guided by the expected Zoya type.
    pub(crate) fn from_js_value(
        js: JSValue,
        ty: &Type,
        type_lookup: &TypeLookup,
    ) -> Result<Value, EvalError> {
        match (js, ty) {
            (JSValue::Int(n), Type::Int) => Ok(Value::Int(n)),
            (JSValue::Float(f), Type::Int) => {
                if !f.is_finite() {
                    return Err(EvalError::Panic("division by zero".to_string()));
                }
                Ok(Value::Int(f as i64))
            }
            (JSValue::Int(n), Type::Float) => Ok(Value::Float(n as f64)),
            (JSValue::Float(f), Type::Float) => {
                if !f.is_finite() {
                    return Err(EvalError::Panic("division by zero".to_string()));
                }
                Ok(Value::Float(f))
            }
            (JSValue::BigInt(n), Type::BigInt) => Ok(Value::BigInt(n)),
            (JSValue::Bool(b), Type::Bool) => Ok(Value::Bool(b)),
            (JSValue::String(s), Type::String) => Ok(Value::String(s)),
            (JSValue::Array { tag: None, items }, Type::List(elem_type)) => {
                let values = items
                    .into_iter()
                    .map(|item| Value::from_js_value(item, elem_type, type_lookup))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(values))
            }
            (JSValue::Array { tag: None, items }, Type::Tuple(elem_types)) => {
                let values = items
                    .into_iter()
                    .zip(elem_types.iter())
                    .map(|(item, et)| Value::from_js_value(item, et, type_lookup))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Tuple(values))
            }
            (
                JSValue::Array {
                    tag: Some(ref t),
                    items,
                },
                Type::Set(elem_type),
            ) if t == "Set" => {
                let values = items
                    .into_iter()
                    .map(|item| Value::from_js_value(item, elem_type, type_lookup))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Set(values))
            }
            (
                JSValue::Array {
                    tag: Some(ref t),
                    items,
                },
                Type::Dict(key_type, val_type),
            ) if t == "Dict" => {
                let mut entries = Vec::with_capacity(items.len());
                for item in items {
                    if let JSValue::Array { items: pair, .. } = item {
                        if pair.len() == 2 {
                            let mut iter = pair.into_iter();
                            let key =
                                Value::from_js_value(iter.next().unwrap(), key_type, type_lookup)?;
                            let val =
                                Value::from_js_value(iter.next().unwrap(), val_type, type_lookup)?;
                            entries.push((key, val));
                        } else {
                            return Err(EvalError::RuntimeError(
                                "dict entry must be a 2-element array".to_string(),
                            ));
                        }
                    } else {
                        return Err(EvalError::RuntimeError(
                            "dict entry must be an array".to_string(),
                        ));
                    }
                }
                Ok(Value::Dict(entries))
            }
            (
                JSValue::Object { tag: None, fields },
                Type::Struct {
                    name,
                    type_args,
                    fields: type_fields,
                    ..
                },
            ) => {
                let resolved_fields =
                    type_lookup.resolve_struct_fields(name, type_fields, type_args);
                let mut field_values = Vec::with_capacity(resolved_fields.len());
                for (field_name, field_type) in &resolved_fields {
                    let js_field = fields.get(field_name).ok_or_else(|| {
                        EvalError::RuntimeError(format!("missing struct field: {field_name}"))
                    })?;
                    let field_value =
                        Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
                    field_values.push((field_name.clone(), field_value));
                }
                Ok(Value::Struct {
                    name: name.clone(),
                    fields: field_values,
                })
            }
            (
                JSValue::Object {
                    tag: Some(variant_name),
                    fields,
                },
                Type::Enum {
                    name: enum_name,
                    type_args,
                    variants,
                    ..
                },
            ) => {
                let resolved_variants =
                    type_lookup.resolve_enum_variants(enum_name, variants, type_args);
                let variant_type = resolved_variants
                    .iter()
                    .find(|(vname, _)| vname == &variant_name)
                    .map(|(_, vt)| vt)
                    .ok_or_else(|| {
                        EvalError::RuntimeError(format!("unknown enum variant: {variant_name}"))
                    })?;

                let enum_fields = match variant_type {
                    EnumVariantType::Unit => EnumValueFields::Unit,
                    EnumVariantType::Tuple(field_types) => {
                        let mut values = Vec::with_capacity(field_types.len());
                        for (i, field_type) in field_types.iter().enumerate() {
                            let key = format!("${i}");
                            let js_field = fields.get(&key).ok_or_else(|| {
                                EvalError::RuntimeError(format!("missing tuple field: {key}"))
                            })?;
                            let val =
                                Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
                            values.push(val);
                        }
                        EnumValueFields::Tuple(values)
                    }
                    EnumVariantType::Struct(field_defs) => {
                        let mut field_values = Vec::with_capacity(field_defs.len());
                        for (field_name, field_type) in field_defs {
                            let js_field = fields.get(field_name).ok_or_else(|| {
                                EvalError::RuntimeError(format!(
                                    "missing enum struct field: {field_name}"
                                ))
                            })?;
                            let val =
                                Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
                            field_values.push((field_name.clone(), val));
                        }
                        EnumValueFields::Struct(field_values)
                    }
                };

                Ok(Value::Enum {
                    enum_name: enum_name.clone(),
                    variant_name,
                    fields: enum_fields,
                })
            }
            (_, Type::Function { .. }) => Err(EvalError::RuntimeError(
                "cannot convert function value from JS".to_string(),
            )),
            (_, Type::Var(id)) => Err(EvalError::RuntimeError(format!(
                "unresolved type variable: {id}"
            ))),
            (js, ty) => Err(EvalError::RuntimeError(format!(
                "type mismatch: cannot convert {js_kind} to {ty}",
                js_kind = js_value_kind(&js),
            ))),
        }
    }
}

fn js_value_kind(js: &JSValue) -> &'static str {
    match js {
        JSValue::Int(_) => "Int",
        JSValue::BigInt(_) => "BigInt",
        JSValue::Float(_) => "Float",
        JSValue::Bool(_) => "Bool",
        JSValue::String(_) => "String",
        JSValue::Array { .. } => "Array",
        JSValue::Object { .. } => "Object",
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
