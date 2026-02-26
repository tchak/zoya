use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zoya_ir::{DefinitionLookup, Type};

use crate::{Error, Value, ValueData, variant_type_to_fields};

/// Intermediate representation of a JavaScript value, decoupled from QuickJS runtime.
///
/// Uses serde's externally-tagged format so values can be deserialized from
/// `rquickjs_serde` → `serde_json::Value` → `JSValue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JSValue {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<JSValue>),
    Object(HashMap<String, JSValue>),
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
            JSValue::Array(_) => "Array",
            JSValue::Object(_) => "Object",
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
            (JSValue::Array(items), Type::List(elem_type)) => {
                let values = items
                    .into_iter()
                    .map(|item| Value::from_js_value(item, elem_type, definitions))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::List(values))
            }
            (JSValue::Array(items), Type::Tuple(elem_types)) => {
                let values = items
                    .into_iter()
                    .zip(elem_types.iter())
                    .map(|(item, et)| Value::from_js_value(item, et, definitions))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Tuple(values))
            }
            (JSValue::Array(items), Type::Task(inner_type)) => {
                let inner_js =
                    items
                        .into_iter()
                        .next()
                        .ok_or_else(|| Error::TypeMismatch {
                            from: "empty Task array".to_string(),
                            to: "Task".to_string(),
                        })?;
                let inner = Value::from_js_value(inner_js, inner_type, definitions)?;
                Ok(Value::Task(Box::new(inner)))
            }
            (JSValue::Array(items), Type::Set(elem_type)) => {
                let values = items
                    .into_iter()
                    .map(|item| Value::from_js_value(item, elem_type, definitions))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Set(values))
            }
            (JSValue::Array(items), Type::Dict(key_type, val_type)) => {
                let mut entries = Vec::with_capacity(items.len());
                for item in items {
                    if let JSValue::Array(pair) = item {
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
                JSValue::Object(fields),
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
                JSValue::Object(mut fields),
                Type::Enum {
                    module,
                    name: enum_name,
                    type_args,
                    variants,
                },
            ) => {
                // Extract $tag from the object to determine the variant
                let variant_name = match fields.remove("$tag") {
                    Some(JSValue::String(name)) => name,
                    _ => {
                        return Err(Error::TypeMismatch {
                            from: "Object (missing $tag)".to_string(),
                            to: format!("Enum {}", enum_name),
                        });
                    }
                };
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
