//! Type unification for Hindley-Milner style type inference.

use std::collections::HashMap;

use crate::types::{Type, TypeError, TypeVarId};

/// Unification context that tracks type variable bindings.
#[derive(Debug, Clone, Default)]
pub struct UnifyCtx {
    /// Maps type variables to their bound types (Union-Find style)
    substitutions: HashMap<TypeVarId, Type>,
    /// Counter for generating fresh type variables
    next_var: usize,
}

impl UnifyCtx {
    /// Create a new empty unification context.
    pub fn new() -> Self {
        Self::default()
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
            _ => ty.clone(),
        }
    }

    /// Check if a type variable occurs in a type (occurs check).
    /// This prevents infinite types like T = List<T>.
    fn occurs(&self, var_id: TypeVarId, ty: &Type) -> bool {
        let ty = self.resolve(ty);
        match ty {
            Type::Var(id) => id == var_id,
            // For now, only Var can contain type variables
            // When we add App(constructor, args), we'll check args recursively
            _ => false,
        }
    }

    /// Unify two types, adding bindings to make them equal.
    /// Returns an error if the types cannot be unified.
    pub fn unify(&mut self, t1: &Type, t2: &Type) -> Result<(), TypeError> {
        let t1 = self.resolve(t1);
        let t2 = self.resolve(t2);

        match (&t1, &t2) {
            // Same concrete types - always unify
            (Type::Int32, Type::Int32) => Ok(()),
            (Type::Int64, Type::Int64) => Ok(()),
            (Type::Float, Type::Float) => Ok(()),
            (Type::Bool, Type::Bool) => Ok(()),
            (Type::String, Type::String) => Ok(()),

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

            // Different concrete types - cannot unify
            _ => Err(TypeError {
                message: format!("type mismatch: {} vs {}", t1, t2),
            }),
        }
    }

    /// Check if a type is fully resolved (contains no unbound type variables).
    #[allow(dead_code)]
    pub fn is_concrete(&self, ty: &Type) -> bool {
        let resolved = self.resolve(ty);
        !matches!(resolved, Type::Var(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(ctx.unify(&Type::Int32, &Type::Int32).is_ok());
        assert!(ctx.unify(&Type::Float, &Type::Float).is_ok());
        assert!(ctx.unify(&Type::Bool, &Type::Bool).is_ok());
        assert!(ctx.unify(&Type::String, &Type::String).is_ok());
    }

    #[test]
    fn test_unify_different_concrete() {
        let mut ctx = UnifyCtx::new();
        let result = ctx.unify(&Type::Int32, &Type::Float);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("type mismatch"));
    }

    #[test]
    fn test_unify_var_with_concrete() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Unify type variable with Int32
        assert!(ctx.unify(&var, &Type::Int32).is_ok());

        // The variable should now resolve to Int32
        assert_eq!(ctx.resolve(&var), Type::Int32);
    }

    #[test]
    fn test_unify_concrete_with_var() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Unify Int32 with type variable (reversed order)
        assert!(ctx.unify(&Type::Int32, &var).is_ok());

        // The variable should now resolve to Int32
        assert_eq!(ctx.resolve(&var), Type::Int32);
    }

    #[test]
    fn test_unify_two_vars() {
        let mut ctx = UnifyCtx::new();
        let v1 = ctx.fresh_var();
        let v2 = ctx.fresh_var();

        // Unify two type variables
        assert!(ctx.unify(&v1, &v2).is_ok());

        // Now bind one to a concrete type
        assert!(ctx.unify(&v1, &Type::Int32).is_ok());

        // Both should resolve to Int32
        assert_eq!(ctx.resolve(&v1), Type::Int32);
        assert_eq!(ctx.resolve(&v2), Type::Int32);
    }

    #[test]
    fn test_unify_var_already_bound() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        // Bind to Int32
        assert!(ctx.unify(&var, &Type::Int32).is_ok());

        // Unifying with same type should succeed
        assert!(ctx.unify(&var, &Type::Int32).is_ok());

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

        // Create chain: v1 -> v2 -> v3 -> Int32
        ctx.unify(&v1, &v2).unwrap();
        ctx.unify(&v2, &v3).unwrap();
        ctx.unify(&v3, &Type::Int32).unwrap();

        // All should resolve to Int32
        assert_eq!(ctx.resolve(&v1), Type::Int32);
        assert_eq!(ctx.resolve(&v2), Type::Int32);
        assert_eq!(ctx.resolve(&v3), Type::Int32);
    }

    #[test]
    fn test_is_concrete() {
        let mut ctx = UnifyCtx::new();
        let var = ctx.fresh_var();

        assert!(ctx.is_concrete(&Type::Int32));
        assert!(!ctx.is_concrete(&var));

        ctx.unify(&var, &Type::Float).unwrap();
        assert!(ctx.is_concrete(&var));
    }
}
