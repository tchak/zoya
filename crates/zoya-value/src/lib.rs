mod js;
mod json;
mod parse;

pub use js::JSValue;

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TerminationError {
    #[error("{0}")]
    Failed(String),
    #[error("unexpected return value: {0}")]
    UnexpectedReturn(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValueData {
    Unit,
    Tuple(Vec<Value>),
    Struct(HashMap<String, Value>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub path: QualifiedPath,
    pub args: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    BigInt(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Set(Vec<Value>),
    Dict(Vec<(Value, Value)>),
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
    Bytes(Vec<u8>),
    Json(serde_json::Value),
}

fn write_comma_separated(
    f: &mut fmt::Formatter<'_>,
    items: impl IntoIterator<Item = impl fmt::Display>,
) -> fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

fn write_value_data(f: &mut fmt::Formatter<'_>, data: &ValueData) -> fmt::Result {
    match data {
        ValueData::Unit => Ok(()),
        ValueData::Tuple(values) => {
            f.write_str("(")?;
            write_comma_separated(f, values)?;
            f.write_str(")")
        }
        ValueData::Struct(map) => {
            if map.is_empty() {
                f.write_str(" {}")
            } else {
                f.write_str(" { ")?;
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {}", map[*k])?;
                }
                f.write_str(" }")
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
            Value::Struct { name, data, .. } => {
                f.write_str(name)?;
                write_value_data(f, data)
            }
            Value::Set(items) => {
                f.write_str("{")?;
                let mut sorted = items.clone();
                sorted.sort_by_cached_key(|a| a.to_string());
                write_comma_separated(f, &sorted)?;
                f.write_str("}")
            }
            Value::Dict(entries) => {
                f.write_str("{")?;
                let mut sorted = entries.clone();
                sorted.sort_by_cached_key(|(k, _)| k.to_string());
                for (i, (k, v)) in sorted.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                f.write_str("}")
            }
            Value::EnumVariant {
                enum_name,
                variant_name,
                data,
                ..
            } => {
                f.write_str(enum_name)?;
                f.write_str("::")?;
                f.write_str(variant_name)?;
                write_value_data(f, data)
            }
            Value::Task(inner) => write!(f, "Task({})", inner),
            Value::Bytes(data) => {
                f.write_str("Bytes([")?;
                for (i, byte) in data.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{:02x}", byte)?;
                }
                f.write_str("])")
            }
            Value::Json(v) => write!(f, "{}", v),
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
    definitions: &DefinitionLookup,
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
                val.check_type(field_type, definitions)?;
            }
        }
        ValueData::Struct(map) => {
            for (field_name, field_type) in expected_fields {
                if let Some(val) = map.get(field_name) {
                    val.check_type(field_type, definitions)?;
                } else {
                    return Err(Error::MissingField(field_name.clone()));
                }
            }
        }
    }
    Ok(())
}

impl Value {
    /// Parse a CLI argument string into a typed `Value`.
    ///
    /// Strings are passed through raw (no quotes needed). All other types
    /// are tokenized and parsed according to the expected type.
    pub fn parse(
        input: &str,
        expected: &Type,
        definitions: &DefinitionLookup,
    ) -> Result<Value, Error> {
        parse::parse_value(input, expected, definitions)
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
            Value::Bytes(_) => "Bytes",
            Value::Json(_) => "Json",
        }
    }

    /// Validate that this value matches an expected Zoya type.
    pub fn check_type(&self, expected: &Type, definitions: &DefinitionLookup) -> Result<(), Error> {
        match (self, expected) {
            (Value::Int(_), Type::Int) => Ok(()),
            (Value::BigInt(_), Type::BigInt) => Ok(()),
            (Value::Float(_), Type::Float) => Ok(()),
            (Value::Bool(_), Type::Bool) => Ok(()),
            (Value::String(_), Type::String) => Ok(()),
            (Value::List(items), Type::List(elem_type)) => {
                for item in items {
                    item.check_type(elem_type, definitions)?;
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
                    item.check_type(ty, definitions)?;
                }
                Ok(())
            }
            (Value::Set(items), Type::Set(elem_type)) => {
                for item in items {
                    item.check_type(elem_type, definitions)?;
                }
                Ok(())
            }
            (Value::Dict(entries), Type::Dict(key_type, val_type)) => {
                for (k, v) in entries {
                    k.check_type(key_type, definitions)?;
                    v.check_type(val_type, definitions)?;
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
                    definitions.resolve_struct_fields(module, name, type_fields, type_args);
                check_value_data(data, &resolved_fields, name, definitions)
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
                    definitions.resolve_enum_variants(module, enum_name, variants, type_args);
                let variant_type = resolved_variants
                    .iter()
                    .find(|(vname, _)| vname == variant_name)
                    .map(|(_, vt)| vt)
                    .ok_or_else(|| Error::UnknownVariant(variant_name.clone()))?;
                let variant_fields = variant_type_to_fields(variant_type);
                let context = format!("{}::{}", enum_name, variant_name);
                check_value_data(data, &variant_fields, &context, definitions)
            }
            (Value::Task(inner), Type::Task(elem_type)) => inner.check_type(elem_type, definitions),
            (Value::Bytes(_), Type::Bytes) => Ok(()),
            (Value::Json(_), Type::Enum { name, module, .. })
                if name == "JSON"
                    && module.segments().first().map(|s| s.as_str()) == Some("std")
                    && module.segments().last().map(|s| s.as_str()) == Some("json") =>
            {
                Ok(())
            }
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

    /// Interpret this value as a termination result.
    ///
    /// - `()` → Ok
    /// - `Result::Ok(...)` → Ok
    /// - `Result::Err(e)` → Err (failed)
    /// - `Task(inner)` → recurse
    /// - anything else → unexpected return
    pub fn termination(&self) -> Result<(), TerminationError> {
        match self {
            Value::Tuple(elems) if elems.is_empty() => Ok(()),
            Value::EnumVariant {
                enum_name,
                variant_name,
                data: ValueData::Tuple(_),
                ..
            } if enum_name == "Result" && variant_name == "Ok" => Ok(()),
            Value::EnumVariant {
                enum_name,
                variant_name,
                data: ValueData::Tuple(values),
                ..
            } if enum_name == "Result" && variant_name == "Err" => {
                let msg = values
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "failed".to_string());
                Err(TerminationError::Failed(msg))
            }
            Value::Task(inner) => inner.termination(),
            _ => Err(TerminationError::UnexpectedReturn(format!("{self}"))),
        }
    }

    /// Convert a Job enum variant value into (function_path, args).
    ///
    /// Looks up the variant name in the provided jobs mapping to resolve
    /// the qualified path of the job function.
    pub fn as_job(&self, jobs: &[(QualifiedPath, String)]) -> Result<Job, Error> {
        match self {
            Value::EnumVariant {
                enum_name,
                variant_name,
                module,
                data,
            } if enum_name == "Job" && module == &QualifiedPath::root() => {
                let path = jobs
                    .iter()
                    .find(|(_, name)| name == variant_name)
                    .map(|(p, _)| p.clone())
                    .ok_or_else(|| Error::UnknownVariant(variant_name.clone()))?;
                let args = match data {
                    ValueData::Unit => vec![],
                    ValueData::Tuple(values) => values.clone(),
                    ValueData::Struct(_) => {
                        return Err(Error::UnsupportedConversion(
                            "Job variants cannot have struct data".into(),
                        ));
                    }
                };
                Ok(Job { path, args })
            }
            _ => Err(Error::TypeMismatch {
                from: self.type_name().to_string(),
                to: "Job".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let val = Value::Set(vec![Value::Int(1)]);
        assert!(
            val.check_type(&Type::Set(Box::new(Type::Int)), &lookup)
                .is_ok()
        );
    }

    #[test]
    fn check_type_dict_ok() {
        let lookup = empty_lookup();
        let val = Value::Dict(vec![(Value::String("a".into()), Value::Int(1))]);
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
        let module = QualifiedPath::root();
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
        let module = QualifiedPath::root();
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
        let module = QualifiedPath::root();
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
        let module = QualifiedPath::root();
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

    // ── termination tests ───────────────────────────────────────────

    #[test]
    fn termination_unit() {
        let value = Value::Tuple(vec![]);
        assert_eq!(value.termination(), Ok(()));
    }

    #[test]
    fn termination_result_ok() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert_eq!(value.termination(), Ok(()));
    }

    #[test]
    fn termination_result_err() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            data: ValueData::Tuple(vec![Value::String("something failed".to_string())]),
        };
        let err = value.termination().unwrap_err();
        assert!(matches!(err, TerminationError::Failed(_)));
        assert!(err.to_string().contains("something failed"));
    }

    #[test]
    fn termination_wrong_enum_name() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Option".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert!(matches!(
            value.termination(),
            Err(TerminationError::UnexpectedReturn(_))
        ));
    }

    #[test]
    fn termination_task_unit() {
        let value = Value::Task(Box::new(Value::Tuple(vec![])));
        assert_eq!(value.termination(), Ok(()));
    }

    #[test]
    fn termination_task_result_ok() {
        let value = Value::Task(Box::new(Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        }));
        assert_eq!(value.termination(), Ok(()));
    }

    #[test]
    fn termination_task_result_err() {
        let value = Value::Task(Box::new(Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            data: ValueData::Tuple(vec![Value::String("async failed".to_string())]),
        }));
        let err = value.termination().unwrap_err();
        assert!(err.to_string().contains("async failed"));
    }

    #[test]
    fn termination_unexpected_value() {
        let value = Value::Int(42);
        assert!(matches!(
            value.termination(),
            Err(TerminationError::UnexpectedReturn(_))
        ));
    }

    #[test]
    fn as_job_unit_variant() {
        let path = QualifiedPath::from("root::app::deploy");
        let jobs = vec![(path.clone(), "Deploy".to_string())];
        let value = Value::EnumVariant {
            enum_name: "Job".to_string(),
            variant_name: "Deploy".to_string(),
            module: QualifiedPath::root(),
            data: ValueData::Unit,
        };
        let job = value.as_job(&jobs).unwrap();
        assert_eq!(job.path, path);
        assert!(job.args.is_empty());
    }

    #[test]
    fn as_job_tuple_variant() {
        let path = QualifiedPath::from("root::app::send_email");
        let jobs = vec![(path.clone(), "SendEmail".to_string())];
        let value = Value::EnumVariant {
            enum_name: "Job".to_string(),
            variant_name: "SendEmail".to_string(),
            module: QualifiedPath::root(),
            data: ValueData::Tuple(vec![Value::String("hello@example.com".to_string())]),
        };
        let job = value.as_job(&jobs).unwrap();
        assert_eq!(job.path, path);
        assert_eq!(
            job.args,
            vec![Value::String("hello@example.com".to_string())]
        );
    }

    #[test]
    fn as_job_unknown_variant() {
        let jobs = vec![(
            QualifiedPath::from("root::app::deploy"),
            "Deploy".to_string(),
        )];
        let value = Value::EnumVariant {
            enum_name: "Job".to_string(),
            variant_name: "Unknown".to_string(),
            module: QualifiedPath::root(),
            data: ValueData::Unit,
        };
        assert!(matches!(
            value.as_job(&jobs),
            Err(Error::UnknownVariant(name)) if name == "Unknown"
        ));
    }

    #[test]
    fn as_job_not_enum() {
        let jobs = vec![];
        let value = Value::Int(42);
        assert!(matches!(
            value.as_job(&jobs),
            Err(Error::TypeMismatch { to, .. }) if to == "Job"
        ));
    }

    #[test]
    fn as_job_wrong_enum() {
        let jobs = vec![];
        let value = Value::EnumVariant {
            enum_name: "Option".to_string(),
            variant_name: "Some".to_string(),
            module: QualifiedPath::root(),
            data: ValueData::Tuple(vec![Value::Int(1)]),
        };
        assert!(matches!(
            value.as_job(&jobs),
            Err(Error::TypeMismatch { to, .. }) if to == "Job"
        ));
    }

    #[test]
    fn as_job_wrong_module() {
        let jobs = vec![];
        let value = Value::EnumVariant {
            enum_name: "Job".to_string(),
            variant_name: "Deploy".to_string(),
            module: QualifiedPath::from("root::other"),
            data: ValueData::Unit,
        };
        assert!(matches!(
            value.as_job(&jobs),
            Err(Error::TypeMismatch { to, .. }) if to == "Job"
        ));
    }
}
