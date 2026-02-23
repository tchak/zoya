mod parse;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::ser::{SerializeMap, SerializeSeq};
use zoya_ir::{DefinitionLookup, EnumVariantType, QualifiedPath, Type};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("panic: {0}")]
    Panic(String),
    #[error("type mismatch: cannot convert {from} to {to}")]
    TypeMismatch { from: String, to: String },
    #[error("missing field: {0}")]
    MissingField(String),
    #[error("unknown variant: {0}")]
    UnknownVariant(String),
    #[error("unsupported conversion: {0}")]
    UnsupportedConversion(String),
    #[error("parse error: {0}")]
    ParseError(String),
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
    Task(Box<Value>),
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
            (Value::Task(a), Value::Task(b)) => a == b,
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
            Value::Task(inner) => inner.hash(state),
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
            Value::Task(inner) => write!(f, "Task({})", inner),
        }
    }
}

fn qualified_type_name(module: &QualifiedPath, name: &str) -> String {
    let segments: Vec<&str> = module
        .segments()
        .iter()
        .skip_while(|s| s.as_str() == "root")
        .map(|s| s.as_str())
        .chain(std::iter::once(name))
        .collect();
    segments.join("::")
}

fn serialize_data<S: serde::Serializer>(
    serializer: S,
    type_name: &str,
    data: &ValueData,
) -> Result<S::Ok, S::Error> {
    match data {
        ValueData::Unit => {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry("type", type_name)?;
            map.end()
        }
        ValueData::Tuple(values) => {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("type", type_name)?;
            map.serialize_entry("data", values)?;
            map.end()
        }
        ValueData::Struct(fields) => {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("type", type_name)?;
            map.serialize_entry("data", fields)?;
            map.end()
        }
    }
}

impl serde::Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Int(n) => serializer.serialize_i64(*n),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::BigInt(n) => serializer.serialize_str(&n.to_string()),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::String(s) => serializer.serialize_str(s),
            Value::Tuple(values) | Value::List(values) => values.serialize(serializer),
            Value::Set(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for v in values {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Value::Dict(entries) => {
                let mut seq = serializer.serialize_seq(Some(entries.len()))?;
                for (k, v) in entries {
                    seq.serialize_element(&(k, v))?;
                }
                seq.end()
            }
            Value::Struct { name, module, data } => {
                let type_name = qualified_type_name(module, name);
                serialize_data(serializer, &type_name, data)
            }
            Value::EnumVariant {
                enum_name,
                variant_name,
                module,
                data,
            } => {
                let name = format!("{enum_name}::{variant_name}");
                let type_name = qualified_type_name(module, &name);
                serialize_data(serializer, &type_name, data)
            }
            Value::Task(inner) => inner.serialize(serializer),
        }
    }
}

