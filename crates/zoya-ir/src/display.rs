use std::collections::HashMap;
use std::fmt::Write;

use crate::{Type, TypeVarId};

/// Converts an index to a variable name: 0 -> "a", 25 -> "z", 26 -> "a1", 51 -> "z1", etc.
fn index_to_name(index: usize) -> String {
    let letter = (b'a' + (index % 26) as u8) as char;
    let suffix = index / 26;
    if suffix == 0 {
        letter.to_string()
    } else {
        format!("{}{}", letter, suffix)
    }
}

/// Maps TypeVarIds to readable names (a, b, c, ...) in encounter order.
struct TypeVarNamer {
    names: HashMap<TypeVarId, String>,
    next_index: usize,
}

impl TypeVarNamer {
    fn new() -> Self {
        TypeVarNamer {
            names: HashMap::new(),
            next_index: 0,
        }
    }

    /// Get or assign a name for a type variable.
    fn name(&mut self, id: TypeVarId) -> &str {
        self.names.entry(id).or_insert_with(|| {
            let name = index_to_name(self.next_index);
            self.next_index += 1;
            name
        })
    }

    /// Collect all type variables from a type in encounter order.
    fn collect_vars(&mut self, ty: &Type) {
        match ty {
            Type::Var(id) => {
                self.name(*id);
            }
            Type::List(elem) => self.collect_vars(elem),
            Type::Set(elem) => self.collect_vars(elem),
            Type::Task(elem) => self.collect_vars(elem),
            Type::Dict(key, val) => {
                self.collect_vars(key);
                self.collect_vars(val);
            }
            Type::Tuple(elems) => {
                for elem in elems {
                    self.collect_vars(elem);
                }
            }
            Type::Function { params, ret } => {
                for param in params {
                    self.collect_vars(param);
                }
                self.collect_vars(ret);
            }
            Type::Struct { type_args, .. } | Type::Enum { type_args, .. } => {
                for arg in type_args {
                    self.collect_vars(arg);
                }
            }
            Type::Int | Type::BigInt | Type::Float | Type::Bool | Type::String | Type::Bytes => {}
        }
    }

    /// Write a type using assigned names into the buffer.
    fn write(&self, buf: &mut String, ty: &Type) {
        match ty {
            Type::Int => buf.push_str("Int"),
            Type::BigInt => buf.push_str("BigInt"),
            Type::Float => buf.push_str("Float"),
            Type::Bool => buf.push_str("Bool"),
            Type::String => buf.push_str("String"),
            Type::Bytes => buf.push_str("Bytes"),
            Type::List(elem) => {
                buf.push_str("List<");
                self.write(buf, elem);
                buf.push('>');
            }
            Type::Set(elem) => {
                buf.push_str("Set<");
                self.write(buf, elem);
                buf.push('>');
            }
            Type::Task(elem) => {
                buf.push_str("Task<");
                self.write(buf, elem);
                buf.push('>');
            }
            Type::Dict(key, val) => {
                buf.push_str("Dict<");
                self.write(buf, key);
                buf.push_str(", ");
                self.write(buf, val);
                buf.push('>');
            }
            Type::Tuple(elems) => {
                buf.push('(');
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        buf.push_str(", ");
                    }
                    self.write(buf, e);
                }
                buf.push(')');
            }
            Type::Var(id) => match self.names.get(id) {
                Some(name) => buf.push_str(name),
                None => write!(buf, "?{}", id.0).unwrap(),
            },
            Type::Function { params, ret } => {
                if params.is_empty() {
                    buf.push_str("() -> ");
                    self.write(buf, ret);
                } else if params.len() == 1 {
                    // Wrap function types in parentheses when they are parameters
                    if matches!(params[0], Type::Function { .. }) {
                        buf.push('(');
                        self.write(buf, &params[0]);
                        buf.push(')');
                    } else {
                        self.write(buf, &params[0]);
                    }
                    buf.push_str(" -> ");
                    self.write(buf, ret);
                } else {
                    buf.push('(');
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            buf.push_str(", ");
                        }
                        self.write(buf, p);
                    }
                    buf.push_str(") -> ");
                    self.write(buf, ret);
                }
            }
            Type::Struct {
                name, type_args, ..
            }
            | Type::Enum {
                name, type_args, ..
            } => {
                buf.push_str(name);
                if !type_args.is_empty() {
                    buf.push('<');
                    for (i, t) in type_args.iter().enumerate() {
                        if i > 0 {
                            buf.push_str(", ");
                        }
                        self.write(buf, t);
                    }
                    buf.push('>');
                }
            }
        }
    }
}

