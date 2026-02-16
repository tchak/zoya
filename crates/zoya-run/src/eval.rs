use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

use rquickjs::{BigInt, CatchResultExt, Context, Ctx, FromJs, Runtime};

use zoya_ir::{DefinitionLookup, EnumVariantType, QualifiedPath, Type};

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
    type_lookup: &DefinitionLookup,
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

#[derive(Debug, Clone)]
pub enum ValueData {
    Unit,
    Tuple(Vec<Value>),
    Struct(HashMap<String, Value>),
}

impl PartialEq for ValueData {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ValueData::Unit, ValueData::Unit) => true,
            (ValueData::Tuple(a), ValueData::Tuple(b)) => a == b,
            (ValueData::Struct(a), ValueData::Struct(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ValueData {}

impl Hash for ValueData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            ValueData::Unit => {}
            ValueData::Tuple(values) => values.hash(state),
            ValueData::Struct(map) => {
                state.write_usize(map.len());
                // Order-independent hash for HashMap
                let mut hash_sum: u64 = 0;
                for (k, v) in map {
                    let mut entry_hasher = std::hash::DefaultHasher::new();
                    k.hash(&mut entry_hasher);
                    v.hash(&mut entry_hasher);
                    hash_sum = hash_sum.wrapping_add(entry_hasher.finish());
                }
                state.write_u64(hash_sum);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Set(HashSet<Value>),
    Dict(HashMap<Value, Value>),
    Struct {
        name: String,
        module: QualifiedPath,
        data: ValueData,
    },
    EnumVariant {
        enum_name: String,
        variant_name: String,
        module: QualifiedPath,
        data: ValueData,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
            (Value::Dict(a), Value::Dict(b)) => a == b,
            (
                Value::Struct {
                    name: n1,
                    module: m1,
                    data: d1,
                },
                Value::Struct {
                    name: n2,
                    module: m2,
                    data: d2,
                },
            ) => n1 == n2 && m1 == m2 && d1 == d2,
            (
                Value::EnumVariant {
                    enum_name: en1,
                    variant_name: vn1,
                    module: m1,
                    data: d1,
                },
                Value::EnumVariant {
                    enum_name: en2,
                    variant_name: vn2,
                    module: m2,
                    data: d2,
                },
            ) => en1 == en2 && vn1 == vn2 && m1 == m2 && d1 == d2,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Int(n) => n.hash(state),
            Value::BigInt(n) => n.hash(state),
            Value::Float(f) => f.to_bits().hash(state),
            Value::Bool(b) => b.hash(state),
            Value::String(s) => s.hash(state),
            Value::List(elements) => elements.hash(state),
            Value::Tuple(elements) => elements.hash(state),
            Value::Set(set) => {
                state.write_usize(set.len());
                let mut hash_sum: u64 = 0;
                for v in set {
                    let mut h = std::hash::DefaultHasher::new();
                    v.hash(&mut h);
                    hash_sum = hash_sum.wrapping_add(h.finish());
                }
                state.write_u64(hash_sum);
            }
            Value::Dict(map) => {
                state.write_usize(map.len());
                let mut hash_sum: u64 = 0;
                for (k, v) in map {
                    let mut h = std::hash::DefaultHasher::new();
                    k.hash(&mut h);
                    v.hash(&mut h);
                    hash_sum = hash_sum.wrapping_add(h.finish());
                }
                state.write_u64(hash_sum);
            }
            Value::Struct { name, module, data } => {
                name.hash(state);
                module.hash(state);
                data.hash(state);
            }
            Value::EnumVariant {
                enum_name,
                variant_name,
                module,
                data,
            } => {
                enum_name.hash(state);
                variant_name.hash(state);
                module.hash(state);
                data.hash(state);
            }
        }
    }
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

fn write_data(f: &mut fmt::Formatter<'_>, prefix: &str, data: &ValueData) -> fmt::Result {
    match data {
        ValueData::Unit => write!(f, "{}", prefix),
        ValueData::Tuple(values) => {
            write!(f, "{}(", prefix)?;
            write_comma_separated(f, values)?;
            write!(f, ")")
        }
        ValueData::Struct(map) => {
            if map.is_empty() {
                write!(f, "{} {{}}", prefix)
            } else {
                write!(f, "{} {{ ", prefix)?;
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, map[*k])?;
                }
                write!(f, " }}")
            }
        }
    }
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
            Value::Struct { name, data, .. } => write_data(f, name, data),
            Value::Set(items) => {
                write!(f, "{{")?;
                let mut sorted: Vec<_> = items.iter().collect();
                sorted.sort_by_key(|a| a.to_string());
                write_comma_separated(f, sorted)?;
                write!(f, "}}")
            }
            Value::Dict(entries) => {
                write!(f, "{{")?;
                let mut sorted: Vec<_> = entries.iter().collect();
                sorted.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()));
                for (i, (k, v)) in sorted.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::EnumVariant {
                enum_name,
                variant_name,
                data,
                ..
            } => {
                let path = format!("{}::{}", enum_name, variant_name);
                write_data(f, &path, data)
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
        type_lookup: &DefinitionLookup,
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
                    .collect::<Result<HashSet<_>, _>>()?;
                Ok(Value::Set(values))
            }
            (
                JSValue::Array {
                    tag: Some(ref t),
                    items,
                },
                Type::Dict(key_type, val_type),
            ) if t == "Dict" => {
                let mut map = HashMap::with_capacity(items.len());
                for item in items {
                    if let JSValue::Array { items: pair, .. } = item {
                        if pair.len() == 2 {
                            let mut iter = pair.into_iter();
                            let key =
                                Value::from_js_value(iter.next().unwrap(), key_type, type_lookup)?;
                            let val =
                                Value::from_js_value(iter.next().unwrap(), val_type, type_lookup)?;
                            map.insert(key, val);
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
                Ok(Value::Dict(map))
            }
            (
                JSValue::Object { tag: None, fields },
                Type::Struct {
                    module,
                    name,
                    type_args,
                    fields: type_fields,
                },
            ) => {
                let resolved_fields =
                    type_lookup.resolve_struct_fields(module, name, type_fields, type_args);
                let data = if resolved_fields.is_empty() {
                    ValueData::Unit
                } else if resolved_fields[0].0.starts_with('$') {
                    let mut values = Vec::with_capacity(resolved_fields.len());
                    for (field_name, field_type) in &resolved_fields {
                        let js_field = fields.get(field_name).ok_or_else(|| {
                            EvalError::RuntimeError(format!("missing tuple field: {field_name}"))
                        })?;
                        let field_value =
                            Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
                        values.push(field_value);
                    }
                    ValueData::Tuple(values)
                } else {
                    let mut map = HashMap::with_capacity(resolved_fields.len());
                    for (field_name, field_type) in &resolved_fields {
                        let js_field = fields.get(field_name).ok_or_else(|| {
                            EvalError::RuntimeError(format!("missing struct field: {field_name}"))
                        })?;
                        let field_value =
                            Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
                        map.insert(field_name.clone(), field_value);
                    }
                    ValueData::Struct(map)
                };
                Ok(Value::Struct {
                    module: module.clone(),
                    name: name.clone(),
                    data,
                })
            }
            (
                JSValue::Object {
                    tag: Some(variant_name),
                    fields,
                },
                Type::Enum {
                    module,
                    name: enum_name,
                    type_args,
                    variants,
                },
            ) => {
                let resolved_variants =
                    type_lookup.resolve_enum_variants(module, enum_name, variants, type_args);
                let variant_type = resolved_variants
                    .iter()
                    .find(|(vname, _)| vname == &variant_name)
                    .map(|(_, vt)| vt)
                    .ok_or_else(|| {
                        EvalError::RuntimeError(format!("unknown enum variant: {variant_name}"))
                    })?;

                let data = match variant_type {
                    EnumVariantType::Unit => ValueData::Unit,
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
                        ValueData::Tuple(values)
                    }
                    EnumVariantType::Struct(field_defs) => {
                        let mut map = HashMap::with_capacity(field_defs.len());
                        for (field_name, field_type) in field_defs {
                            let js_field = fields.get(field_name).ok_or_else(|| {
                                EvalError::RuntimeError(format!(
                                    "missing enum struct field: {field_name}"
                                ))
                            })?;
                            let val =
                                Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
                            map.insert(field_name.clone(), val);
                        }
                        ValueData::Struct(map)
                    }
                };

                Ok(Value::EnumVariant {
                    module: module.clone(),
                    enum_name: enum_name.clone(),
                    variant_name,
                    data,
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