impl Value {
    /// Parse a CLI argument string into a typed `Value`.
    ///
    /// Strings are passed through raw (no quotes needed). All other types
    /// are tokenized and parsed according to the expected type.
    pub fn parse(
        input: &str,
        expected: &Type,
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        parse::parse_value(input, expected, type_lookup)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::BigInt(_) => "BigInt",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::String(_) => "String",
            Value::List(_) => "List",
            Value::Tuple(_) => "Tuple",
            Value::Set(_) => "Set",
            Value::Dict(_) => "Dict",
            Value::Struct { .. } => "Struct",
            Value::EnumVariant { .. } => "EnumVariant",
            Value::Task(_) => "Task",
        }
    }

    /// Validate that this value matches an expected Zoya type.
    pub fn check_type(&self, expected: &Type, type_lookup: &DefinitionLookup) -> Result<(), Error> {
        match (self, expected) {
            (Value::Int(_), Type::Int) => Ok(()),
            (Value::BigInt(_), Type::BigInt) => Ok(()),
            (Value::Float(_), Type::Float) => Ok(()),
            (Value::Bool(_), Type::Bool) => Ok(()),
            (Value::String(_), Type::String) => Ok(()),
            (Value::List(items), Type::List(elem_type)) => {
                for item in items {
                    item.check_type(elem_type, type_lookup)?;
                }
                Ok(())
            }
            (Value::Tuple(items), Type::Tuple(types)) => {
                if items.len() != types.len() {
                    return Err(Error::TypeMismatch {
                        from: format!("Tuple({})", items.len()),
                        to: format!("Tuple({})", types.len()),
                    });
                }
                for (item, ty) in items.iter().zip(types.iter()) {
                    item.check_type(ty, type_lookup)?;
                }
                Ok(())
            }
            (Value::Set(items), Type::Set(elem_type)) => {
                for item in items {
                    item.check_type(elem_type, type_lookup)?;
                }
                Ok(())
            }
            (Value::Dict(entries), Type::Dict(key_type, val_type)) => {
                for (k, v) in entries {
                    k.check_type(key_type, type_lookup)?;
                    v.check_type(val_type, type_lookup)?;
                }
                Ok(())
            }
            (
                Value::Struct { name, module, data },
                Type::Struct {
                    name: type_name,
                    module: type_module,
                    type_args,
                    fields: type_fields,
                },
            ) => {
                if name != type_name || module != type_module {
                    return Err(Error::TypeMismatch {
                        from: format!("{}::{}", module, name),
                        to: format!("{}::{}", type_module, type_name),
                    });
                }
                let resolved_fields =
                    type_lookup.resolve_struct_fields(module, name, type_fields, type_args);
                check_value_data(data, &resolved_fields, name, type_lookup)
            }
            (
                Value::EnumVariant {
                    enum_name,
                    variant_name,
                    module,
                    data,
                },
                Type::Enum {
                    name: type_name,
                    module: type_module,
                    type_args,
                    variants,
                },
            ) => {
                if enum_name != type_name || module != type_module {
                    return Err(Error::TypeMismatch {
                        from: format!("{}::{}", module, enum_name),
                        to: format!("{}::{}", type_module, type_name),
                    });
                }
                let resolved_variants =
                    type_lookup.resolve_enum_variants(module, enum_name, variants, type_args);
                let variant_type = resolved_variants
                    .iter()
                    .find(|(vname, _)| vname == variant_name)
                    .map(|(_, vt)| vt)
                    .ok_or_else(|| Error::UnknownVariant(variant_name.clone()))?;
                let variant_fields = variant_type_to_fields(variant_type);
                let context = format!("{}::{}", enum_name, variant_name);
                check_value_data(data, &variant_fields, &context, type_lookup)
            }
            (Value::Task(inner), Type::Task(elem_type)) => inner.check_type(elem_type, type_lookup),
            (_, Type::Var(_)) => Ok(()),
            (_, Type::Function { .. }) => Err(Error::UnsupportedConversion(
                "function args not supported".into(),
            )),
            _ => Err(Error::TypeMismatch {
                from: self.type_name().to_string(),
                to: expected.to_string(),
            }),
        }
    }
}

/// Intermediate representation of a JavaScript value, decoupled from QuickJS runtime.
#[derive(Debug, Clone)]
pub enum JSValue {
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

impl JSValue {
    pub fn kind(&self) -> &'static str {
        match self {
            JSValue::Int(_) => "Int",
            JSValue::BigInt(_) => "BigInt",
            JSValue::Float(_) => "Float",
            JSValue::Bool(_) => "Bool",
            JSValue::String(_) => "String",
            JSValue::Array { .. } => "Array",
            JSValue::Object { .. } => "Object",
        }
    }
}

fn variant_type_to_fields(variant_type: &EnumVariantType) -> Vec<(String, Type)> {
    match variant_type {
        EnumVariantType::Unit => vec![],
        EnumVariantType::Tuple(types) => types
            .iter()
            .enumerate()
            .map(|(i, t)| (format!("${i}"), t.clone()))
            .collect(),
        EnumVariantType::Struct(fields) => fields.clone(),
    }
}

fn check_value_data(
    data: &ValueData,
    expected_fields: &[(String, Type)],
    context_name: &str,
    type_lookup: &DefinitionLookup,
) -> Result<(), Error> {
    match data {
        ValueData::Unit => {
            if !expected_fields.is_empty() {
                return Err(Error::TypeMismatch {
                    from: format!("{} (unit)", context_name),
                    to: format!("{} (with fields)", context_name),
                });
            }
        }
        ValueData::Tuple(values) => {
            for (val, (_, field_type)) in values.iter().zip(expected_fields.iter()) {
                val.check_type(field_type, type_lookup)?;
            }
        }
        ValueData::Struct(map) => {
            for (field_name, field_type) in expected_fields {
                if let Some(val) = map.get(field_name) {
                    val.check_type(field_type, type_lookup)?;
                } else {
                    return Err(Error::MissingField(field_name.clone()));
                }
            }
        }
    }
    Ok(())
}

