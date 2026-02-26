use std::collections::HashMap;

use zoya_ir::{DefinitionLookup, Type};

use crate::{Error, Value, ValueData, variant_type_to_fields};

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
    Bytes(Vec<u8>),
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
            JSValue::Bytes(_) => "Bytes",
        }
    }
}

fn convert_js_fields_to_value_data(
    fields: &HashMap<String, JSValue>,
    expected_fields: &[(String, Type)],
    definitions: &DefinitionLookup,
) -> Result<ValueData, Error> {
    if expected_fields.is_empty() {
        Ok(ValueData::Unit)
    } else if expected_fields[0].0.starts_with('$') {
        let mut values = Vec::with_capacity(expected_fields.len());
        for (field_name, field_type) in expected_fields {
            let js_field = fields
                .get(field_name)
                .ok_or_else(|| Error::MissingField(field_name.clone()))?;
            let field_value = Value::from_js_value(js_field.clone(), field_type, definitions)?;
            values.push(field_value);
        }
        Ok(ValueData::Tuple(values))
    } else {
        let mut map = HashMap::with_capacity(expected_fields.len());
        for (field_name, field_type) in expected_fields {
            let js_field = fields
                .get(field_name)
                .ok_or_else(|| Error::MissingField(field_name.clone()))?;
            let field_value = Value::from_js_value(js_field.clone(), field_type, definitions)?;
            map.insert(field_name.clone(), field_value);
        }
        Ok(ValueData::Struct(map))
    }
}

#[cfg(feature = "quickjs")]
impl<'js> rquickjs::FromJs<'js> for JSValue {
    #[allow(clippy::only_used_in_recursion)]
    fn from_js(ctx: &rquickjs::Ctx<'js>, value: rquickjs::Value<'js>) -> rquickjs::Result<Self> {
        // Check for Uint8Array before other object/array checks
        if let Ok(typed_array) = rquickjs::TypedArray::<u8>::from_value(value.clone()) {
            return Ok(JSValue::Bytes(typed_array.as_bytes().unwrap().to_vec()));
        }
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
        definitions: &DefinitionLookup,
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
            (JSValue::Bytes(data), Type::Bytes) => Ok(Value::Bytes(data)),
            (JSValue::Array { tag: None, items }, Type::List(elem_type)) => {
                let values = items
                    .into_iter()
                    .map(|item| Value::from_js_value(item, elem_type, definitions))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(values))
            }
            (JSValue::Array { tag: None, items }, Type::Tuple(elem_types)) => {
                let values = items
                    .into_iter()
                    .zip(elem_types.iter())
                    .map(|(item, et)| Value::from_js_value(item, et, definitions))
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
                let inner = Value::from_js_value(inner_js, inner_type, definitions)?;
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
                    .map(|item| Value::from_js_value(item, elem_type, definitions))
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
                                Value::from_js_value(iter.next().unwrap(), key_type, definitions)?;
                            let val =
                                Value::from_js_value(iter.next().unwrap(), val_type, definitions)?;
                            entries.push((key, val));
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
                Ok(Value::Dict(entries))
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
                    definitions.resolve_struct_fields(module, name, type_fields, type_args);
                let data = convert_js_fields_to_value_data(&fields, &resolved_fields, definitions)?;
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
                    definitions.resolve_enum_variants(module, enum_name, variants, type_args);
                let variant_type = resolved_variants
                    .iter()
                    .find(|(vname, _)| vname == &variant_name)
                    .map(|(_, vt)| vt)
                    .ok_or_else(|| Error::UnknownVariant(variant_name.clone()))?;
                let variant_fields = variant_type_to_fields(variant_type);
                let data = convert_js_fields_to_value_data(&fields, &variant_fields, definitions)?;

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