/// Format a type with normalized variable names (a, b, c, ...).
///
/// Type variables are named in encounter order, so the same type structure
/// will always produce the same output regardless of internal variable IDs.
///
/// # Examples
///
/// ```
/// use zoya_ir::{Type, TypeVarId, pretty_type};
///
/// // A single type variable becomes "a"
/// let ty = Type::Var(TypeVarId(42));
/// assert_eq!(pretty_type(&ty), "a");
///
/// // Function with same type variable for both params becomes "(a, a) -> a"
/// let id = TypeVarId(100);
/// let ty = Type::Function {
///     params: vec![Type::Var(id), Type::Var(id)],
///     ret: Box::new(Type::Var(id)),
/// };
/// assert_eq!(pretty_type(&ty), "(a, a) -> a");
/// ```
pub fn pretty_type(ty: &Type) -> String {
    let mut namer = TypeVarNamer::new();
    namer.collect_vars(ty);
    let mut buf = String::new();
    namer.write(&mut buf, ty);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_package::QualifiedPath;

    #[test]
    fn test_index_to_name() {
        assert_eq!(index_to_name(0), "a");
        assert_eq!(index_to_name(1), "b");
        assert_eq!(index_to_name(25), "z");
        assert_eq!(index_to_name(26), "a1");
        assert_eq!(index_to_name(27), "b1");
        assert_eq!(index_to_name(51), "z1");
        assert_eq!(index_to_name(52), "a2");
    }

    #[test]
    fn test_pretty_type_primitives() {
        assert_eq!(pretty_type(&Type::Int), "Int");
        assert_eq!(pretty_type(&Type::BigInt), "BigInt");
        assert_eq!(pretty_type(&Type::Float), "Float");
        assert_eq!(pretty_type(&Type::Bool), "Bool");
        assert_eq!(pretty_type(&Type::String), "String");
        assert_eq!(pretty_type(&Type::Bytes), "Bytes");
    }

    #[test]
    fn test_pretty_type_single_var() {
        let ty = Type::Var(TypeVarId(42));
        assert_eq!(pretty_type(&ty), "a");
    }

    #[test]
    fn test_pretty_type_same_var_multiple_times() {
        let id = TypeVarId(100);
        let ty = Type::Function {
            params: vec![Type::Var(id), Type::Var(id)],
            ret: Box::new(Type::Var(id)),
        };
        assert_eq!(pretty_type(&ty), "(a, a) -> a");
    }

    #[test]
    fn test_pretty_type_multiple_vars() {
        let id1 = TypeVarId(5);
        let id2 = TypeVarId(10);
        let ty = Type::Function {
            params: vec![Type::Var(id1)],
            ret: Box::new(Type::Var(id2)),
        };
        assert_eq!(pretty_type(&ty), "a -> b");
    }

    #[test]
    fn test_pretty_type_list_with_var() {
        let ty = Type::List(Box::new(Type::Var(TypeVarId(0))));
        assert_eq!(pretty_type(&ty), "List<a>");
    }

    #[test]
    fn test_pretty_type_tuple_with_vars() {
        let ty = Type::Tuple(vec![
            Type::Var(TypeVarId(1)),
            Type::Var(TypeVarId(2)),
            Type::Var(TypeVarId(1)),
        ]);
        assert_eq!(pretty_type(&ty), "(a, b, a)");
    }

    #[test]
    fn test_pretty_type_nested_function() {
        let id = TypeVarId(0);
        // (a -> a) -> a
        let inner = Type::Function {
            params: vec![Type::Var(id)],
            ret: Box::new(Type::Var(id)),
        };
        let ty = Type::Function {
            params: vec![inner],
            ret: Box::new(Type::Var(id)),
        };
        assert_eq!(pretty_type(&ty), "(a -> a) -> a");
    }

    #[test]
    fn test_pretty_type_struct_with_vars() {
        let ty = Type::Struct {
            module: QualifiedPath::root(),
            name: "Pair".to_string(),
            type_args: vec![Type::Var(TypeVarId(5)), Type::Var(TypeVarId(10))],
            fields: vec![],
        };
        assert_eq!(pretty_type(&ty), "Pair<a, b>");
    }

    #[test]
    fn test_pretty_type_enum_with_vars() {
        let ty = Type::Enum {
            module: QualifiedPath::root(),
            name: "Result".to_string(),
            type_args: vec![Type::Var(TypeVarId(3)), Type::Var(TypeVarId(7))],
            variants: vec![],
        };
        assert_eq!(pretty_type(&ty), "Result<a, b>");
    }

    #[test]
    fn test_pretty_type_complex() {
        // (a, List<b>) -> Result<b, a>
        let a = TypeVarId(100);
        let b = TypeVarId(200);
        let ty = Type::Function {
            params: vec![Type::Var(a), Type::List(Box::new(Type::Var(b)))],
            ret: Box::new(Type::Enum {
                module: QualifiedPath::root(),
                name: "Result".to_string(),
                type_args: vec![Type::Var(b), Type::Var(a)],
                variants: vec![],
            }),
        };
        assert_eq!(pretty_type(&ty), "(a, List<b>) -> Result<b, a>");
    }

    #[test]
    fn test_pretty_type_empty_tuple() {
        let ty = Type::Tuple(vec![]);
        assert_eq!(pretty_type(&ty), "()");
    }

    #[test]
    fn test_pretty_type_no_param_function() {
        let ty = Type::Function {
            params: vec![],
            ret: Box::new(Type::Int),
        };
        assert_eq!(pretty_type(&ty), "() -> Int");
    }

    #[test]
    fn test_pretty_type_single_param_function() {
        let ty = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        assert_eq!(pretty_type(&ty), "Int -> Bool");
    }
}