fn convert_js_fields_to_value_data(
    fields: &HashMap<String, JSValue>,
    expected_fields: &[(String, Type)],
    type_lookup: &DefinitionLookup,
) -> Result<ValueData, Error> {
    if expected_fields.is_empty() {
        Ok(ValueData::Unit)
    } else if expected_fields[0].0.starts_with('$') {
        let mut values = Vec::with_capacity(expected_fields.len());
        for (field_name, field_type) in expected_fields {
            let js_field = fields
                .get(field_name)
                .ok_or_else(|| Error::MissingField(field_name.clone()))?;
            let field_value = Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
            values.push(field_value);
        }
        Ok(ValueData::Tuple(values))
    } else {
        let mut map = HashMap::with_capacity(expected_fields.len());
        for (field_name, field_type) in expected_fields {
            let js_field = fields
                .get(field_name)
                .ok_or_else(|| Error::MissingField(field_name.clone()))?;
            let field_value = Value::from_js_value(js_field.clone(), field_type, type_lookup)?;
            map.insert(field_name.clone(), field_value);
        }
        Ok(ValueData::Struct(map))
    }
}

fn value_data_to_fields(data: ValueData) -> HashMap<String, JSValue> {
    match data {
        ValueData::Unit => HashMap::new(),
        ValueData::Tuple(values) => values
            .into_iter()
            .enumerate()
            .map(|(i, v)| (format!("${i}"), JSValue::from(v)))
            .collect(),
        ValueData::Struct(map) => map
            .into_iter()
            .map(|(k, v)| (k, JSValue::from(v)))
            .collect(),
    }
}

impl From<Value> for JSValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Int(n) => JSValue::Int(n),
            Value::BigInt(n) => JSValue::BigInt(n),
            Value::Float(f) => JSValue::Float(f),
            Value::Bool(b) => JSValue::Bool(b),
            Value::String(s) => JSValue::String(s),
            Value::List(items) => JSValue::Array {
                tag: None,
                items: items.into_iter().map(JSValue::from).collect(),
            },
            Value::Tuple(items) => JSValue::Array {
                tag: None,
                items: items.into_iter().map(JSValue::from).collect(),
            },
            Value::Set(items) => JSValue::Array {
                tag: Some("Set".to_string()),
                items: items.into_iter().map(JSValue::from).collect(),
            },
            Value::Dict(entries) => JSValue::Array {
                tag: Some("Dict".to_string()),
                items: entries
                    .into_iter()
                    .map(|(k, v)| JSValue::Array {
                        tag: None,
                        items: vec![JSValue::from(k), JSValue::from(v)],
                    })
                    .collect(),
            },
            Value::Struct { data, .. } => JSValue::Object {
                tag: None,
                fields: value_data_to_fields(data),
            },
            Value::EnumVariant {
                variant_name, data, ..
            } => JSValue::Object {
                tag: Some(variant_name),
                fields: value_data_to_fields(data),
            },
            Value::Task(inner) => JSValue::Array {
                tag: Some("Task".to_string()),
                items: vec![JSValue::from(*inner)],
            },
        }
    }
}

