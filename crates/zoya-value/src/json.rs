use std::collections::HashMap;

use serde::ser::{SerializeMap, SerializeSeq};
use zoya_ir::QualifiedPath;

use crate::{Value, ValueData};

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

/// Serde module for round-trip serialization of `HashSet<Value>` as `Vec<Value>`.
pub(crate) mod set_serde {
    use super::Value;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashSet;

    pub fn serialize<S: Serializer>(
        set: &HashSet<Value>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let vec: Vec<&Value> = set.iter().collect();
        vec.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<HashSet<Value>, D::Error> {
        let vec = Vec::<Value>::deserialize(deserializer)?;
        Ok(vec.into_iter().collect())
    }
}

/// Serde module for round-trip serialization of `HashMap<Value, Value>` as `Vec<(Value, Value)>`.
pub(crate) mod dict_serde {
    use super::Value;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S: Serializer>(
        map: &HashMap<Value, Value>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let vec: Vec<(&Value, &Value)> = map.iter().collect();
        vec.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<HashMap<Value, Value>, D::Error> {
        let vec = Vec::<(Value, Value)>::deserialize(deserializer)?;
        Ok(vec.into_iter().collect())
    }
}

/// Lossy JSON wrapper used by `to_json()` / `to_json_pretty()`.
///
/// Produces a simplified JSON representation intended for human consumption
/// and API output (e.g. `--json` flag). Not suitable for round-tripping.
struct SimpleJson<'a>(&'a Value);

/// Serialize struct/enum data using the lossy `SimpleJson` representation for nested values.
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
            let wrapped: Vec<SimpleJson> = values.iter().map(SimpleJson).collect();
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("type", type_name)?;
            map.serialize_entry("data", &wrapped)?;
            map.end()
        }
        ValueData::Struct(fields) => {
            let wrapped: HashMap<&String, SimpleJson> =
                fields.iter().map(|(k, v)| (k, SimpleJson(v))).collect();
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("type", type_name)?;
            map.serialize_entry("data", &wrapped)?;
            map.end()
        }
    }
}

impl serde::Serialize for SimpleJson<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Int(n) => serializer.serialize_i64(*n),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::BigInt(n) => serializer.serialize_str(&n.to_string()),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::String(s) => serializer.serialize_str(s),
            Value::Tuple(values) | Value::List(values) => {
                let wrapped: Vec<SimpleJson> = values.iter().map(SimpleJson).collect();
                wrapped.serialize(serializer)
            }
            Value::Set(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for v in values {
                    seq.serialize_element(&SimpleJson(v))?;
                }
                seq.end()
            }
            Value::Dict(entries) => {
                let mut seq = serializer.serialize_seq(Some(entries.len()))?;
                for (k, v) in entries {
                    seq.serialize_element(&(SimpleJson(k), SimpleJson(v)))?;
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
            Value::Task(inner) => SimpleJson(inner).serialize(serializer),
            Value::Bytes(data) => {
                let mut seq = serializer.serialize_seq(Some(data.len()))?;
                for byte in data {
                    seq.serialize_element(byte)?;
                }
                seq.end()
            }
        }
    }
}

impl Value {
    pub fn to_json(&self) -> String {
        serde_json::to_string(&SimpleJson(self)).unwrap()
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&SimpleJson(self)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use zoya_ir::QualifiedPath;

    use crate::{Value, ValueData};

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
            module: QualifiedPath::root(),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"Foo"}"#);
    }

    #[test]
    fn serialize_struct_tuple() {
        let val = Value::Struct {
            name: "Point".into(),
            module: QualifiedPath::root(),
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
            module: QualifiedPath::root(),
            data: ValueData::Struct(fields),
        };
        assert_eq!(val.to_json(), r#"{"type":"Point","data":{"x":10}}"#);
    }

    #[test]
    fn serialize_struct_in_module() {
        let val = Value::Struct {
            name: "Foo".into(),
            module: QualifiedPath::from("root::mymod"),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"mymod::Foo"}"#);
    }

    #[test]
    fn serialize_struct_in_nested_module() {
        let val = Value::Struct {
            name: "Bar".into(),
            module: QualifiedPath::from("root::a::b"),
            data: ValueData::Tuple(vec![Value::Int(1)]),
        };
        assert_eq!(val.to_json(), r#"{"type":"a::b::Bar","data":[1]}"#);
    }

    #[test]
    fn serialize_enum_unit() {
        let val = Value::EnumVariant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
            module: QualifiedPath::root(),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"Color::Red"}"#);
    }

    #[test]
    fn serialize_enum_tuple() {
        let val = Value::EnumVariant {
            enum_name: "Shape".into(),
            variant_name: "Circle".into(),
            module: QualifiedPath::root(),
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
            module: QualifiedPath::root(),
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
            module: QualifiedPath::from("root::graphics"),
            data: ValueData::Unit,
        };
        assert_eq!(val.to_json(), r#"{"type":"graphics::Color::Red"}"#);
    }

    #[test]
    fn serialize_nested() {
        let inner = Value::Struct {
            name: "Unit".into(),
            module: QualifiedPath::root(),
            data: ValueData::Unit,
        };
        let val = Value::List(vec![inner, Value::Int(42)]);
        assert_eq!(val.to_json(), r#"[{"type":"Unit"},42]"#);
    }
}
