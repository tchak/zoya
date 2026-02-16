//! Type unification for Hindley-Milner style type inference.

use std::collections::{HashMap, HashSet};

// Re-export substitute functions from zoya_ir so existing callers can still use them.
pub use zoya_ir::{substitute_type_vars, substitute_variant_type_vars};

use zoya_ir::{EnumVariantType, Type, TypeError, TypeScheme, TypeVarId};

/// Unification context that tracks type variable bindings.
#[derive(Debug, Clone)]
pub struct UnifyCtx {
    /// Maps type variables to their bound types (Union-Find style)
    substitutions: HashMap<TypeVarId, Type>,
    /// Counter for generating fresh type variables
    next_var: usize,
}

impl UnifyCtx {
    /// Create a new empty unification context with the counter starting at 0.
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            substitutions: HashMap::new(),
            next_var: 0,
        }
    }

    /// Create a new unification context with the counter starting after
    /// the given value. Used to avoid TypeVarId collisions with dependency
    /// definitions that already use TypeVarIds in the range `0..start`.
    pub fn with_start(start: usize) -> Self {
        Self {
            substitutions: HashMap::new(),
            next_var: start,
        }
    }

    /// Clear all substitutions while preserving the variable counter.
    /// Used to isolate type inference between independent function body checks.
    pub fn clear_substitutions(&mut self) {
        self.substitutions.clear();
    }

    /// Create a fresh type variable.
    pub fn fresh_var(&mut self) -> Type {
        let id = TypeVarId(self.next_var);
        self.next_var += 1;
        Type::Var(id)
    }

    /// Resolve a type by following type variable bindings.
    /// Returns the most concrete type known for the given type.
    pub fn resolve(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(id) => {
                if let Some(bound) = self.substitutions.get(id) {
                    // Recursively resolve in case of chained bindings
                    self.resolve(bound)
                } else {
                    ty.clone()
                }
            }
            Type::List(elem) => Type::List(Box::new(self.resolve(elem))),
            Type::Set(elem) => Type::Set(Box::new(self.resolve(elem))),
            Type::Dict(key, val) => {
                Type::Dict(Box::new(self.resolve(key)), Box::new(self.resolve(val)))
            }
            Type::Tuple(elems) => Type::Tuple(elems.iter().map(|e| self.resolve(e)).collect()),
            Type::Function { params, ret } => Type::Function {
                params: params.iter().map(|p| self.resolve(p)).collect(),
                ret: Box::new(self.resolve(ret)),
            },
            Type::Struct {
                module,
                name,
                type_args,
                fields,
            } => Type::Struct {
                module: module.clone(),
                name: name.clone(),
                type_args: type_args.iter().map(|t| self.resolve(t)).collect(),
                fields: fields
                    .iter()
                    .map(|(n, t)| (n.clone(), self.resolve(t)))
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
                type_args: type_args.iter().map(|t| self.resolve(t)).collect(),
                variants: variants
                    .iter()
                    .map(|(n, vt)| (n.clone(), self.resolve_variant_type(vt)))
                    .collect(),
            },
            _ => ty.clone(),
        }
    }

    /// Resolve type variables within an enum variant type.
    fn resolve_variant_type(&self, vt: &EnumVariantType) -> EnumVariantType {
        match vt {
            EnumVariantType::Unit => EnumVariantType::Unit,
            EnumVariantType::Tuple(types) => {
                EnumVariantType::Tuple(types.iter().map(|t| self.resolve(t)).collect())
            }
            EnumVariantType::Struct(fields) => EnumVariantType::Struct(
                fields
                    .iter()
                    .map(|(n, t)| (n.clone(), self.resolve(t)))
                    .collect(),
            ),
        }
    }

    /// Check if a type variable occurs in a type (occurs check).
    /// This prevents infinite types like T = List<T>.
    fn occurs(&self, var_id: TypeVarId, ty: &Type) -> bool {
        let ty = self.resolve(ty);
        match ty {
            Type::Var(id) => id == var_id,
            Type::List(elem) => self.occurs(var_id, &elem),
            Type::Set(elem) => self.occurs(var_id, &elem),
            Type::Dict(key, val) => self.occurs(var_id, &key) || self.occurs(var_id, &val),
            Type::Tuple(elems) => elems.iter().any(|e| self.occurs(var_id, e)),
            Type::Function { params, ret } => {
                params.iter().any(|p| self.occurs(var_id, p)) || self.occurs(var_id, &ret)
            }
            Type::Struct {
                type_args, fields, ..
            } => {
                type_args.iter().any(|t| self.occurs(var_id, t))
                    || fields.iter().any(|(_, t)| self.occurs(var_id, t))
            }
            Type::Enum {
                type_args,
                variants,
                ..
            } => {
                type_args.iter().any(|t| self.occurs(var_id, t))
                    || variants
                        .iter()
                        .any(|(_, vt)| self.occurs_in_variant(var_id, vt))
            }
            _ => false,
        }
    }

    /// Check if a type variable occurs in an enum variant type.
    fn occurs_in_variant(&self, var_id: TypeVarId, vt: &EnumVariantType) -> bool {
        match vt {
            EnumVariantType::Unit => false,
            EnumVariantType::Tuple(types) => types.iter().any(|t| self.occurs(var_id, t)),
            EnumVariantType::Struct(fields) => fields.iter().any(|(_, t)| self.occurs(var_id, t)),
        }
    }

    /// Unify two types, adding bindings to make them equal.
    /// Returns an error if the types cannot be unified.
    pub fn unify(&mut self, t1: &Type, t2: &Type) -> Result<(), TypeError> {
        let t1 = self.resolve(t1);
        let t2 = self.resolve(t2);

        match (&t1, &t2) {
            // Same concrete types - always unify
            (Type::Int, Type::Int) => Ok(()),
            (Type::BigInt, Type::BigInt) => Ok(()),
            (Type::Float, Type::Float) => Ok(()),
            (Type::Bool, Type::Bool) => Ok(()),
            (Type::String, Type::String) => Ok(()),

            // List types - unify element types
            (Type::List(e1), Type::List(e2)) => self.unify(e1, e2),

            // Set types - unify element types
            (Type::Set(e1), Type::Set(e2)) => self.unify(e1, e2),

            // Dict types - unify key and value types
            (Type::Dict(k1, v1), Type::Dict(k2, v2)) => {
                self.unify(k1, k2)?;
                self.unify(v1, v2)
            }

            // Tuple types - unify element types pairwise
            (Type::Tuple(elems1), Type::Tuple(elems2)) => {
                if elems1.len() != elems2.len() {
                    return Err(TypeError {
                        message: format!(
                            "tuple length mismatch: {} vs {}",
                            elems1.len(),
                            elems2.len()
                        ),
                    });
                }
                for (e1, e2) in elems1.iter().zip(elems2.iter()) {
                    self.unify(e1, e2)?;
                }
                Ok(())
            }

            // Same type variable - already unified
            (Type::Var(id1), Type::Var(id2)) if id1 == id2 => Ok(()),

            // Type variable on left: bind it to the right type
            (Type::Var(id), other) => {
                if self.occurs(*id, other) {
                    return Err(TypeError {
                        message: format!("infinite type: {} = {}", id, other),
                    });
                }
                self.substitutions.insert(*id, other.clone());
                Ok(())
            }

            // Type variable on right: bind it to the left type
            (other, Type::Var(id)) => {
                if self.occurs(*id, other) {
                    return Err(TypeError {
                        message: format!("infinite type: {} = {}", id, other),
                    });
                }
                self.substitutions.insert(*id, other.clone());
                Ok(())
            }

            // Function types - unify parameter types and return types
            (
                Type::Function {
                    params: params1,
                    ret: ret1,
                },
                Type::Function {
                    params: params2,
                    ret: ret2,
                },
            ) => {
                if params1.len() != params2.len() {
                    return Err(TypeError {
                        message: format!(
                            "function arity mismatch: {} parameters vs {}",
                            params1.len(),
                            params2.len()
                        ),
                    });
                }
                for (p1, p2) in params1.iter().zip(params2.iter()) {
                    self.unify(p1, p2)?;
                }
                self.unify(ret1, ret2)
            }

            // Struct types - unify if same module+name and type args unify
            // Note: fields are derived from name + type_args, so we only unify type_args
            (
                Type::Struct {
                    module: mod1,
                    name: name1,
                    type_args: args1,
                    ..
                },
                Type::Struct {
                    module: mod2,
                    name: name2,
                    type_args: args2,
                    ..
                },
            ) => {
                if mod1 != mod2 || name1 != name2 {
                    return Err(TypeError {
                        message: format!("struct type mismatch: {} vs {}", name1, name2),
                    });
                }
                if args1.len() != args2.len() {
                    return Err(TypeError {
                        message: format!(
                            "struct {} type argument count mismatch: {} vs {}",
                            name1,
                            args1.len(),
                            args2.len()
                        ),
                    });
                }
                for (a1, a2) in args1.iter().zip(args2.iter()) {
                    self.unify(a1, a2)?;
                }
                Ok(())
            }

            // Enum types - unify if same module+name and type args unify
            // Note: variants are derived from name + type_args, so we only unify type_args
            (
                Type::Enum {
                    module: mod1,
                    name: name1,
                    type_args: args1,
                    ..
                },
                Type::Enum {
                    module: mod2,
                    name: name2,
                    type_args: args2,
                    ..
                },
            ) => {
                if mod1 != mod2 || name1 != name2 {
                    return Err(TypeError {
                        message: format!("enum type mismatch: {} vs {}", name1, name2),
                    });
                }
                if args1.len() != args2.len() {
                    return Err(TypeError {
                        message: format!(
                            "enum {} type argument count mismatch: {} vs {}",
                            name1,
                            args1.len(),
                            args2.len()
                        ),
                    });
                }
                for (a1, a2) in args1.iter().zip(args2.iter()) {
                    self.unify(a1, a2)?;
                }
                Ok(())
            }

            // Different concrete types - cannot unify
            _ => Err(TypeError {
                message: format!("type mismatch: {} vs {}", t1, t2),
            }),
        }
    }

    /// Collect all free (unbound) type variables in a type.
    pub fn free_vars(&self, ty: &Type) -> HashSet<TypeVarId> {
        let ty = self.resolve(ty);
        match ty {
            Type::Var(id) => {
                let mut set = HashSet::new();
                set.insert(id);
                set
            }
            Type::List(elem) => self.free_vars(&elem),
            Type::Set(elem) => self.free_vars(&elem),
            Type::Dict(key, val) => {
                let mut set = self.free_vars(&key);
                set.extend(self.free_vars(&val));
                set
            }
            Type::Tuple(elems) => elems.iter().flat_map(|e| self.free_vars(e)).collect(),
            Type::Function { params, ret } => {
                let mut set: HashSet<TypeVarId> =
                    params.iter().flat_map(|p| self.free_vars(p)).collect();
                set.extend(self.free_vars(&ret));
                set
            }
            Type::Struct {
                type_args, fields, ..
            } => {
                let mut set: HashSet<TypeVarId> =
                    type_args.iter().flat_map(|t| self.free_vars(t)).collect();
                set.extend(fields.iter().flat_map(|(_, t)| self.free_vars(t)));
                set
            }
            Type::Enum {
                type_args,
                variants,
                ..
            } => {
                let mut set: HashSet<TypeVarId> =
                    type_args.iter().flat_map(|t| self.free_vars(t)).collect();
                for (_, vt) in variants {
                    set.extend(self.free_vars_in_variant(&vt));
                }
                set
            }
            _ => HashSet::new(),
        }
    }

    /// Collect free type variables in an enum variant type.
    fn free_vars_in_variant(&self, vt: &EnumVariantType) -> HashSet<TypeVarId> {
        match vt {
            EnumVariantType::Unit => HashSet::new(),
            EnumVariantType::Tuple(types) => types.iter().flat_map(|t| self.free_vars(t)).collect(),
            EnumVariantType::Struct(fields) => {
                fields.iter().flat_map(|(_, t)| self.free_vars(t)).collect()
            }
        }
    }

    /// Generalize a type to a type scheme by quantifying over type variables
    /// that are free in the type but not in the given set of "fixed" variables.
    /// The fixed variables typically come from the outer environment.
    pub fn generalize(&self, ty: &Type, fixed_vars: &HashSet<TypeVarId>) -> TypeScheme {
        let ty = self.resolve(ty);
        let ty_vars = self.free_vars(&ty);
        let quantified: Vec<TypeVarId> = ty_vars.difference(fixed_vars).cloned().collect();
        TypeScheme { quantified, ty }
    }

    /// Instantiate a type scheme by replacing quantified variables with fresh ones.
    pub fn instantiate(&mut self, scheme: &TypeScheme) -> Type {
        if scheme.quantified.is_empty() {
            return scheme.ty.clone();
        }

        let mut mapping = HashMap::new();
        for &var_id in &scheme.quantified {
            mapping.insert(var_id, self.fresh_var());
        }

        substitute_type_vars(&scheme.ty, &mapping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ir::QualifiedPath;

    fn test_struct(name: &str, type_args: Vec<Type>, fields: Vec<(String, Type)>) -> Type {
        Type::Struct {
            module: QualifiedPath::root(),
            name: name.to_string(),
            type_args,
            fields,
        }
    }

    fn test_enum(
        name: &str,
        type_args: Vec<Type>,
        variants: Vec<(String, EnumVariantType)>,
    ) -> Type {
        Type::Enum {
            module: QualifiedPath::root(),
            name: name.to_string(),
            type_args,
            variants,
        }
    }

    #[test]
    fn test_fresh_var() {
        let mut ctx = UnifyCtx::new();
        let v1 = ctx.fresh_var();
        let v2 = ctx.fresh_var();
        assert_ne!(v1, v2);
        assert!(matches!(v1, Type::Var(TypeVarId(0))));
        assert!(matches!(v2, Type::Var(TypeVarId(1))));
    }

    #[test]
    fn test_unify_same_concrete() {
        let mut ctx = UnifyCtx::new();
        assert!(ctx.unify(&Type::Int, &Type::Int).is_ok());
        assert!(ctx.unify(&Type::Float, &Type::Float).is_ok());
        assert!(ctx.unify(&Type::Bool, &Type::Bool).is_ok());
        assert!(ctx.unify(&Type::String, &Type::String).is_ok());
    }

    #[test]
    fn test_unify_different_concrete() {
        let mut ctx = UnifyCtx::new();
        let result = ctx.unify(&Type::Int, &Type::Float);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_unify_var_with_concrete() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Unify type variable with Int
        assert!(ctx.unify(&var, &Type::Int).is_ok());

        // The variable should now resolve to Int
        assert_eq!(ctx.resolve(&var), Type::Int);
    }

    #[test]
    fn test_unify_concrete_with_var() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Unify Int with type variable (reversed order)
        assert!(ctx.unify(&Type::Int, &var).is_ok());

        // The variable should now resolve to Int
        assert_eq!(ctx.resolve(&var), Type::Int);
    }

    #[test]
    fn test_unify_two_vars() {
        let mut ctx = UnifyCtx::new();
        let v1 = ctx.fresh_var();
        let v2 = ctx.fresh_var();

        // Unify two type variables
        assert!(ctx.unify(&v1, &v2).is_ok());

        // Now bind one to a concrete type
        assert!(ctx.unify(&v1, &Type::Int).is_ok());

        // Both should resolve to Int
        assert_eq!(ctx.resolve(&v1), Type::Int);
        assert_eq!(ctx.resolve(&v2), Type::Int);
    }

    #[test]
    fn test_unify_var_already_bound() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Bind to Int
        assert!(ctx.unify(&var, &Type::Int).is_ok());

        // Unifying with same type should succeed
        assert!(ctx.unify(&var, &Type::Int).is_ok());

        // Unifying with different type should fail
        let result = ctx.unify(&var, &Type::Float);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_unbound() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Unbound variable resolves to itself
        assert_eq!(ctx.resolve(&var), var);
    }

    #[test]
    fn test_resolve_chain() {
        let mut ctx = UnifyCtx::new();
        let v1 = ctx.fresh_var();
        let v2 = ctx.fresh_var();
        let v3 = ctx.fresh_var();

        // Create chain: v1 -> v2 -> v3 -> Int
        ctx.unify(&v1, &v2).unwrap();
        ctx.unify(&v2, &v3).unwrap();
        ctx.unify(&v3, &Type::Int).unwrap();

        // All should resolve to Int
        assert_eq!(ctx.resolve(&v1), Type::Int);
        assert_eq!(ctx.resolve(&v2), Type::Int);
        assert_eq!(ctx.resolve(&v3), Type::Int);
    }

    #[test]
    fn test_unify_list_same_element() {
        let mut ctx = UnifyCtx::new();
        let list1 = Type::List(Box::new(Type::Int));
        let list2 = Type::List(Box::new(Type::Int));
        assert!(ctx.unify(&list1, &list2).is_ok());
    }

    #[test]
    fn test_unify_list_different_element() {
        let mut ctx = UnifyCtx::new();
        let list1 = Type::List(Box::new(Type::Int));
        let list2 = Type::List(Box::new(Type::String));
        let result = ctx.unify(&list1, &list2);
        assert!(result.is_err());
    }

    #[test]
    fn test_unify_list_with_var_element() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let list1 = Type::List(Box::new(var.clone()));
        let list2 = Type::List(Box::new(Type::Int));

        assert!(ctx.unify(&list1, &list2).is_ok());
        assert_eq!(ctx.resolve(&var), Type::Int);
        assert_eq!(ctx.resolve(&list1), Type::List(Box::new(Type::Int)));
    }

    #[test]
    fn test_unify_list_occurs_check() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let list = Type::List(Box::new(var.clone()));

        // T = List<T> should fail (infinite type)
        let result = ctx.unify(&var, &list);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("infinite type"));
    }

    #[test]
    fn test_unify_function_same() {
        let mut ctx = UnifyCtx::new();
        let f1 = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        let f2 = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        assert!(ctx.unify(&f1, &f2).is_ok());
    }

    #[test]
    fn test_unify_function_different_return() {
        let mut ctx = UnifyCtx::new();
        let f1 = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        let f2 = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::String),
        };
        assert!(ctx.unify(&f1, &f2).is_err());
    }

    #[test]
    fn test_unify_function_different_arity() {
        let mut ctx = UnifyCtx::new();
        let f1 = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        let f2 = Type::Function {
            params: vec![Type::Int, Type::String],
            ret: Box::new(Type::Bool),
        };
        let result = ctx.unify(&f1, &f2);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("arity"));
    }

    #[test]
    fn test_unify_function_with_var() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let f = Type::Function {
            params: vec![var.clone()],
            ret: Box::new(Type::Bool),
        };
        let expected = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        assert!(ctx.unify(&f, &expected).is_ok());
        assert_eq!(ctx.resolve(&var), Type::Int);
    }

    #[test]
    fn test_free_vars_concrete() {
        let ctx = UnifyCtx::new();
        assert!(ctx.free_vars(&Type::Int).is_empty());
        assert!(ctx.free_vars(&Type::String).is_empty());
    }

    #[test]
    fn test_free_vars_var() {
        let mut ctx = UnifyCtx::new();
        let v = ctx.fresh_var();
        let fv = ctx.free_vars(&v);
        assert_eq!(fv.len(), 1);
        if let Type::Var(id) = v {
            assert!(fv.contains(&id));
        }
    }

    #[test]
    fn test_free_vars_function() {
        let mut ctx = UnifyCtx::new();
        let v1 = ctx.fresh_var();
        let v2 = ctx.fresh_var();
        let f = Type::Function {
            params: vec![v1.clone()],
            ret: Box::new(v2.clone()),
        };
        let fv = ctx.free_vars(&f);
        assert_eq!(fv.len(), 2);
    }

    #[test]
    fn test_generalize_no_free_vars() {
        let ctx = UnifyCtx::new();
        let ty = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        let scheme = ctx.generalize(&ty, &HashSet::new());
        assert!(scheme.quantified.is_empty());
    }

    #[test]
    fn test_generalize_with_free_vars() {
        let mut ctx = UnifyCtx::new();
        let v = ctx.fresh_var();
        let ty = Type::Function {
            params: vec![v.clone()],
            ret: Box::new(v.clone()),
        };
        let scheme = ctx.generalize(&ty, &HashSet::new());
        assert_eq!(scheme.quantified.len(), 1);
    }

    #[test]
    fn test_generalize_with_fixed_vars() {
        let mut ctx = UnifyCtx::new();
        let v = ctx.fresh_var();
        let ty = Type::Function {
            params: vec![v.clone()],
            ret: Box::new(v.clone()),
        };
        // If v is in fixed_vars, it shouldn't be quantified
        let mut fixed = HashSet::new();
        if let Type::Var(id) = v {
            fixed.insert(id);
        }
        let scheme = ctx.generalize(&ty, &fixed);
        assert!(scheme.quantified.is_empty());
    }

    #[test]
    fn test_instantiate_mono() {
        let mut ctx = UnifyCtx::new();
        let ty = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        let scheme = TypeScheme {
            quantified: vec![],
            ty: ty.clone(),
        };
        let instantiated = ctx.instantiate(&scheme);
        assert_eq!(instantiated, ty);
    }

    #[test]
    fn test_instantiate_poly() {
        let mut ctx = UnifyCtx::new();
        let v = ctx.fresh_var();
        let id = if let Type::Var(id) = v { id } else { panic!() };

        let ty = Type::Function {
            params: vec![Type::Var(id)],
            ret: Box::new(Type::Var(id)),
        };
        let scheme = TypeScheme {
            quantified: vec![id],
            ty,
        };

        let inst1 = ctx.instantiate(&scheme);
        let inst2 = ctx.instantiate(&scheme);

        // Both should be function types
        assert!(matches!(inst1, Type::Function { .. }));
        assert!(matches!(inst2, Type::Function { .. }));

        // The fresh vars should be different in each instantiation
        if let (Type::Function { params: p1, .. }, Type::Function { params: p2, .. }) =
            (&inst1, &inst2)
        {
            assert_ne!(p1[0], p2[0]);
        }
    }

    // ==================== Tuple Unification Tests ====================

    #[test]
    fn test_unify_tuple_same_types() {
        let mut ctx = UnifyCtx::new();
        let t1 = Type::Tuple(vec![Type::Int, Type::Bool]);
        let t2 = Type::Tuple(vec![Type::Int, Type::Bool]);
        assert!(ctx.unify(&t1, &t2).is_ok());
    }

    #[test]
    fn test_unify_tuple_different_lengths() {
        let mut ctx = UnifyCtx::new();
        let t1 = Type::Tuple(vec![Type::Int, Type::Bool]);
        let t2 = Type::Tuple(vec![Type::Int]);
        let result = ctx.unify(&t1, &t2);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("tuple length mismatch")
        );
    }

    #[test]
    fn test_unify_tuple_different_elements() {
        let mut ctx = UnifyCtx::new();
        let t1 = Type::Tuple(vec![Type::Int, Type::Bool]);
        let t2 = Type::Tuple(vec![Type::Int, Type::String]);
        assert!(ctx.unify(&t1, &t2).is_err());
    }

    #[test]
    fn test_unify_tuple_with_var() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let t1 = Type::Tuple(vec![var.clone(), Type::Bool]);
        let t2 = Type::Tuple(vec![Type::Int, Type::Bool]);
        assert!(ctx.unify(&t1, &t2).is_ok());
        assert_eq!(ctx.resolve(&var), Type::Int);
    }

    #[test]
    fn test_unify_tuple_empty() {
        let mut ctx = UnifyCtx::new();
        let t1 = Type::Tuple(vec![]);
        let t2 = Type::Tuple(vec![]);
        assert!(ctx.unify(&t1, &t2).is_ok());
    }

    #[test]
    fn test_unify_tuple_nested() {
        let mut ctx = UnifyCtx::new();
        let inner = Type::Tuple(vec![Type::Int, Type::Bool]);
        let t1 = Type::Tuple(vec![inner.clone(), Type::String]);
        let t2 = Type::Tuple(vec![inner, Type::String]);
        assert!(ctx.unify(&t1, &t2).is_ok());
    }

    // ==================== Struct Unification Tests ====================

    #[test]
    fn test_unify_struct_same_name() {
        let mut ctx = UnifyCtx::new();
        let s1 = test_struct(
            "Point",
            vec![],
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        );
        let s2 = test_struct(
            "Point",
            vec![],
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        );
        assert!(ctx.unify(&s1, &s2).is_ok());
    }

    #[test]
    fn test_unify_struct_different_names() {
        let mut ctx = UnifyCtx::new();
        let s1 = test_struct("Point", vec![], vec![]);
        let s2 = test_struct("Vec", vec![], vec![]);
        let result = ctx.unify(&s1, &s2);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("struct type mismatch"));
    }

    #[test]
    fn test_unify_struct_with_type_args() {
        let mut ctx = UnifyCtx::new();
        let s1 = test_struct(
            "Pair",
            vec![Type::Int, Type::Bool],
            vec![
                ("first".to_string(), Type::Int),
                ("second".to_string(), Type::Bool),
            ],
        );
        let s2 = test_struct(
            "Pair",
            vec![Type::Int, Type::Bool],
            vec![
                ("first".to_string(), Type::Int),
                ("second".to_string(), Type::Bool),
            ],
        );
        assert!(ctx.unify(&s1, &s2).is_ok());
    }

    #[test]
    fn test_unify_struct_different_type_args() {
        let mut ctx = UnifyCtx::new();
        let s1 = test_struct("Pair", vec![Type::Int, Type::Bool], vec![]);
        let s2 = test_struct("Pair", vec![Type::Int, Type::String], vec![]);
        assert!(ctx.unify(&s1, &s2).is_err());
    }

    #[test]
    fn test_unify_struct_with_var_type_args() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let s1 = test_struct("Pair", vec![var.clone(), Type::Bool], vec![]);
        let s2 = test_struct("Pair", vec![Type::Int, Type::Bool], vec![]);
        assert!(ctx.unify(&s1, &s2).is_ok());
        assert_eq!(ctx.resolve(&var), Type::Int);
    }

    #[test]
    fn test_unify_struct_type_arg_count_mismatch() {
        let mut ctx = UnifyCtx::new();
        let s1 = test_struct("Pair", vec![Type::Int], vec![]);
        let s2 = test_struct("Pair", vec![Type::Int, Type::Bool], vec![]);
        let result = ctx.unify(&s1, &s2);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("type argument count mismatch")
        );
    }

    // ==================== Enum Unification Tests ====================

    #[test]
    fn test_unify_enum_same_name() {
        let mut ctx = UnifyCtx::new();
        let e1 = test_enum(
            "Color",
            vec![],
            vec![
                ("Red".to_string(), EnumVariantType::Unit),
                ("Green".to_string(), EnumVariantType::Unit),
            ],
        );
        let e2 = test_enum(
            "Color",
            vec![],
            vec![
                ("Red".to_string(), EnumVariantType::Unit),
                ("Green".to_string(), EnumVariantType::Unit),
            ],
        );
        assert!(ctx.unify(&e1, &e2).is_ok());
    }

    #[test]
    fn test_unify_enum_different_names() {
        let mut ctx = UnifyCtx::new();
        let e1 = test_enum("Color", vec![], vec![]);
        let e2 = test_enum("Direction", vec![], vec![]);
        let result = ctx.unify(&e1, &e2);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("enum type mismatch"));
    }

    #[test]
    fn test_unify_enum_with_type_args() {
        let mut ctx = UnifyCtx::new();
        let e1 = test_enum(
            "Option",
            vec![Type::Int],
            vec![
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
                ("None".to_string(), EnumVariantType::Unit),
            ],
        );
        let e2 = test_enum(
            "Option",
            vec![Type::Int],
            vec![
                ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
                ("None".to_string(), EnumVariantType::Unit),
            ],
        );
        assert!(ctx.unify(&e1, &e2).is_ok());
    }

    #[test]
    fn test_unify_enum_different_type_args() {
        let mut ctx = UnifyCtx::new();
        let e1 = test_enum("Option", vec![Type::Int], vec![]);
        let e2 = test_enum("Option", vec![Type::String], vec![]);
        assert!(ctx.unify(&e1, &e2).is_err());
    }

    #[test]
    fn test_unify_enum_with_var_type_args() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let e1 = test_enum("Option", vec![var.clone()], vec![]);
        let e2 = test_enum("Option", vec![Type::Int], vec![]);
        assert!(ctx.unify(&e1, &e2).is_ok());
        assert_eq!(ctx.resolve(&var), Type::Int);
    }

    #[test]
    fn test_unify_enum_type_arg_count_mismatch() {
        let mut ctx = UnifyCtx::new();
        let e1 = test_enum("Result", vec![Type::Int], vec![]);
        let e2 = test_enum("Result", vec![Type::Int, Type::String], vec![]);
        let result = ctx.unify(&e1, &e2);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("type argument count mismatch")
        );
    }

    // ==================== Additional Occurs Check Tests ====================

    #[test]
    fn test_unify_tuple_occurs_check() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let tuple = Type::Tuple(vec![var.clone(), Type::Int]);
        // T = (T, Int) should fail (infinite type)
        let result = ctx.unify(&var, &tuple);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("infinite type"));
    }

    #[test]
    fn test_unify_function_occurs_check() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let func = Type::Function {
            params: vec![var.clone()],
            ret: Box::new(Type::Int),
        };
        // T = T -> Int should fail (infinite type)
        let result = ctx.unify(&var, &func);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("infinite type"));
    }

    #[test]
    fn test_unify_struct_occurs_check() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let s = test_struct(
            "Box",
            vec![var.clone()],
            vec![("value".to_string(), var.clone())],
        );
        // T = Box<T> should fail (infinite type)
        let result = ctx.unify(&var, &s);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("infinite type"));
    }

    #[test]
    fn test_unify_enum_occurs_check() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let e = test_enum(
            "Option",
            vec![var.clone()],
            vec![(
                "Some".to_string(),
                EnumVariantType::Tuple(vec![var.clone()]),
            )],
        );
        // T = Option<T> should fail (infinite type)
        let result = ctx.unify(&var, &e);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("infinite type"));
    }

    // ==================== Free Variables Tests ====================

    #[test]
    fn test_free_vars_list() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let list = Type::List(Box::new(var.clone()));
        let fv = ctx.free_vars(&list);
        assert_eq!(fv.len(), 1);
        if let Type::Var(id) = var {
            assert!(fv.contains(&id));
        }
    }

    #[test]
    fn test_free_vars_tuple() {
        let mut ctx = UnifyCtx::new();
        let v1 = ctx.fresh_var();
        let v2 = ctx.fresh_var();
        let tuple = Type::Tuple(vec![v1.clone(), v2.clone()]);
        let fv = ctx.free_vars(&tuple);
        assert_eq!(fv.len(), 2);
    }

    #[test]
    fn test_free_vars_struct() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let s = test_struct(
            "Box",
            vec![var.clone()],
            vec![("value".to_string(), var.clone())],
        );
        let fv = ctx.free_vars(&s);
        assert_eq!(fv.len(), 1);
        if let Type::Var(id) = var {
            assert!(fv.contains(&id));
        }
    }

    #[test]
    fn test_free_vars_enum() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let e = test_enum(
            "Option",
            vec![var.clone()],
            vec![(
                "Some".to_string(),
                EnumVariantType::Tuple(vec![var.clone()]),
            )],
        );
        let fv = ctx.free_vars(&e);
        assert_eq!(fv.len(), 1);
        if let Type::Var(id) = var {
            assert!(fv.contains(&id));
        }
    }

    #[test]
    fn test_free_vars_enum_struct_variant() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let e = test_enum(
            "Result",
            vec![var.clone()],
            vec![(
                "Ok".to_string(),
                EnumVariantType::Struct(vec![("value".to_string(), var.clone())]),
            )],
        );
        let fv = ctx.free_vars(&e);
        assert_eq!(fv.len(), 1);
    }

    // ==================== Resolve Tests for Complex Types ====================

    #[test]
    fn test_resolve_struct() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let s = test_struct(
            "Box",
            vec![var.clone()],
            vec![("value".to_string(), var.clone())],
        );

        ctx.unify(&var, &Type::Int).unwrap();

        let resolved = ctx.resolve(&s);
        if let Type::Struct {
            type_args, fields, ..
        } = resolved
        {
            assert_eq!(type_args[0], Type::Int);
            assert_eq!(fields[0].1, Type::Int);
        } else {
            panic!("Expected Struct type");
        }
    }

    #[test]
    fn test_resolve_enum() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let e = test_enum(
            "Option",
            vec![var.clone()],
            vec![
                (
                    "Some".to_string(),
                    EnumVariantType::Tuple(vec![var.clone()]),
                ),
                ("None".to_string(), EnumVariantType::Unit),
            ],
        );

        ctx.unify(&var, &Type::Int).unwrap();

        let resolved = ctx.resolve(&e);
        if let Type::Enum {
            type_args,
            variants,
            ..
        } = resolved
        {
            assert_eq!(type_args[0], Type::Int);
            if let EnumVariantType::Tuple(types) = &variants[0].1 {
                assert_eq!(types[0], Type::Int);
            } else {
                panic!("Expected Tuple variant");
            }
        } else {
            panic!("Expected Enum type");
        }
    }

    #[test]
    fn test_resolve_enum_struct_variant() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();
        let e = test_enum(
            "Result",
            vec![var.clone()],
            vec![(
                "Ok".to_string(),
                EnumVariantType::Struct(vec![("value".to_string(), var.clone())]),
            )],
        );

        ctx.unify(&var, &Type::Int).unwrap();

        let resolved = ctx.resolve(&e);
        if let Type::Enum { variants, .. } = resolved {
            if let EnumVariantType::Struct(fields) = &variants[0].1 {
                assert_eq!(fields[0].1, Type::Int);
            } else {
                panic!("Expected Struct variant");
            }
        } else {
            panic!("Expected Enum type");
        }
    }

    use std::collections::HashSet;
}