#[cfg(feature = "quickjs")]
impl<'js> rquickjs::IntoJs<'js> for JSValue {
    fn into_js(self, ctx: &rquickjs::Ctx<'js>) -> rquickjs::Result<rquickjs::Value<'js>> {
        match self {
            JSValue::Int(n) => n.into_js(ctx),
            JSValue::BigInt(n) => rquickjs::BigInt::from_i64(ctx.clone(), n)?.into_js(ctx),
            JSValue::Float(f) => f.into_js(ctx),
            JSValue::Bool(b) => b.into_js(ctx),
            JSValue::String(s) => s.into_js(ctx),
            JSValue::Array { tag, items } => {
                let array = rquickjs::Array::new(ctx.clone())?;
                for (i, item) in items.into_iter().enumerate() {
                    array.set(i, item.into_js(ctx)?)?;
                }
                if let Some(tag) = tag {
                    let obj = array.as_object();
                    obj.set("$tag", tag)?;
                }
                array.into_js(ctx)
            }
            JSValue::Object { tag, fields } => {
                let obj = rquickjs::Object::new(ctx.clone())?;
                if let Some(tag) = tag {
                    obj.set("$tag", tag)?;
                }
                for (key, val) in fields {
                    obj.set(key, val.into_js(ctx)?)?;
                }
                obj.into_js(ctx)
            }
        }
    }
}

#[cfg(feature = "quickjs")]
impl<'js> rquickjs::FromJs<'js> for JSValue {
    #[allow(clippy::only_used_in_recursion)]
    fn from_js(ctx: &rquickjs::Ctx<'js>, value: rquickjs::Value<'js>) -> rquickjs::Result<Self> {
        if value.is_bool() {
            return Ok(JSValue::Bool(value.as_bool().unwrap()));
        }
        if value.is_int() {
            return Ok(JSValue::Int(value.as_int().unwrap() as i64));
        }
        if value.type_of() == rquickjs::Type::BigInt {
            let big: rquickjs::BigInt = value.get()?;
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
    pub fn from_js_value(
        js: JSValue,
        ty: &Type,
        type_lookup: &DefinitionLookup,
    ) -> Result<Value, Error> {
        match (js, ty) {
            (JSValue::Int(n), Type::Int) => Ok(Value::Int(n)),
            (JSValue::Float(f), Type::Int) => {
                if !f.is_finite() {
                    return Err(Error::Panic("division by zero".to_string()));
                }
                Ok(Value::Int(f as i64))
            }
            (JSValue::Int(n), Type::Float) => Ok(Value::Float(n as f64)),
            (JSValue::Float(f), Type::Float) => {
                if !f.is_finite() {
                    return Err(Error::Panic("division by zero".to_string()));
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
                Type::Task(inner_type),
            ) if t == "Task" => {
                let inner_js = items
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::TypeMismatch {
                        from: "empty Task array".to_string(),
                        to: "Task".to_string(),
                    })?;
                let inner = Value::from_js_value(inner_js, inner_type, type_lookup)?;
                Ok(Value::Task(Box::new(inner)))
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
                            return Err(Error::TypeMismatch {
                                from: "array".to_string(),
                                to: "2-element dict entry".to_string(),
                            });
                        }
                    } else {
                        return Err(Error::TypeMismatch {
                            from: "non-array".to_string(),
                            to: "dict entry".to_string(),
                        });
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
                let data = convert_js_fields_to_value_data(&fields, &resolved_fields, type_lookup)?;
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
                    .ok_or_else(|| Error::UnknownVariant(variant_name.clone()))?;
                let variant_fields = variant_type_to_fields(variant_type);
                let data = convert_js_fields_to_value_data(&fields, &variant_fields, type_lookup)?;

                Ok(Value::EnumVariant {
                    module: module.clone(),
                    enum_name: enum_name.clone(),
                    variant_name,
                    data,
                })
            }
            (_, Type::Function { .. }) => Err(Error::UnsupportedConversion(
                "cannot convert function value from JS".to_string(),
            )),
            (_, Type::Var(id)) => Err(Error::UnsupportedConversion(format!(
                "unresolved type variable: {id}"
            ))),
            (js, ty) => Err(Error::TypeMismatch {
                from: js.kind().to_string(),
                to: ty.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_int() {
        assert_eq!(Value::Int(42).to_json(), "42");
    }

    #[test]
    fn serialize_float() {
        assert_eq!(Value::Float(3.14).to_json(), "3.14");
    }

    #[test]
    fn serialize_bigint() {
        assert_eq!(Value::BigInt(123).to_json(), r#""123""#);
    }

    #[test]
    fn serialize_bool() {
        assert_eq!(Value::Bool(true).to_json(), "true");
        assert_eq!(Value::Bool(false).to_json(), "false");
    }

    #[test]
    fn serialize_string() {
        assert_eq!(Value::String("hello".into()).to_json(), r#""hello""#);
    }

    #[test]
    fn serialize_tuple() {
        let val = Value::Tuple(vec![Value::Int(1), Value::Bool(true)]);
        assert_eq!(val.to_json(), "[1,true]");
    }

    #[test]
    fn serialize_list() {
        let val = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(val.to_json(), "[1,2,3]");
    }

    #[test]
    fn serialize_set() {
        let mut set = HashSet::new();
        set.insert(Value::Int(1));
        let val = Value::Set(set);
        assert_eq!(val.to_json(), "[1]");
    }

    #[test]
    fn serialize_dict() {
        let mut map = HashMap::new();
        map.insert(Value::String("a".into()), Value::Int(1));
        let val = Value::Dict(map);
        assert_eq!(val.to_json(), r#"[["a",1]]"#);
    }

    #[test]
    fn serialize_struct_unit() {
        let val = Value::Struct {
            name: "Foo".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"Foo"}"#);
    }

    #[test]
    fn serialize_struct_tuple() {
        let val = Value::Struct {
            name: "Point".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Tuple(vec![Value::Int(1), Value::Int(2)]),
        };
        assert_eq!(val.to_json(), r#"{"type":"Point","data":[1,2]}"#);
    }

    #[test]
    fn serialize_struct_fields() {
        let mut fields = HashMap::new();
        fields.insert("x".into(), Value::Int(10));
        let val = Value::Struct {
            name: "Point".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Struct(fields),
        };
        assert_eq!(val.to_json(), r#"{"type":"Point","data":{"x":10}}"#);
    }

    #[test]
    fn serialize_struct_in_module() {
        let val = Value::Struct {
            name: "Foo".into(),
            module: QualifiedPath::new(vec!["root".into(), "mymod".into()]),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"mymod::Foo"}"#);
    }

    #[test]
    fn serialize_struct_in_nested_module() {
        let val = Value::Struct {
            name: "Bar".into(),
            module: QualifiedPath::new(vec!["root".into(), "a".into(), "b".into()]),
            data: ValueData::Tuple(vec![Value::Int(1)]),
        };
        assert_eq!(val.to_json(), r#"{"type":"a::b::Bar","data":[1]}"#);
    }

    #[test]
    fn serialize_enum_unit() {
        let val = Value::EnumVariant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"Color::Red"}"#);
    }

    #[test]
    fn serialize_enum_tuple() {
        let val = Value::EnumVariant {
            enum_name: "Shape".into(),
            variant_name: "Circle".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Tuple(vec![Value::Float(5.0)]),
        };
        assert_eq!(val.to_json(), r#"{"type":"Shape::Circle","data":[5.0]}"#);
    }

    #[test]
    fn serialize_enum_struct() {
        let mut fields = HashMap::new();
        fields.insert("name".into(), Value::String("Alice".into()));
        let val = Value::EnumVariant {
            enum_name: "Result".into(),
            variant_name: "Ok".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Struct(fields),
        };
        assert_eq!(
            val.to_json(),
            r#"{"type":"Result::Ok","data":{"name":"Alice"}}"#
        );
    }

    #[test]
    fn serialize_enum_in_module() {
        let val = Value::EnumVariant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
            module: QualifiedPath::new(vec!["root".into(), "graphics".into()]),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"graphics::Color::Red"}"#);
    }

    #[test]
    fn serialize_nested() {
        let inner = Value::Struct {
            name: "Unit".into(),
            module: QualifiedPath::new(vec!["root".into()]),
            data: ValueData::Unit,
        };
        let val = Value::List(vec![inner, Value::Int(42)]);
        assert_eq!(val.to_json(), r#"[{"type":"Unit"},42]"#);
    }

    // ── check_type tests ─────────────────────────────────────────────

    fn empty_lookup() -> DefinitionLookup {
        DefinitionLookup::empty()
    }

    #[test]
    fn check_type_primitives_match() {
        let lookup = empty_lookup();
        assert!(Value::Int(42).check_type(&Type::Int, &lookup).is_ok());
        assert!(Value::BigInt(1).check_type(&Type::BigInt, &lookup).is_ok());
        assert!(Value::Float(3.14).check_type(&Type::Float, &lookup).is_ok());
        assert!(Value::Bool(true).check_type(&Type::Bool, &lookup).is_ok());
        assert!(
            Value::String("hi".into())
                .check_type(&Type::String, &lookup)
                .is_ok()
        );
    }

    #[test]
    fn check_type_primitives_mismatch() {
        let lookup = empty_lookup();
        assert!(Value::Int(42).check_type(&Type::String, &lookup).is_err());
        assert!(
            Value::String("hi".into())
                .check_type(&Type::Int, &lookup)
                .is_err()
        );
        assert!(Value::Bool(true).check_type(&Type::Float, &lookup).is_err());
    }

    #[test]
    fn check_type_list_ok() {
        let lookup = empty_lookup();
        let val = Value::List(vec![Value::Int(1), Value::Int(2)]);
        assert!(
            val.check_type(&Type::List(Box::new(Type::Int)), &lookup)
                .is_ok()
        );
    }

    #[test]
    fn check_type_list_wrong_elem() {
        let lookup = empty_lookup();
        let val = Value::List(vec![Value::Int(1), Value::String("x".into())]);
        assert!(
            val.check_type(&Type::List(Box::new(Type::Int)), &lookup)
                .is_err()
        );
    }

    #[test]
    fn check_type_tuple_ok() {
        let lookup = empty_lookup();
        let val = Value::Tuple(vec![Value::Int(1), Value::Bool(true)]);
        assert!(
            val.check_type(&Type::Tuple(vec![Type::Int, Type::Bool]), &lookup)
                .is_ok()
        );
    }

    #[test]
    fn check_type_tuple_wrong_length() {
        let lookup = empty_lookup();
        let val = Value::Tuple(vec![Value::Int(1)]);
        assert!(
            val.check_type(&Type::Tuple(vec![Type::Int, Type::Bool]), &lookup)
                .is_err()
        );
    }

    #[test]
    fn check_type_set_ok() {
        let lookup = empty_lookup();
        let mut set = HashSet::new();
        set.insert(Value::Int(1));
        let val = Value::Set(set);
        assert!(
            val.check_type(&Type::Set(Box::new(Type::Int)), &lookup)
                .is_ok()
        );
    }

    #[test]
    fn check_type_dict_ok() {
        let lookup = empty_lookup();
        let mut map = HashMap::new();
        map.insert(Value::String("a".into()), Value::Int(1));
        let val = Value::Dict(map);
        assert!(
            val.check_type(
                &Type::Dict(Box::new(Type::String), Box::new(Type::Int)),
                &lookup
            )
            .is_ok()
        );
    }

    #[test]
    fn check_type_struct_ok() {
        let lookup = empty_lookup();
        let module = QualifiedPath::new(vec!["root".into()]);
        let val = Value::Struct {
            name: "Point".into(),
            module: module.clone(),
            data: ValueData::Struct({
                let mut m = HashMap::new();
                m.insert("x".into(), Value::Int(1));
                m.insert("y".into(), Value::Int(2));
                m
            }),
        };
        let ty = Type::Struct {
            module: module.clone(),
            name: "Point".into(),
            type_args: vec![],
            fields: vec![("x".into(), Type::Int), ("y".into(), Type::Int)],
        };
        assert!(val.check_type(&ty, &lookup).is_ok());
    }

    #[test]
    fn check_type_struct_name_mismatch() {
        let lookup = empty_lookup();
        let module = QualifiedPath::new(vec!["root".into()]);
        let val = Value::Struct {
            name: "Point".into(),
            module: module.clone(),
            data: ValueData::Unit,
        };
        let ty = Type::Struct {
            module: module.clone(),
            name: "Other".into(),
            type_args: vec![],
            fields: vec![],
        };
        assert!(val.check_type(&ty, &lookup).is_err());
    }

    #[test]
    fn check_type_enum_ok() {
        let lookup = empty_lookup();
        let module = QualifiedPath::new(vec!["root".into()]);
        let val = Value::EnumVariant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
            module: module.clone(),
            data: ValueData::Unit,
        };
        let ty = Type::Enum {
            module: module.clone(),
            name: "Color".into(),
            type_args: vec![],
            variants: vec![
                ("Red".into(), EnumVariantType::Unit),
                ("Green".into(), EnumVariantType::Unit),
            ],
        };
        assert!(val.check_type(&ty, &lookup).is_ok());
    }

    #[test]
    fn check_type_enum_name_mismatch() {
        let lookup = empty_lookup();
        let module = QualifiedPath::new(vec!["root".into()]);
        let val = Value::EnumVariant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
            module: module.clone(),
            data: ValueData::Unit,
        };
        let ty = Type::Enum {
            module: module.clone(),
            name: "Shape".into(),
            type_args: vec![],
            variants: vec![],
        };
        assert!(val.check_type(&ty, &lookup).is_err());
    }

    #[test]
    fn check_type_var_accepts_anything() {
        let lookup = empty_lookup();
        use zoya_ir::TypeVarId;
        assert!(
            Value::Int(42)
                .check_type(&Type::Var(TypeVarId(0)), &lookup)
                .is_ok()
        );
        assert!(
            Value::String("hi".into())
                .check_type(&Type::Var(TypeVarId(1)), &lookup)
                .is_ok()
        );
    }

    #[test]
    fn check_type_function_rejected() {
        let lookup = empty_lookup();
        let ty = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Int),
        };
        assert!(Value::Int(42).check_type(&ty, &lookup).is_err());
    }
}
