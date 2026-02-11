//! Maranget's algorithm for pattern matching exhaustiveness and usefulness checking.
//!
//! Based on "Warnings for pattern matching" (Luc Maranget, JFP 2007).

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use zoya_ir::{
    Definition, EnumVariantType, QualifiedPath, Type, TypeError, TypeVarId, TypedExpr,
    TypedMatchArm, TypedPattern,
};

use crate::unify::{substitute_type_vars, substitute_variant_type_vars};

/// Lookup table for resolving recursive type stubs.
///
/// During two-phase type registration, inner references to recursive types
/// carry empty variants/fields. This table maps type names to their real
/// definitions so `TypeCtors::from_type` and `Pat::from_typed` can inflate
/// these stubs at any nesting depth.
type EnumInfo = (Vec<TypeVarId>, Vec<(String, EnumVariantType)>);
type StructInfo = (Vec<TypeVarId>, Vec<(String, Type)>);

pub struct DefinitionLookup {
    enums: HashMap<String, EnumInfo>,
    structs: HashMap<String, StructInfo>,
}

impl DefinitionLookup {
    /// Build lookup from the global definitions table.
    pub fn from_definitions(definitions: &HashMap<QualifiedPath, Definition>) -> Self {
        let mut enums = HashMap::new();
        let mut structs = HashMap::new();

        for def in definitions.values() {
            match def {
                Definition::Enum(enum_type) if !enum_type.variants.is_empty() => {
                    enums.insert(
                        enum_type.name.clone(),
                        (enum_type.type_var_ids.clone(), enum_type.variants.clone()),
                    );
                }
                Definition::Struct(struct_type) if !struct_type.fields.is_empty() => {
                    structs.insert(
                        struct_type.name.clone(),
                        (struct_type.type_var_ids.clone(), struct_type.fields.clone()),
                    );
                }
                _ => {}
            }
        }

        DefinitionLookup { enums, structs }
    }

    /// Create an empty lookup (for tests that don't need recursive type resolution).
    #[cfg(test)]
    pub fn empty() -> Self {
        DefinitionLookup {
            enums: HashMap::new(),
            structs: HashMap::new(),
        }
    }

    /// Resolve an enum type: if it has empty variants but the lookup has real ones,
    /// return the inflated type.
    fn resolve_enum(
        &self,
        name: &str,
        variants: &[(String, EnumVariantType)],
        type_args: &[Type],
    ) -> Vec<(String, EnumVariantType)> {
        if !variants.is_empty() {
            return variants.to_vec();
        }
        if let Some((type_var_ids, real_variants)) = self.enums.get(name) {
            if type_args.is_empty() || type_var_ids.is_empty() {
                return real_variants.clone();
            }
            // Build substitution: type_var_ids -> type_args
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

    /// Resolve a struct type: if it has empty fields but the lookup has real ones,
    /// return the inflated fields.
    fn resolve_struct(
        &self,
        name: &str,
        fields: &[(String, Type)],
        type_args: &[Type],
    ) -> Vec<(String, Type)> {
        if !fields.is_empty() {
            return fields.to_vec();
        }
        if let Some((type_var_ids, real_fields)) = self.structs.get(name) {
            if type_args.is_empty() || type_var_ids.is_empty() {
                return real_fields.clone();
            }
            // Build substitution: type_var_ids -> type_args
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

/// A constructor represents a way to build a value of a given type.
#[derive(Debug, Clone)]
pub enum Constructor {
    // Bool constructors
    True,
    False,

    // List constructors (binary: empty vs cons)
    ListNil,  // []
    ListCons, // [head | tail] - arity 2

    // Special constructor for suffix/prefix-suffix patterns with literals
    // This represents "some specific non-empty lists" that don't overlap
    // with regular ListCons patterns for usefulness checking
    ListSpecific(u64), // deterministic hash of pattern structure

    // Tuple constructor (single constructor, arity = tuple length)
    Tuple(usize),

    // Literals for infinite types
    IntLiteral(i64),
    FloatLiteral(OrderedFloat),
    StringLiteral(String),

    // Struct constructor (single constructor per struct type)
    // Fields: (struct_name, field_names, field_types)
    Struct {
        name: String,
        field_names: Vec<String>,
        field_types: Vec<Type>,
    },

    // Enum variant constructor
    EnumVariant {
        path: QualifiedPath,
        kind: EnumVariantConstructorKind,
    },

    // Represents "all other values" for infinite types
    NonExhaustive,
}

/// Kind of enum variant for usefulness checking
#[derive(Debug, Clone)]
pub enum EnumVariantConstructorKind {
    Unit,
    Tuple {
        arity: usize,
        field_types: Vec<Type>,
    },
    Struct {
        field_names: Vec<String>,
        field_types: Vec<Type>,
    },
}

// Custom PartialEq: Struct/Enum constructors compare by name only
impl PartialEq for Constructor {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Constructor::True, Constructor::True) => true,
            (Constructor::False, Constructor::False) => true,
            (Constructor::ListNil, Constructor::ListNil) => true,
            (Constructor::ListCons, Constructor::ListCons) => true,
            (Constructor::ListSpecific(a), Constructor::ListSpecific(b)) => a == b,
            (Constructor::Tuple(a), Constructor::Tuple(b)) => a == b,
            (Constructor::IntLiteral(a), Constructor::IntLiteral(b)) => a == b,
            (Constructor::FloatLiteral(a), Constructor::FloatLiteral(b)) => a == b,
            (Constructor::StringLiteral(a), Constructor::StringLiteral(b)) => a == b,
            (Constructor::Struct { name: n1, .. }, Constructor::Struct { name: n2, .. }) => {
                n1 == n2
            }
            (
                Constructor::EnumVariant { path: p1, .. },
                Constructor::EnumVariant { path: p2, .. },
            ) => p1 == p2,
            (Constructor::NonExhaustive, Constructor::NonExhaustive) => true,
            _ => false,
        }
    }
}

impl Eq for Constructor {}

impl std::hash::Hash for Constructor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Discriminant-based hash
        std::mem::discriminant(self).hash(state);
        match self {
            Constructor::True | Constructor::False => {}
            Constructor::ListNil | Constructor::ListCons => {}
            Constructor::ListSpecific(id) => id.hash(state),
            Constructor::Tuple(n) => n.hash(state),
            Constructor::IntLiteral(n) => n.hash(state),
            Constructor::FloatLiteral(f) => f.hash(state),
            Constructor::StringLiteral(s) => s.hash(state),
            Constructor::Struct { name, .. } => name.hash(state), // Only hash name
            Constructor::EnumVariant { path, .. } => {
                path.segments().hash(state);
            }
            Constructor::NonExhaustive => {}
        }
    }
}

/// Wrapper for f64 that implements Eq and Hash (for use in HashSet)
#[derive(Debug, Clone)]
pub struct OrderedFloat(pub f64);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedFloat {}

impl std::hash::Hash for OrderedFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl Constructor {
    /// Returns the arity (number of sub-patterns) for this constructor
    fn arity(&self, _ty: &Type) -> usize {
        match self {
            Constructor::True | Constructor::False => 0,
            Constructor::ListNil => 0,
            Constructor::ListCons => 2,        // (head, tail)
            Constructor::ListSpecific(_) => 0, // opaque, no sub-patterns
            Constructor::Tuple(n) => *n,
            Constructor::Struct { field_names, .. } => field_names.len(),
            Constructor::EnumVariant { kind, .. } => match kind {
                EnumVariantConstructorKind::Unit => 0,
                EnumVariantConstructorKind::Tuple { arity, .. } => *arity,
                EnumVariantConstructorKind::Struct { field_names, .. } => field_names.len(),
            },
            Constructor::IntLiteral(_)
            | Constructor::FloatLiteral(_)
            | Constructor::StringLiteral(_)
            | Constructor::NonExhaustive => 0,
        }
    }

    /// Returns sub-types for each argument position
    fn arg_types(&self, ty: &Type) -> Vec<Type> {
        match (self, ty) {
            (Constructor::ListCons, Type::List(elem)) => {
                vec![(**elem).clone(), ty.clone()] // (T, List<T>)
            }
            (Constructor::Tuple(_), Type::Tuple(elems)) => elems.clone(),
            (Constructor::Struct { field_types, .. }, _) => field_types.clone(),
            (Constructor::EnumVariant { kind, .. }, _) => match kind {
                EnumVariantConstructorKind::Unit => vec![],
                EnumVariantConstructorKind::Tuple { field_types, .. } => field_types.clone(),
                EnumVariantConstructorKind::Struct { field_types, .. } => field_types.clone(),
            },
            _ => vec![],
        }
    }

    /// Pretty-print constructor for error messages
    fn to_string_with_args(&self, args: &[Pat], ty: &Type) -> String {
        match self {
            Constructor::True => "true".to_string(),
            Constructor::False => "false".to_string(),
            Constructor::ListNil => "[]".to_string(),
            Constructor::ListCons => {
                // Flatten nested Cons into list syntax
                flatten_list_pattern(&Pat::Ctor(self.clone(), args.to_vec()), ty)
            }
            Constructor::ListSpecific(_) => {
                // Opaque pattern - display as wildcard
                "[..]".to_string()
            }
            Constructor::Tuple(n) => {
                if *n == 0 {
                    "()".to_string()
                } else {
                    let elem_types = match ty {
                        Type::Tuple(ts) => ts.clone(),
                        _ => vec![Type::Int; *n], // fallback
                    };
                    let arg_strs: Vec<String> = args
                        .iter()
                        .zip(elem_types.iter())
                        .map(|(p, t)| p.to_pattern_string(t))
                        .collect();
                    if *n == 1 {
                        format!("({},)", arg_strs.join(", "))
                    } else {
                        format!("({})", arg_strs.join(", "))
                    }
                }
            }
            Constructor::IntLiteral(n) => n.to_string(),
            Constructor::FloatLiteral(f) => f.0.to_string(),
            Constructor::StringLiteral(s) => format!("\"{}\"", s),
            Constructor::Struct {
                name,
                field_names,
                field_types,
            } => {
                if field_names.is_empty() {
                    format!("{} {{}}", name)
                } else {
                    let field_strs: Vec<String> = field_names
                        .iter()
                        .zip(field_types.iter())
                        .zip(args.iter())
                        .map(|((field_name, field_ty), pat)| {
                            format!("{}: {}", field_name, pat.to_pattern_string(field_ty))
                        })
                        .collect();
                    format!("{} {{ {} }}", name, field_strs.join(", "))
                }
            }
            Constructor::EnumVariant { path, kind } => match kind {
                EnumVariantConstructorKind::Unit => path.to_string(),
                EnumVariantConstructorKind::Tuple { field_types, .. } => {
                    if args.is_empty() {
                        path.to_string()
                    } else {
                        let arg_strs: Vec<String> = args
                            .iter()
                            .zip(field_types.iter())
                            .map(|(p, t)| p.to_pattern_string(t))
                            .collect();
                        format!("{}({})", path, arg_strs.join(", "))
                    }
                }
                EnumVariantConstructorKind::Struct {
                    field_names,
                    field_types,
                } => {
                    if field_names.is_empty() {
                        format!("{} {{}}", path)
                    } else {
                        let field_strs: Vec<String> = field_names
                            .iter()
                            .zip(field_types.iter())
                            .zip(args.iter())
                            .map(|((fname, fty), pat)| {
                                format!("{}: {}", fname, pat.to_pattern_string(fty))
                            })
                            .collect();
                        format!("{} {{ {} }}", path, field_strs.join(", "))
                    }
                }
            },
            Constructor::NonExhaustive => "_".to_string(),
        }
    }
}

/// Type signature defines the set of constructors for a type
#[derive(Debug, Clone)]
pub enum TypeCtors {
    /// Finite set of constructors (Bool, Tuple)
    Finite(Vec<Constructor>),
    /// Infinite but structured (List = Nil | Cons)
    Structured { base_ctors: Vec<Constructor> },
    /// Infinite unstructured (Int, Float, String) - only wildcard covers all
    Infinite,
}

impl TypeCtors {
    /// Get the type signature for a given type
    pub fn from_type(ty: &Type, lookup: &DefinitionLookup) -> Self {
        match ty {
            Type::Bool => TypeCtors::Finite(vec![Constructor::True, Constructor::False]),
            Type::List(_) => TypeCtors::Structured {
                base_ctors: vec![Constructor::ListNil, Constructor::ListCons],
            },
            Type::Tuple(elems) => TypeCtors::Finite(vec![Constructor::Tuple(elems.len())]),
            Type::Int | Type::BigInt | Type::Float | Type::String => TypeCtors::Infinite,
            Type::Var(_) | Type::Function { .. } => TypeCtors::Infinite, // Conservative
            Type::Struct {
                name,
                fields,
                type_args,
            } => {
                // Resolve potentially empty fields via lookup
                let resolved_fields = lookup.resolve_struct(name, fields, type_args);
                let (field_names, field_types): (Vec<_>, Vec<_>) =
                    resolved_fields.into_iter().unzip();
                TypeCtors::Finite(vec![Constructor::Struct {
                    name: name.clone(),
                    field_names,
                    field_types,
                }])
            }
            Type::Enum {
                name: enum_name,
                variants,
                type_args,
            } => {
                // Resolve potentially empty variants via lookup
                let resolved_variants = lookup.resolve_enum(enum_name, variants, type_args);
                let ctors: Vec<Constructor> = resolved_variants
                    .iter()
                    .map(|(variant_name, variant_type)| {
                        let kind = match variant_type {
                            EnumVariantType::Unit => EnumVariantConstructorKind::Unit,
                            EnumVariantType::Tuple(field_types) => {
                                EnumVariantConstructorKind::Tuple {
                                    arity: field_types.len(),
                                    field_types: field_types.clone(),
                                }
                            }
                            EnumVariantType::Struct(fields) => {
                                let (names, types): (Vec<_>, Vec<_>) =
                                    fields.iter().cloned().unzip();
                                EnumVariantConstructorKind::Struct {
                                    field_names: names,
                                    field_types: types,
                                }
                            }
                        };
                        Constructor::EnumVariant {
                            path: QualifiedPath::new(vec![enum_name.clone(), variant_name.clone()]),
                            kind,
                        }
                    })
                    .collect();
                TypeCtors::Finite(ctors)
            }
        }
    }

    /// Check if a set of constructors covers all cases for this type
    pub fn is_complete(&self, seen: &HashSet<Constructor>) -> bool {
        match self {
            TypeCtors::Finite(ctors) => ctors.iter().all(|c| seen.contains(c)),
            TypeCtors::Structured { base_ctors } => base_ctors.iter().all(|c| seen.contains(c)),
            TypeCtors::Infinite => false, // Never complete without wildcard
        }
    }

    /// Get missing constructors
    pub fn missing(&self, seen: &HashSet<Constructor>) -> Vec<Constructor> {
        match self {
            TypeCtors::Finite(ctors) | TypeCtors::Structured { base_ctors: ctors } => ctors
                .iter()
                .filter(|c| !seen.contains(c))
                .cloned()
                .collect(),
            TypeCtors::Infinite => vec![Constructor::NonExhaustive],
        }
    }

    /// Get all constructors for this type
    pub fn all_ctors(&self) -> Vec<Constructor> {
        match self {
            TypeCtors::Finite(ctors) | TypeCtors::Structured { base_ctors: ctors } => ctors.clone(),
            TypeCtors::Infinite => vec![], // Cannot enumerate
        }
    }
}

/// Simplified pattern for the usefulness algorithm
#[derive(Debug, Clone)]
pub enum Pat {
    /// Wildcard or variable (matches anything)
    Wild,
    /// Constructor with sub-patterns
    Ctor(Constructor, Vec<Pat>),
}

impl Pat {
    /// Convert from TypedPattern to Pat
    pub fn from_typed(pattern: &TypedPattern, ty: &Type, lookup: &DefinitionLookup) -> Self {
        match pattern {
            TypedPattern::Wildcard | TypedPattern::Var { .. } => Pat::Wild,

            // As pattern: for exhaustiveness, behaves like the inner pattern
            TypedPattern::As { pattern, .. } => Pat::from_typed(pattern, ty, lookup),

            TypedPattern::Literal(lit) => {
                let ctor = match lit {
                    TypedExpr::Bool(true) => Constructor::True,
                    TypedExpr::Bool(false) => Constructor::False,
                    TypedExpr::Int(n) => Constructor::IntLiteral(*n),
                    TypedExpr::BigInt(n) => Constructor::IntLiteral(*n),
                    TypedExpr::Float(f) => Constructor::FloatLiteral(OrderedFloat(*f)),
                    TypedExpr::String(s) => Constructor::StringLiteral(s.clone()),
                    _ => return Pat::Wild, // Fallback for complex expressions
                };
                Pat::Ctor(ctor, vec![])
            }

            TypedPattern::ListEmpty => Pat::Ctor(Constructor::ListNil, vec![]),

            TypedPattern::ListExact { patterns, .. } => {
                // [a, b, c] = Cons(a, Cons(b, Cons(c, Nil)))
                Self::list_exact_to_cons(patterns, ty, lookup)
            }

            TypedPattern::ListPrefix { patterns, .. } => {
                // [a, b, ..] = Cons(a, Cons(b, _))
                Self::list_prefix_to_cons(patterns, ty, lookup)
            }

            TypedPattern::ListSuffix {
                patterns, min_len, ..
            } => {
                // [.., x] matches any list with at least min_len elements
                // If patterns contain specific literals, use a unique opaque pattern
                // to avoid incorrectly claiming coverage of all non-empty lists
                if Self::contains_specific_pattern(patterns) {
                    // Pattern like [.., 0] only matches lists ending with 0
                    // Use a deterministic hash so identical patterns share the same constructor
                    let id = Self::pattern_hash(patterns);
                    Pat::Ctor(Constructor::ListSpecific(id), vec![])
                } else {
                    // Pattern like [.., x] or [.., _] covers all non-empty lists
                    Self::list_min_length_pattern(*min_len, ty)
                }
            }

            TypedPattern::ListPrefixSuffix {
                prefix,
                suffix,
                min_len,
                ..
            } => {
                // [a, .., z] matches lists with at least min_len elements
                // If prefix/suffix contain specific literals, use unique opaque pattern
                if Self::contains_specific_pattern(prefix)
                    || Self::contains_specific_pattern(suffix)
                {
                    // Hash both prefix and suffix for a deterministic ID
                    let mut hasher = std::hash::DefaultHasher::new();
                    format!("{:?}", prefix).hash(&mut hasher);
                    format!("{:?}", suffix).hash(&mut hasher);
                    let id = hasher.finish();
                    Pat::Ctor(Constructor::ListSpecific(id), vec![])
                } else {
                    Self::list_min_length_pattern(*min_len, ty)
                }
            }

            TypedPattern::TupleEmpty => Pat::Ctor(Constructor::Tuple(0), vec![]),

            TypedPattern::TupleExact { patterns, len } => {
                let elem_types = match ty {
                    Type::Tuple(ts) => ts.clone(),
                    _ => vec![Type::Int; *len], // fallback
                };
                let sub_pats: Vec<Pat> = patterns
                    .iter()
                    .zip(elem_types.iter())
                    .map(|(p, t)| Pat::from_typed(p, t, lookup))
                    .collect();
                Pat::Ctor(Constructor::Tuple(*len), sub_pats)
            }

            TypedPattern::TuplePrefix {
                patterns,
                total_len,
                ..
            } => {
                // (a, b, ..) in a tuple of total_len elements
                Self::expand_tuple_pattern_prefix(patterns, *total_len, ty, lookup)
            }

            TypedPattern::TupleSuffix {
                patterns,
                total_len,
                ..
            } => {
                // (.., y, z) in a tuple of total_len elements
                Self::expand_tuple_pattern_suffix(patterns, *total_len, ty, lookup)
            }

            TypedPattern::TuplePrefixSuffix {
                prefix,
                suffix,
                total_len,
                ..
            } => {
                // (a, .., z) in a tuple of total_len elements
                Self::expand_tuple_pattern_both(prefix, suffix, *total_len, ty, lookup)
            }

            TypedPattern::StructExact { path, fields } => {
                // Get field info from the type, resolving stubs via lookup
                let (field_names, field_types) = Self::get_struct_field_info(ty, lookup);

                // Build sub-patterns in field order
                let mut sub_pats = Vec::with_capacity(field_names.len());
                for (field_name, field_ty) in field_names.iter().zip(field_types.iter()) {
                    // Find the pattern for this field
                    if let Some((_, sub_pattern)) = fields.iter().find(|(n, _)| n == field_name) {
                        sub_pats.push(Pat::from_typed(sub_pattern, field_ty, lookup));
                    } else {
                        sub_pats.push(Pat::Wild);
                    }
                }

                Pat::Ctor(
                    Constructor::Struct {
                        name: path.last().to_string(),
                        field_names,
                        field_types,
                    },
                    sub_pats,
                )
            }

            TypedPattern::StructPartial { path, fields } => {
                // Get field info from the type, resolving stubs via lookup
                let (field_names, field_types) = Self::get_struct_field_info(ty, lookup);

                // Build sub-patterns in field order
                // For partial patterns, unmentioned fields become wildcards
                let mut sub_pats = Vec::with_capacity(field_names.len());
                for (field_name, field_ty) in field_names.iter().zip(field_types.iter()) {
                    if let Some((_, sub_pattern)) = fields.iter().find(|(n, _)| n == field_name) {
                        sub_pats.push(Pat::from_typed(sub_pattern, field_ty, lookup));
                    } else {
                        sub_pats.push(Pat::Wild);
                    }
                }

                Pat::Ctor(
                    Constructor::Struct {
                        name: path.last().to_string(),
                        field_names,
                        field_types,
                    },
                    sub_pats,
                )
            }

            // Tuple struct patterns
            TypedPattern::StructTupleExact { path, patterns, .. }
            | TypedPattern::StructTuplePrefix { path, patterns, .. } => {
                let (field_names, field_types) = Self::get_struct_field_info(ty, lookup);
                let sub_pats: Vec<Pat> = patterns
                    .iter()
                    .zip(field_types.iter())
                    .map(|(p, t)| Pat::from_typed(p, t, lookup))
                    .collect();
                // Pad with wildcards if prefix pattern
                let mut all_pats = sub_pats;
                while all_pats.len() < field_types.len() {
                    all_pats.push(Pat::Wild);
                }
                Pat::Ctor(
                    Constructor::Struct {
                        name: path.last().to_string(),
                        field_names,
                        field_types,
                    },
                    all_pats,
                )
            }

            TypedPattern::StructTupleSuffix {
                path,
                patterns,
                total_fields,
                ..
            } => {
                let (field_names, field_types) = Self::get_struct_field_info(ty, lookup);
                let start_idx = total_fields - patterns.len();
                let mut sub_pats = vec![Pat::Wild; start_idx];
                for (p, t) in patterns.iter().zip(field_types.iter().skip(start_idx)) {
                    sub_pats.push(Pat::from_typed(p, t, lookup));
                }
                Pat::Ctor(
                    Constructor::Struct {
                        name: path.last().to_string(),
                        field_names,
                        field_types,
                    },
                    sub_pats,
                )
            }

            TypedPattern::StructTuplePrefixSuffix {
                path,
                prefix,
                suffix,
                total_fields,
                ..
            } => {
                let (field_names, field_types) = Self::get_struct_field_info(ty, lookup);
                let mut sub_pats = Vec::with_capacity(*total_fields);
                // Add prefix patterns
                for (p, t) in prefix.iter().zip(field_types.iter()) {
                    sub_pats.push(Pat::from_typed(p, t, lookup));
                }
                // Add wildcards for middle
                let middle_count = total_fields - prefix.len() - suffix.len();
                for _ in 0..middle_count {
                    sub_pats.push(Pat::Wild);
                }
                // Add suffix patterns
                let suffix_start = total_fields - suffix.len();
                for (p, t) in suffix.iter().zip(field_types.iter().skip(suffix_start)) {
                    sub_pats.push(Pat::from_typed(p, t, lookup));
                }
                Pat::Ctor(
                    Constructor::Struct {
                        name: path.last().to_string(),
                        field_names,
                        field_types,
                    },
                    sub_pats,
                )
            }

            // Enum patterns
            TypedPattern::EnumUnit { path } => Pat::Ctor(
                Constructor::EnumVariant {
                    path: path.clone(),
                    kind: EnumVariantConstructorKind::Unit,
                },
                vec![],
            ),

            TypedPattern::EnumTupleExact { path, patterns, .. }
            | TypedPattern::EnumTuplePrefix { path, patterns, .. } => {
                // Get field types from the type, resolving stubs via lookup
                let field_types = Self::get_enum_tuple_variant_types(ty, path.last(), lookup);
                let sub_pats: Vec<Pat> = patterns
                    .iter()
                    .zip(field_types.iter())
                    .map(|(p, t)| Pat::from_typed(p, t, lookup))
                    .collect();
                // Pad with wildcards if prefix pattern
                let mut all_pats = sub_pats;
                while all_pats.len() < field_types.len() {
                    all_pats.push(Pat::Wild);
                }
                Pat::Ctor(
                    Constructor::EnumVariant {
                        path: path.clone(),
                        kind: EnumVariantConstructorKind::Tuple {
                            arity: field_types.len(),
                            field_types,
                        },
                    },
                    all_pats,
                )
            }

            TypedPattern::EnumTupleSuffix {
                path,
                patterns,
                total_fields,
                ..
            } => {
                let field_types = Self::get_enum_tuple_variant_types(ty, path.last(), lookup);
                let start_idx = total_fields - patterns.len();
                let mut sub_pats = vec![Pat::Wild; start_idx];
                for (p, t) in patterns.iter().zip(field_types.iter().skip(start_idx)) {
                    sub_pats.push(Pat::from_typed(p, t, lookup));
                }
                Pat::Ctor(
                    Constructor::EnumVariant {
                        path: path.clone(),
                        kind: EnumVariantConstructorKind::Tuple {
                            arity: *total_fields,
                            field_types,
                        },
                    },
                    sub_pats,
                )
            }

            TypedPattern::EnumTuplePrefixSuffix {
                path,
                prefix,
                suffix,
                total_fields,
                ..
            } => {
                let field_types = Self::get_enum_tuple_variant_types(ty, path.last(), lookup);
                let mut sub_pats = Vec::with_capacity(*total_fields);
                // Add prefix patterns
                for (p, t) in prefix.iter().zip(field_types.iter()) {
                    sub_pats.push(Pat::from_typed(p, t, lookup));
                }
                // Add wildcards for middle
                let middle_count = total_fields - prefix.len() - suffix.len();
                for _ in 0..middle_count {
                    sub_pats.push(Pat::Wild);
                }
                // Add suffix patterns
                let suffix_start = total_fields - suffix.len();
                for (p, t) in suffix.iter().zip(field_types.iter().skip(suffix_start)) {
                    sub_pats.push(Pat::from_typed(p, t, lookup));
                }
                Pat::Ctor(
                    Constructor::EnumVariant {
                        path: path.clone(),
                        kind: EnumVariantConstructorKind::Tuple {
                            arity: *total_fields,
                            field_types,
                        },
                    },
                    sub_pats,
                )
            }

            TypedPattern::EnumStructExact { path, fields }
            | TypedPattern::EnumStructPartial { path, fields } => {
                // Get field info from the type, resolving stubs via lookup
                let (field_names, field_types) =
                    Self::get_enum_struct_variant_info(ty, path.last(), lookup);

                // Build sub-patterns in field order
                let mut sub_pats = Vec::with_capacity(field_names.len());
                for (field_name, field_ty) in field_names.iter().zip(field_types.iter()) {
                    if let Some((_, sub_pattern)) = fields.iter().find(|(n, _)| n == field_name) {
                        sub_pats.push(Pat::from_typed(sub_pattern, field_ty, lookup));
                    } else {
                        sub_pats.push(Pat::Wild);
                    }
                }

                Pat::Ctor(
                    Constructor::EnumVariant {
                        path: path.clone(),
                        kind: EnumVariantConstructorKind::Struct {
                            field_names,
                            field_types,
                        },
                    },
                    sub_pats,
                )
            }
        }
    }

    /// Get tuple variant field types from an enum type, resolving stubs via lookup
    fn get_enum_tuple_variant_types(
        ty: &Type,
        variant_name: &str,
        lookup: &DefinitionLookup,
    ) -> Vec<Type> {
        match ty {
            Type::Enum {
                name,
                variants,
                type_args,
            } => {
                let resolved = lookup.resolve_enum(name, variants, type_args);
                for (vname, vtype) in &resolved {
                    if vname == variant_name
                        && let EnumVariantType::Tuple(types) = vtype
                    {
                        return types.clone();
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// Get struct variant field info from an enum type, resolving stubs via lookup
    fn get_enum_struct_variant_info(
        ty: &Type,
        variant_name: &str,
        lookup: &DefinitionLookup,
    ) -> (Vec<String>, Vec<Type>) {
        match ty {
            Type::Enum {
                name,
                variants,
                type_args,
            } => {
                let resolved = lookup.resolve_enum(name, variants, type_args);
                for (vname, vtype) in &resolved {
                    if vname == variant_name
                        && let EnumVariantType::Struct(fields) = vtype
                    {
                        let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                        let types: Vec<Type> = fields.iter().map(|(_, t)| t.clone()).collect();
                        return (names, types);
                    }
                }
                (vec![], vec![])
            }
            _ => (vec![], vec![]),
        }
    }

    /// Get struct field info from a struct type, resolving stubs via lookup
    fn get_struct_field_info(ty: &Type, lookup: &DefinitionLookup) -> (Vec<String>, Vec<Type>) {
        match ty {
            Type::Struct {
                name,
                fields,
                type_args,
            } => {
                let resolved = lookup.resolve_struct(name, fields, type_args);
                let names: Vec<String> = resolved.iter().map(|(n, _)| n.clone()).collect();
                let types: Vec<Type> = resolved.iter().map(|(_, t)| t.clone()).collect();
                (names, types)
            }
            _ => (vec![], vec![]),
        }
    }

    /// Check if any pattern in a list contains specific literals (not wildcards/variables)
    fn contains_specific_pattern(patterns: &[TypedPattern]) -> bool {
        patterns
            .iter()
            .any(|p| matches!(p, TypedPattern::Literal(_)))
    }

    /// Compute a deterministic ID for a list of typed patterns by hashing their Debug representation.
    /// Structurally identical patterns produce the same ID.
    fn pattern_hash(patterns: &[TypedPattern]) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        format!("{:?}", patterns).hash(&mut hasher);
        hasher.finish()
    }

    /// Convert [a, b, c] to nested Cons: Cons(a, Cons(b, Cons(c, Nil)))
    fn list_exact_to_cons(patterns: &[TypedPattern], ty: &Type, lookup: &DefinitionLookup) -> Pat {
        let elem_ty = match ty {
            Type::List(e) => (**e).clone(),
            _ => Type::Int, // Fallback
        };

        let mut result = Pat::Ctor(Constructor::ListNil, vec![]);
        for pat in patterns.iter().rev() {
            let head = Pat::from_typed(pat, &elem_ty, lookup);
            result = Pat::Ctor(Constructor::ListCons, vec![head, result]);
        }
        result
    }

    /// Convert [a, b, ..] to Cons(a, Cons(b, _))
    fn list_prefix_to_cons(patterns: &[TypedPattern], ty: &Type, lookup: &DefinitionLookup) -> Pat {
        let elem_ty = match ty {
            Type::List(e) => (**e).clone(),
            _ => Type::Int,
        };

        let mut result = Pat::Wild; // Tail is wildcard
        for pat in patterns.iter().rev() {
            let head = Pat::from_typed(pat, &elem_ty, lookup);
            result = Pat::Ctor(Constructor::ListCons, vec![head, result]);
        }
        result
    }

    /// Create a pattern that matches lists with at least n elements
    /// Cons(_, Cons(_, ... Cons(_, _)))
    fn list_min_length_pattern(min_len: usize, _ty: &Type) -> Pat {
        let mut result = Pat::Wild;
        for _ in 0..min_len {
            result = Pat::Ctor(Constructor::ListCons, vec![Pat::Wild, result]);
        }
        result
    }

    /// Expand (a, b, ..) to (a, b, _, _, ...) for a tuple of total_len
    fn expand_tuple_pattern_prefix(
        patterns: &[TypedPattern],
        total_len: usize,
        ty: &Type,
        lookup: &DefinitionLookup,
    ) -> Pat {
        let elem_types = match ty {
            Type::Tuple(ts) => ts.clone(),
            _ => vec![Type::Int; total_len],
        };

        let mut sub_pats = Vec::with_capacity(total_len);
        for (i, elem_ty) in elem_types.iter().enumerate() {
            if i < patterns.len() {
                sub_pats.push(Pat::from_typed(&patterns[i], elem_ty, lookup));
            } else {
                sub_pats.push(Pat::Wild);
            }
        }
        Pat::Ctor(Constructor::Tuple(total_len), sub_pats)
    }

    /// Expand (.., y, z) to (_, _, ..., y, z) for a tuple of total_len
    fn expand_tuple_pattern_suffix(
        patterns: &[TypedPattern],
        total_len: usize,
        ty: &Type,
        lookup: &DefinitionLookup,
    ) -> Pat {
        let elem_types = match ty {
            Type::Tuple(ts) => ts.clone(),
            _ => vec![Type::Int; total_len],
        };

        let prefix_wilds = total_len - patterns.len();
        let mut sub_pats = Vec::with_capacity(total_len);

        for (i, elem_ty) in elem_types.iter().enumerate() {
            if i < prefix_wilds {
                sub_pats.push(Pat::Wild);
            } else {
                sub_pats.push(Pat::from_typed(
                    &patterns[i - prefix_wilds],
                    elem_ty,
                    lookup,
                ));
            }
        }
        Pat::Ctor(Constructor::Tuple(total_len), sub_pats)
    }

    /// Expand (a, .., z) to (a, _, ..., _, z) for a tuple of total_len
    fn expand_tuple_pattern_both(
        prefix: &[TypedPattern],
        suffix: &[TypedPattern],
        total_len: usize,
        ty: &Type,
        lookup: &DefinitionLookup,
    ) -> Pat {
        let elem_types = match ty {
            Type::Tuple(ts) => ts.clone(),
            _ => vec![Type::Int; total_len],
        };

        let middle_wilds = total_len - prefix.len() - suffix.len();
        let mut sub_pats = Vec::with_capacity(total_len);

        for (i, elem_ty) in elem_types.iter().enumerate() {
            if i < prefix.len() {
                sub_pats.push(Pat::from_typed(&prefix[i], elem_ty, lookup));
            } else if i < prefix.len() + middle_wilds {
                sub_pats.push(Pat::Wild);
            } else {
                let suffix_idx = i - prefix.len() - middle_wilds;
                sub_pats.push(Pat::from_typed(&suffix[suffix_idx], elem_ty, lookup));
            }
        }
        Pat::Ctor(Constructor::Tuple(total_len), sub_pats)
    }

    /// Pretty-print pattern for error messages
    pub fn to_pattern_string(&self, ty: &Type) -> String {
        match self {
            Pat::Wild => "_".to_string(),
            Pat::Ctor(c, args) => c.to_string_with_args(args, ty),
        }
    }
}

/// Flatten Cons(a, Cons(b, Nil)) to "[a, b]" or Cons(a, Wild) to "[a, ..]"
fn flatten_list_pattern(pat: &Pat, ty: &Type) -> String {
    let elem_ty = match ty {
        Type::List(e) => (**e).clone(),
        _ => Type::Int,
    };

    let mut elements = vec![];
    let mut current = pat;

    loop {
        match current {
            Pat::Ctor(Constructor::ListCons, args) if args.len() == 2 => {
                elements.push(args[0].to_pattern_string(&elem_ty));
                current = &args[1];
            }
            Pat::Ctor(Constructor::ListNil, _) => {
                return format!("[{}]", elements.join(", "));
            }
            Pat::Wild => {
                if elements.is_empty() {
                    return "[_, ..]".to_string();
                } else {
                    return format!("[{}, ..]", elements.join(", "));
                }
            }
            _ => {
                if elements.is_empty() {
                    return "[..]".to_string();
                } else {
                    return format!("[{}, ..]", elements.join(", "));
                }
            }
        }
    }
}

/// A matrix of patterns where each row is a match arm's patterns
#[derive(Debug, Clone)]
pub struct PatternMatrix {
    /// Each row: (patterns for each column, row index for reporting)
    rows: Vec<(Vec<Pat>, usize)>,
    /// Types for each column
    types: Vec<Type>,
}

impl PatternMatrix {
    /// Create from a list of match arms
    pub fn from_arms(
        arms: &[TypedMatchArm],
        scrutinee_ty: &Type,
        lookup: &DefinitionLookup,
    ) -> Self {
        let rows: Vec<_> = arms
            .iter()
            .enumerate()
            .map(|(idx, arm)| {
                let pat = Pat::from_typed(&arm.pattern, scrutinee_ty, lookup);
                (vec![pat], idx)
            })
            .collect();

        PatternMatrix {
            rows,
            types: vec![scrutinee_ty.clone()],
        }
    }

    /// Check if matrix is empty (no rows)
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Check if matrix has no columns (zero-width)
    pub fn has_no_columns(&self) -> bool {
        self.types.is_empty()
    }

    /// Get the first column type
    pub fn first_type(&self) -> Option<&Type> {
        self.types.first()
    }
}

/// Witness: A concrete pattern that demonstrates non-exhaustiveness
#[derive(Debug, Clone)]
pub struct Witness(pub Vec<Pat>);

impl Witness {
    /// Pretty-print the witness as a missing pattern
    pub fn to_string(&self, types: &[Type]) -> String {
        if self.0.is_empty() || types.is_empty() {
            return "_".to_string();
        }
        self.0[0].to_pattern_string(&types[0])
    }
}

/// Result of usefulness check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Usefulness {
    /// Pattern is useful - can match values not covered by earlier patterns
    Useful,
    /// Pattern is not useful (unreachable)
    NotUseful,
}

/// Result of exhaustiveness check
#[derive(Debug, Clone)]
pub enum Exhaustiveness {
    /// All cases covered
    Exhaustive,
    /// Some cases missing, with witness examples
    NonExhaustive(Vec<Witness>),
}

/// Check if pattern vector q is useful with respect to matrix P.
///
/// Base cases:
/// - If P has no columns: useful iff P has no rows
/// - If P has no rows: always useful
///
/// Inductive case:
/// - Look at first column
/// - If q starts with constructor c: specialize P and q by c, recurse
/// - If q starts with wildcard: check if default matrix is complete
fn useful(matrix: &PatternMatrix, q: &[Pat], lookup: &DefinitionLookup) -> Usefulness {
    // Base case: zero columns
    if matrix.has_no_columns() {
        return if matrix.is_empty() {
            Usefulness::Useful
        } else {
            Usefulness::NotUseful
        };
    }

    // Base case: empty matrix (no rows)
    if matrix.is_empty() {
        return Usefulness::Useful;
    }

    let ty = matrix.first_type().unwrap();
    let first_pat = &q[0];

    match first_pat {
        Pat::Ctor(c, args) => {
            // Specialize by constructor c
            let specialized = specialize_matrix(matrix, c, ty);
            let specialized_q = specialize_row(q, c, args, ty);
            useful(&specialized, &specialized_q, lookup)
        }
        Pat::Wild => {
            // Collect all constructors appearing in first column
            let seen_ctors = collect_head_ctors(matrix);
            let type_ctors = TypeCtors::from_type(ty, lookup);

            if type_ctors.is_complete(&seen_ctors) {
                // Complete: must check each constructor
                for c in type_ctors.all_ctors() {
                    let specialized = specialize_matrix(matrix, &c, ty);
                    let arity = c.arity(ty);
                    let wild_args: Vec<Pat> = (0..arity).map(|_| Pat::Wild).collect();
                    let specialized_q: Vec<Pat> = wild_args
                        .into_iter()
                        .chain(q[1..].iter().cloned())
                        .collect();

                    if useful(&specialized, &specialized_q, lookup) == Usefulness::Useful {
                        return Usefulness::Useful;
                    }
                }
                Usefulness::NotUseful
            } else {
                // Not complete: use default matrix
                let default = default_matrix(matrix);
                let default_q: Vec<Pat> = q[1..].to_vec();
                useful(&default, &default_q, lookup)
            }
        }
    }
}

/// Specialize matrix by constructor c.
/// For each row:
/// - If head is c(p1,...,pn): replace with [p1,...,pn, rest...]
/// - If head is wildcard: replace with [_,...,_, rest...] (arity copies)
/// - Otherwise: drop row
fn specialize_matrix(matrix: &PatternMatrix, c: &Constructor, ty: &Type) -> PatternMatrix {
    let arity = c.arity(ty);
    let arg_types = c.arg_types(ty);

    let new_types: Vec<Type> = arg_types
        .into_iter()
        .chain(matrix.types[1..].iter().cloned())
        .collect();

    let new_rows: Vec<_> = matrix
        .rows
        .iter()
        .filter_map(|(pats, idx)| {
            let head = &pats[0];
            match head {
                Pat::Ctor(head_c, args) if head_c == c => {
                    let new_pats: Vec<Pat> = args
                        .iter()
                        .cloned()
                        .chain(pats[1..].iter().cloned())
                        .collect();
                    Some((new_pats, *idx))
                }
                Pat::Wild => {
                    let wilds: Vec<Pat> = (0..arity).map(|_| Pat::Wild).collect();
                    let new_pats: Vec<Pat> =
                        wilds.into_iter().chain(pats[1..].iter().cloned()).collect();
                    Some((new_pats, *idx))
                }
                _ => None, // Different constructor, drop row
            }
        })
        .collect();

    PatternMatrix {
        rows: new_rows,
        types: new_types,
    }
}

/// Specialize a single row by constructor
fn specialize_row(q: &[Pat], c: &Constructor, args: &[Pat], ty: &Type) -> Vec<Pat> {
    let _ = c.arity(ty); // validate
    args.iter().cloned().chain(q[1..].iter().cloned()).collect()
}

/// Default matrix: rows whose head is a wildcard, with head removed.
fn default_matrix(matrix: &PatternMatrix) -> PatternMatrix {
    let new_types = matrix.types[1..].to_vec();

    let new_rows: Vec<_> = matrix
        .rows
        .iter()
        .filter_map(|(pats, idx)| match &pats[0] {
            Pat::Wild => Some((pats[1..].to_vec(), *idx)),
            _ => None,
        })
        .collect();

    PatternMatrix {
        rows: new_rows,
        types: new_types,
    }
}

/// Collect all constructors appearing as heads of first column
fn collect_head_ctors(matrix: &PatternMatrix) -> HashSet<Constructor> {
    matrix
        .rows
        .iter()
        .filter_map(|(pats, _)| match &pats[0] {
            Pat::Ctor(c, _) => Some(c.clone()),
            Pat::Wild => None,
        })
        .collect()
}

/// Compute witness patterns (missing cases).
/// Similar to `useful`, but returns example patterns instead of just bool.
fn compute_witnesses(matrix: &PatternMatrix, q: &[Pat], lookup: &DefinitionLookup) -> Vec<Witness> {
    if matrix.has_no_columns() {
        return if matrix.is_empty() {
            vec![Witness(vec![])]
        } else {
            vec![]
        };
    }

    if matrix.is_empty() {
        return vec![Witness(q.to_vec())];
    }

    let ty = matrix.first_type().unwrap();
    let first_pat = &q[0];

    match first_pat {
        Pat::Ctor(c, args) => {
            let specialized = specialize_matrix(matrix, c, ty);
            let specialized_q = specialize_row(q, c, args, ty);
            let sub_witnesses = compute_witnesses(&specialized, &specialized_q, lookup);

            // Reconstruct witnesses with constructor c
            sub_witnesses
                .into_iter()
                .map(|w| reconstruct_witness(w, c, ty))
                .collect()
        }
        Pat::Wild => {
            let seen_ctors = collect_head_ctors(matrix);
            let type_ctors = TypeCtors::from_type(ty, lookup);

            if type_ctors.is_complete(&seen_ctors) {
                // Check each constructor
                let mut all_witnesses = vec![];
                for c in type_ctors.all_ctors() {
                    let specialized = specialize_matrix(matrix, &c, ty);
                    let arity = c.arity(ty);
                    let wild_args: Vec<Pat> = (0..arity).map(|_| Pat::Wild).collect();
                    let specialized_q: Vec<Pat> = wild_args
                        .into_iter()
                        .chain(q[1..].iter().cloned())
                        .collect();

                    let sub_witnesses = compute_witnesses(&specialized, &specialized_q, lookup);
                    for w in sub_witnesses {
                        all_witnesses.push(reconstruct_witness(w, &c, ty));
                    }
                }
                all_witnesses
            } else {
                // Find a missing constructor and use it
                let missing = type_ctors.missing(&seen_ctors);

                if missing.is_empty() {
                    // Should use default matrix
                    let default = default_matrix(matrix);
                    let default_q: Vec<Pat> = q[1..].to_vec();
                    let sub_witnesses = compute_witnesses(&default, &default_q, lookup);

                    // Prefix with wildcard
                    sub_witnesses
                        .into_iter()
                        .map(|Witness(pats)| {
                            let mut new_pats = vec![Pat::Wild];
                            new_pats.extend(pats);
                            Witness(new_pats)
                        })
                        .collect()
                } else {
                    // Use first missing constructor
                    let c = &missing[0];
                    let arity = c.arity(ty);

                    // Recurse with this constructor
                    let specialized = specialize_matrix(matrix, c, ty);
                    let wild_args: Vec<Pat> = (0..arity).map(|_| Pat::Wild).collect();
                    let specialized_q: Vec<Pat> = wild_args
                        .into_iter()
                        .chain(q[1..].iter().cloned())
                        .collect();

                    let sub_witnesses = compute_witnesses(&specialized, &specialized_q, lookup);
                    sub_witnesses
                        .into_iter()
                        .map(|w| reconstruct_witness(w, c, ty))
                        .collect()
                }
            }
        }
    }
}

/// Reconstruct a witness by wrapping sub-patterns in constructor c
fn reconstruct_witness(Witness(sub_pats): Witness, c: &Constructor, ty: &Type) -> Witness {
    let arity = c.arity(ty);
    let (ctor_args, rest) = if sub_pats.len() >= arity {
        sub_pats.split_at(arity)
    } else {
        // Not enough patterns, pad with wildcards
        let mut padded = sub_pats.clone();
        while padded.len() < arity {
            padded.push(Pat::Wild);
        }
        return Witness(vec![Pat::Ctor(c.clone(), padded)]);
    };

    let new_head = Pat::Ctor(c.clone(), ctor_args.to_vec());

    let mut result = vec![new_head];
    result.extend(rest.iter().cloned());
    Witness(result)
}

/// Check if a match expression is exhaustive.
/// Returns missing patterns if not.
pub fn check_exhaustiveness(
    arms: &[TypedMatchArm],
    scrutinee_ty: &Type,
    lookup: &DefinitionLookup,
) -> Exhaustiveness {
    let matrix = PatternMatrix::from_arms(arms, scrutinee_ty, lookup);

    // Check if wildcard vector is useful (i.e., can any value slip through?)
    let wild_vec = vec![Pat::Wild];

    let witnesses = compute_witnesses(&matrix, &wild_vec, lookup);
    if witnesses.is_empty() {
        Exhaustiveness::Exhaustive
    } else {
        Exhaustiveness::NonExhaustive(witnesses)
    }
}

/// Check each arm for usefulness (reachability).
/// Returns indices of unreachable arms.
pub fn check_usefulness(
    arms: &[TypedMatchArm],
    scrutinee_ty: &Type,
    lookup: &DefinitionLookup,
) -> Vec<usize> {
    let mut matrix = PatternMatrix {
        rows: vec![],
        types: vec![scrutinee_ty.clone()],
    };

    let mut unreachable = vec![];

    for (idx, arm) in arms.iter().enumerate() {
        let pat = Pat::from_typed(&arm.pattern, scrutinee_ty, lookup);
        let q = vec![pat.clone()];

        if useful(&matrix, &q, lookup) == Usefulness::NotUseful {
            unreachable.push(idx);
        }

        // Add this pattern to the matrix for checking subsequent patterns
        matrix.rows.push((q, idx));
    }

    unreachable
}

/// Combined check: returns error if patterns are non-exhaustive or have unreachable arms
pub fn check_patterns(
    arms: &[TypedMatchArm],
    scrutinee_ty: &Type,
    lookup: &DefinitionLookup,
) -> Result<(), TypeError> {
    // Check for unreachable patterns first
    let unreachable_arms = check_usefulness(arms, scrutinee_ty, lookup);
    if !unreachable_arms.is_empty() {
        let arm_numbers: Vec<String> = unreachable_arms
            .iter()
            .map(|i| (i + 1).to_string())
            .collect();
        return Err(TypeError {
            message: format!("unreachable pattern(s): arm(s) {}", arm_numbers.join(", ")),
        });
    }

    // Check exhaustiveness
    match check_exhaustiveness(arms, scrutinee_ty, lookup) {
        Exhaustiveness::Exhaustive => Ok(()),
        Exhaustiveness::NonExhaustive(witnesses) => {
            let missing_patterns: Vec<String> = witnesses
                .iter()
                .take(3) // Limit to first 3 examples
                .map(|w| w.to_string(std::slice::from_ref(scrutinee_ty)))
                .collect();

            let more = if witnesses.len() > 3 {
                format!(" and {} more", witnesses.len() - 3)
            } else {
                String::new()
            };

            Err(TypeError {
                message: format!(
                    "non-exhaustive match: missing pattern(s) {}{}",
                    missing_patterns.join(", "),
                    more
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ir::TypeVarId;

    fn make_bool_arm(val: bool) -> TypedMatchArm {
        TypedMatchArm {
            pattern: TypedPattern::Literal(TypedExpr::Bool(val)),
            result: TypedExpr::Int(0),
        }
    }

    fn make_wildcard_arm() -> TypedMatchArm {
        TypedMatchArm {
            pattern: TypedPattern::Wildcard,
            result: TypedExpr::Int(0),
        }
    }

    fn make_var_arm(name: &str, ty: Type) -> TypedMatchArm {
        TypedMatchArm {
            pattern: TypedPattern::Var {
                name: name.to_string(),
                ty,
            },
            result: TypedExpr::Int(0),
        }
    }

    fn make_list_empty_arm() -> TypedMatchArm {
        TypedMatchArm {
            pattern: TypedPattern::ListEmpty,
            result: TypedExpr::Int(0),
        }
    }

    fn make_list_prefix_arm(len: usize) -> TypedMatchArm {
        let patterns: Vec<TypedPattern> = (0..len)
            .map(|i| TypedPattern::Var {
                name: format!("x{}", i),
                ty: Type::Int,
            })
            .collect();
        TypedMatchArm {
            pattern: TypedPattern::ListPrefix {
                patterns,
                rest_binding: None,
                min_len: len,
            },
            result: TypedExpr::Int(0),
        }
    }

    // Bool exhaustiveness tests
    #[test]
    fn test_bool_exhaustive_both() {
        let arms = vec![make_bool_arm(true), make_bool_arm(false)];
        let result = check_exhaustiveness(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_bool_exhaustive_wildcard() {
        let arms = vec![make_wildcard_arm()];
        let result = check_exhaustiveness(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_bool_missing_false() {
        let arms = vec![make_bool_arm(true)];
        let result = check_exhaustiveness(&arms, &Type::Bool, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(&[Type::Bool]);
                assert_eq!(missing, "false");
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_bool_missing_true() {
        let arms = vec![make_bool_arm(false)];
        let result = check_exhaustiveness(&arms, &Type::Bool, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(&[Type::Bool]);
                assert_eq!(missing, "true");
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    // List exhaustiveness tests
    #[test]
    fn test_list_exhaustive_empty_and_nonempty() {
        let arms = vec![make_list_empty_arm(), make_list_prefix_arm(1)];
        let result = check_exhaustiveness(
            &arms,
            &Type::List(Box::new(Type::Int)),
            &DefinitionLookup::empty(),
        );
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_list_exhaustive_wildcard() {
        let arms = vec![make_var_arm("xs", Type::List(Box::new(Type::Int)))];
        let result = check_exhaustiveness(
            &arms,
            &Type::List(Box::new(Type::Int)),
            &DefinitionLookup::empty(),
        );
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_list_missing_empty() {
        let arms = vec![make_list_prefix_arm(1)];
        let result = check_exhaustiveness(
            &arms,
            &Type::List(Box::new(Type::Int)),
            &DefinitionLookup::empty(),
        );
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(&[Type::List(Box::new(Type::Int))]);
                assert_eq!(missing, "[]");
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_list_missing_nonempty() {
        let arms = vec![make_list_empty_arm()];
        let result = check_exhaustiveness(
            &arms,
            &Type::List(Box::new(Type::Int)),
            &DefinitionLookup::empty(),
        );
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(&[Type::List(Box::new(Type::Int))]);
                assert!(missing.contains("[") && missing.contains("..")); // [_, ..]
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    // Usefulness tests
    #[test]
    fn test_unreachable_after_wildcard() {
        let arms = vec![make_wildcard_arm(), make_bool_arm(true)];
        let unreachable = check_usefulness(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1]);
    }

    #[test]
    fn test_unreachable_duplicate_literal() {
        let arms = vec![
            make_bool_arm(true),
            make_bool_arm(true), // duplicate
            make_bool_arm(false),
        ];
        let unreachable = check_usefulness(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1]);
    }

    #[test]
    fn test_no_unreachable_all_distinct() {
        let arms = vec![make_bool_arm(true), make_bool_arm(false)];
        let unreachable = check_usefulness(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(unreachable.is_empty());
    }

    // Int exhaustiveness (infinite type)
    #[test]
    fn test_int_not_exhaustive_without_wildcard() {
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::Literal(TypedExpr::Int(0)),
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &Type::Int, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    #[test]
    fn test_int_exhaustive_with_wildcard() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Int(0)),
                result: TypedExpr::Int(0),
            },
            make_wildcard_arm(),
        ];
        let result = check_exhaustiveness(&arms, &Type::Int, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // Tuple exhaustiveness
    #[test]
    fn test_tuple_exhaustive_single_pattern() {
        let tuple_ty = Type::Tuple(vec![Type::Int, Type::Bool]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleExact {
                patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                len: 2,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_tuple_with_bool_needs_both() {
        let tuple_ty = Type::Tuple(vec![Type::Int, Type::Bool]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleExact {
                patterns: vec![
                    TypedPattern::Wildcard,
                    TypedPattern::Literal(TypedExpr::Bool(true)),
                ],
                len: 2,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(std::slice::from_ref(&tuple_ty));
                assert!(missing.contains("false"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    // ==================== Helper functions for new tests ====================

    fn make_struct_type(name: &str, fields: Vec<(&str, Type)>) -> Type {
        Type::Struct {
            name: name.to_string(),
            type_args: vec![],
            fields: fields
                .into_iter()
                .map(|(n, t)| (n.to_string(), t))
                .collect(),
        }
    }

    fn make_enum_type(name: &str, variants: Vec<(&str, EnumVariantType)>) -> Type {
        Type::Enum {
            name: name.to_string(),
            type_args: vec![],
            variants: variants
                .into_iter()
                .map(|(n, v)| (n.to_string(), v))
                .collect(),
        }
    }

    fn make_qualified_path(segments: Vec<&str>) -> QualifiedPath {
        QualifiedPath::new(segments.into_iter().map(|s| s.to_string()).collect())
    }

    // ==================== Struct Pattern Tests ====================

    #[test]
    fn test_struct_exhaustive_single_pattern() {
        let struct_ty = make_struct_type("Point", vec![("x", Type::Int), ("y", Type::Int)]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::StructExact {
                path: make_qualified_path(vec!["Point"]),
                fields: vec![
                    ("x".to_string(), TypedPattern::Wildcard),
                    ("y".to_string(), TypedPattern::Wildcard),
                ],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &struct_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_struct_partial_exhaustive() {
        let struct_ty = make_struct_type("Point", vec![("x", Type::Int), ("y", Type::Int)]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::StructPartial {
                path: make_qualified_path(vec!["Point"]),
                fields: vec![("x".to_string(), TypedPattern::Wildcard)],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &struct_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_struct_with_bool_field_needs_both() {
        let struct_ty = make_struct_type("Flags", vec![("enabled", Type::Bool)]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::StructExact {
                path: make_qualified_path(vec!["Flags"]),
                fields: vec![(
                    "enabled".to_string(),
                    TypedPattern::Literal(TypedExpr::Bool(true)),
                )],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &struct_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    // ==================== Enum Pattern Tests ====================

    #[test]
    fn test_enum_unit_exhaustive() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Option", "None"]),
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumTupleExact {
                    path: make_qualified_path(vec!["Option", "Some"]),
                    patterns: vec![TypedPattern::Wildcard],
                    total_fields: 1,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &option_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_unit_missing_variant() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumUnit {
                path: make_qualified_path(vec!["Option", "None"]),
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &option_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(std::slice::from_ref(&option_ty));
                assert!(missing.contains("Some"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_enum_multiple_variants_exhaustive() {
        let color_ty = make_enum_type(
            "Color",
            vec![
                ("Red", EnumVariantType::Unit),
                ("Green", EnumVariantType::Unit),
                ("Blue", EnumVariantType::Unit),
            ],
        );
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Red"]),
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Green"]),
                },
                result: TypedExpr::Int(1),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Blue"]),
                },
                result: TypedExpr::Int(2),
            },
        ];
        let result = check_exhaustiveness(&arms, &color_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_multiple_variants_missing_one() {
        let color_ty = make_enum_type(
            "Color",
            vec![
                ("Red", EnumVariantType::Unit),
                ("Green", EnumVariantType::Unit),
                ("Blue", EnumVariantType::Unit),
            ],
        );
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Red"]),
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Green"]),
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &color_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(std::slice::from_ref(&color_ty));
                assert!(missing.contains("Blue"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_enum_struct_variant_exhaustive() {
        let msg_ty = make_enum_type(
            "Message",
            vec![
                ("Quit", EnumVariantType::Unit),
                (
                    "Move",
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
            ],
        );
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Message", "Quit"]),
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumStructExact {
                    path: make_qualified_path(vec!["Message", "Move"]),
                    fields: vec![
                        ("x".to_string(), TypedPattern::Wildcard),
                        ("y".to_string(), TypedPattern::Wildcard),
                    ],
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &msg_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_struct_partial_exhaustive() {
        let msg_ty = make_enum_type(
            "Message",
            vec![(
                "Move",
                EnumVariantType::Struct(vec![
                    ("x".to_string(), Type::Int),
                    ("y".to_string(), Type::Int),
                ]),
            )],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumStructPartial {
                path: make_qualified_path(vec!["Message", "Move"]),
                fields: vec![("x".to_string(), TypedPattern::Wildcard)],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &msg_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_with_wildcard_exhaustive() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let arms = vec![make_wildcard_arm()];
        let result = check_exhaustiveness(&arms, &option_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== List Pattern Variant Tests ====================

    #[test]
    fn test_list_exact_exhaustive_with_wildcard() {
        // [a, b] only matches 2-element lists, need wildcard for others
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::ListExact {
                    patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                    len: 2,
                },
                result: TypedExpr::Int(0),
            },
            make_wildcard_arm(),
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_list_exact_not_exhaustive() {
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::ListExact {
                patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                len: 2,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    #[test]
    fn test_list_suffix_exhaustive() {
        // [.., x] matches non-empty, [] matches empty
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![
            make_list_empty_arm(),
            TypedMatchArm {
                pattern: TypedPattern::ListSuffix {
                    patterns: vec![TypedPattern::Wildcard],
                    rest_binding: None,
                    min_len: 1,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_list_prefix_suffix_exhaustive() {
        // [a, .., z] matches 2+ elements, need patterns for 0 and 1 element
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![
            make_list_empty_arm(),
            TypedMatchArm {
                pattern: TypedPattern::ListExact {
                    patterns: vec![TypedPattern::Wildcard],
                    len: 1,
                },
                result: TypedExpr::Int(1),
            },
            TypedMatchArm {
                pattern: TypedPattern::ListPrefixSuffix {
                    prefix: vec![TypedPattern::Wildcard],
                    suffix: vec![TypedPattern::Wildcard],
                    rest_binding: None,
                    min_len: 2,
                },
                result: TypedExpr::Int(2),
            },
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== Tuple Pattern Variant Tests ====================

    #[test]
    fn test_tuple_empty_exhaustive() {
        let tuple_ty = Type::Tuple(vec![]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleEmpty,
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_tuple_prefix_exhaustive() {
        let tuple_ty = Type::Tuple(vec![Type::Int, Type::Int, Type::Int]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TuplePrefix {
                patterns: vec![TypedPattern::Wildcard],
                rest_binding: None,
                total_len: 3,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_tuple_suffix_exhaustive() {
        let tuple_ty = Type::Tuple(vec![Type::Int, Type::Int, Type::Int]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleSuffix {
                patterns: vec![TypedPattern::Wildcard],
                rest_binding: None,
                total_len: 3,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_tuple_prefix_suffix_exhaustive() {
        let tuple_ty = Type::Tuple(vec![Type::Int, Type::Int, Type::Int, Type::Int]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TuplePrefixSuffix {
                prefix: vec![TypedPattern::Wildcard],
                suffix: vec![TypedPattern::Wildcard],
                rest_binding: None,
                total_len: 4,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_tuple_prefix_with_bool_not_exhaustive() {
        let tuple_ty = Type::Tuple(vec![Type::Bool, Type::Int, Type::Int]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TuplePrefix {
                patterns: vec![TypedPattern::Literal(TypedExpr::Bool(true))],
                rest_binding: None,
                total_len: 3,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    // ==================== Float/String/BigInt Literal Tests ====================

    #[test]
    fn test_float_exhaustive_with_wildcard() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Float(1.0)),
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Float(2.0)),
                result: TypedExpr::Int(1),
            },
            make_wildcard_arm(),
        ];
        let result = check_exhaustiveness(&arms, &Type::Float, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_float_not_exhaustive_without_wildcard() {
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::Literal(TypedExpr::Float(1.0)),
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &Type::Float, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    #[test]
    fn test_string_exhaustive_with_wildcard() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::String("hello".to_string())),
                result: TypedExpr::Int(0),
            },
            make_wildcard_arm(),
        ];
        let result = check_exhaustiveness(&arms, &Type::String, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_string_not_exhaustive_without_wildcard() {
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::Literal(TypedExpr::String("hello".to_string())),
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &Type::String, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    #[test]
    fn test_bigint_exhaustive_with_wildcard() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::BigInt(0)),
                result: TypedExpr::Int(0),
            },
            make_wildcard_arm(),
        ];
        let result = check_exhaustiveness(&arms, &Type::BigInt, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_bigint_not_exhaustive_without_wildcard() {
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::Literal(TypedExpr::BigInt(0)),
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &Type::BigInt, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    // ==================== As Pattern Tests ====================

    #[test]
    fn test_as_pattern_exhaustive() {
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::As {
                name: "n".to_string(),
                ty: Type::Int,
                pattern: Box::new(TypedPattern::Wildcard),
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &Type::Int, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_as_pattern_with_nested_bool() {
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::As {
                name: "n".to_string(),
                ty: Type::Bool,
                pattern: Box::new(TypedPattern::Literal(TypedExpr::Bool(true))),
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &Type::Bool, &DefinitionLookup::empty());
        // Only matches true, not false
        assert!(matches!(result, Exhaustiveness::NonExhaustive(_)));
    }

    #[test]
    fn test_as_pattern_with_list() {
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![
            make_list_empty_arm(),
            TypedMatchArm {
                pattern: TypedPattern::As {
                    name: "xs".to_string(),
                    ty: list_ty.clone(),
                    pattern: Box::new(TypedPattern::ListPrefix {
                        patterns: vec![TypedPattern::Wildcard],
                        rest_binding: None,
                        min_len: 1,
                    }),
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== Nested Pattern Tests ====================

    #[test]
    fn test_nested_tuple_in_list() {
        let tuple_ty = Type::Tuple(vec![Type::Int, Type::Bool]);
        let list_ty = Type::List(Box::new(tuple_ty.clone()));
        let arms = vec![
            make_list_empty_arm(),
            TypedMatchArm {
                pattern: TypedPattern::ListPrefix {
                    patterns: vec![TypedPattern::TupleExact {
                        patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                        len: 2,
                    }],
                    rest_binding: None,
                    min_len: 1,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_nested_list_in_tuple() {
        let list_ty = Type::List(Box::new(Type::Int));
        let tuple_ty = Type::Tuple(vec![list_ty.clone(), Type::Bool]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleExact {
                patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                len: 2,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_nested_enum_in_tuple() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let tuple_ty = Type::Tuple(vec![option_ty.clone(), Type::Int]);
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::TupleExact {
                    patterns: vec![
                        TypedPattern::EnumUnit {
                            path: make_qualified_path(vec!["Option", "None"]),
                        },
                        TypedPattern::Wildcard,
                    ],
                    len: 2,
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::TupleExact {
                    patterns: vec![
                        TypedPattern::EnumTupleExact {
                            path: make_qualified_path(vec!["Option", "Some"]),
                            patterns: vec![TypedPattern::Wildcard],
                            total_fields: 1,
                        },
                        TypedPattern::Wildcard,
                    ],
                    len: 2,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== Usefulness Edge Cases ====================

    #[test]
    fn test_unreachable_multiple_arms() {
        let arms = vec![
            make_wildcard_arm(),
            make_bool_arm(true),
            make_bool_arm(false),
        ];
        let unreachable = check_usefulness(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1, 2]);
    }

    #[test]
    fn test_unreachable_enum_after_wildcard() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let arms = vec![
            make_wildcard_arm(),
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Option", "None"]),
                },
                result: TypedExpr::Int(1),
            },
        ];
        let unreachable = check_usefulness(&arms, &option_ty, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1]);
    }

    #[test]
    fn test_unreachable_duplicate_enum_variant() {
        let color_ty = make_enum_type(
            "Color",
            vec![
                ("Red", EnumVariantType::Unit),
                ("Green", EnumVariantType::Unit),
            ],
        );
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Red"]),
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Red"]),
                },
                result: TypedExpr::Int(1),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Color", "Green"]),
                },
                result: TypedExpr::Int(2),
            },
        ];
        let unreachable = check_usefulness(&arms, &color_ty, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1]);
    }

    // ==================== Witness Generation Tests ====================

    #[test]
    fn test_witness_for_missing_enum_variant() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumTupleExact {
                path: make_qualified_path(vec!["Option", "Some"]),
                patterns: vec![TypedPattern::Wildcard],
                total_fields: 1,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &option_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                let missing = witnesses[0].to_string(std::slice::from_ref(&option_ty));
                assert!(missing.contains("None"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_witness_for_tuple_with_missing_bool() {
        let tuple_ty = Type::Tuple(vec![Type::Bool, Type::Bool]);
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::TupleExact {
                    patterns: vec![
                        TypedPattern::Literal(TypedExpr::Bool(true)),
                        TypedPattern::Literal(TypedExpr::Bool(true)),
                    ],
                    len: 2,
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::TupleExact {
                    patterns: vec![
                        TypedPattern::Literal(TypedExpr::Bool(false)),
                        TypedPattern::Literal(TypedExpr::Bool(false)),
                    ],
                    len: 2,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                assert!(!witnesses.is_empty());
                // Should show something like (true, false) or (false, true)
                let missing = witnesses[0].to_string(std::slice::from_ref(&tuple_ty));
                assert!(missing.contains("true") || missing.contains("false"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_single_wildcard_covers_struct() {
        let struct_ty = make_struct_type("Point", vec![("x", Type::Int), ("y", Type::Int)]);
        let arms = vec![make_wildcard_arm()];
        let result = check_exhaustiveness(&arms, &struct_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_var_pattern_covers_all() {
        let arms = vec![make_var_arm("x", Type::Int)];
        let result = check_exhaustiveness(&arms, &Type::Int, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== EnumTupleSuffix and EnumTuplePrefixSuffix Tests ====================

    #[test]
    fn test_enum_tuple_suffix_exhaustive() {
        let result_ty = make_enum_type(
            "Result",
            vec![
                ("Ok", EnumVariantType::Tuple(vec![Type::Int, Type::Int])),
                ("Err", EnumVariantType::Tuple(vec![Type::String])),
            ],
        );
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumTupleSuffix {
                    path: make_qualified_path(vec!["Result", "Ok"]),
                    patterns: vec![TypedPattern::Wildcard], // matches second field
                    rest_binding: None,
                    total_fields: 2,
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumTupleExact {
                    path: make_qualified_path(vec!["Result", "Err"]),
                    patterns: vec![TypedPattern::Wildcard],
                    total_fields: 1,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &result_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_tuple_prefix_suffix_exhaustive() {
        let triple_ty = make_enum_type(
            "Triple",
            vec![(
                "Make",
                EnumVariantType::Tuple(vec![Type::Int, Type::Int, Type::Int]),
            )],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumTuplePrefixSuffix {
                path: make_qualified_path(vec!["Triple", "Make"]),
                prefix: vec![TypedPattern::Wildcard],
                suffix: vec![TypedPattern::Wildcard],
                rest_binding: None,
                total_fields: 3,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &triple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_tuple_prefix_exhaustive() {
        let pair_ty = make_enum_type(
            "Pair",
            vec![("Make", EnumVariantType::Tuple(vec![Type::Int, Type::Int]))],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumTuplePrefix {
                path: make_qualified_path(vec!["Pair", "Make"]),
                patterns: vec![TypedPattern::Wildcard],
                rest_binding: None,
                total_fields: 2,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &pair_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== More Witness Display Tests ====================

    #[test]
    fn test_witness_struct_display() {
        let struct_ty = make_struct_type("Flags", vec![("enabled", Type::Bool)]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::StructExact {
                path: make_qualified_path(vec!["Flags"]),
                fields: vec![(
                    "enabled".to_string(),
                    TypedPattern::Literal(TypedExpr::Bool(true)),
                )],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &struct_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                let missing = witnesses[0].to_string(std::slice::from_ref(&struct_ty));
                assert!(missing.contains("Flags"));
                assert!(missing.contains("false"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_witness_enum_tuple_display() {
        let option_ty = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumUnit {
                path: make_qualified_path(vec!["Option", "None"]),
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &option_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                let missing = witnesses[0].to_string(std::slice::from_ref(&option_ty));
                assert!(missing.contains("Some"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    #[test]
    fn test_witness_enum_struct_display() {
        let msg_ty = make_enum_type(
            "Message",
            vec![
                ("Quit", EnumVariantType::Unit),
                (
                    "Move",
                    EnumVariantType::Struct(vec![
                        ("x".to_string(), Type::Int),
                        ("y".to_string(), Type::Int),
                    ]),
                ),
            ],
        );
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumUnit {
                path: make_qualified_path(vec!["Message", "Quit"]),
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &msg_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                let missing = witnesses[0].to_string(std::slice::from_ref(&msg_ty));
                assert!(missing.contains("Move"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    // ==================== Single-element Tuple Tests ====================

    #[test]
    fn test_tuple_single_element_exhaustive() {
        let tuple_ty = Type::Tuple(vec![Type::Int]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleExact {
                patterns: vec![TypedPattern::Wildcard],
                len: 1,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_tuple_single_bool_not_exhaustive() {
        let tuple_ty = Type::Tuple(vec![Type::Bool]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::TupleExact {
                patterns: vec![TypedPattern::Literal(TypedExpr::Bool(true))],
                len: 1,
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &tuple_ty, &DefinitionLookup::empty());
        match result {
            Exhaustiveness::NonExhaustive(witnesses) => {
                let missing = witnesses[0].to_string(std::slice::from_ref(&tuple_ty));
                // Single element tuple should display as (false,)
                assert!(missing.contains("false"));
            }
            _ => panic!("expected non-exhaustive"),
        }
    }

    // ==================== Empty Struct Tests ====================

    #[test]
    fn test_struct_empty_fields_exhaustive() {
        let struct_ty = Type::Struct {
            name: "Unit".to_string(),
            type_args: vec![],
            fields: vec![],
        };
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::StructExact {
                path: make_qualified_path(vec!["Unit"]),
                fields: vec![],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &struct_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_enum_struct_empty_fields_exhaustive() {
        let msg_ty = make_enum_type("Message", vec![("Empty", EnumVariantType::Struct(vec![]))]);
        let arms = vec![TypedMatchArm {
            pattern: TypedPattern::EnumStructExact {
                path: make_qualified_path(vec!["Message", "Empty"]),
                fields: vec![],
            },
            result: TypedExpr::Int(0),
        }];
        let result = check_exhaustiveness(&arms, &msg_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== List with Literals Tests ====================

    #[test]
    fn test_list_with_literal_patterns() {
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![
            make_list_empty_arm(),
            TypedMatchArm {
                pattern: TypedPattern::ListPrefix {
                    patterns: vec![TypedPattern::Literal(TypedExpr::Int(0))],
                    rest_binding: None,
                    min_len: 1,
                },
                result: TypedExpr::Int(1),
            },
            make_wildcard_arm(),
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== TypeCtors Tests ====================

    #[test]
    fn test_type_ctors_function_type() {
        // Function types are treated as Infinite (conservative)
        let func_ty = Type::Function {
            params: vec![Type::Int],
            ret: Box::new(Type::Bool),
        };
        let arms = vec![make_wildcard_arm()];
        let result = check_exhaustiveness(&arms, &func_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_type_ctors_type_var() {
        // Type variables are treated as Infinite (conservative)
        let var_ty = Type::Var(TypeVarId(0));
        let arms = vec![make_wildcard_arm()];
        let result = check_exhaustiveness(&arms, &var_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== Complex Nested Patterns ====================

    #[test]
    fn test_nested_struct_in_enum() {
        let point_ty = make_struct_type("Point", vec![("x", Type::Int), ("y", Type::Int)]);
        let shape_ty = Type::Enum {
            name: "Shape".to_string(),
            type_args: vec![],
            variants: vec![
                (
                    "Circle".to_string(),
                    EnumVariantType::Tuple(vec![point_ty.clone(), Type::Int]),
                ),
                (
                    "Rectangle".to_string(),
                    EnumVariantType::Tuple(vec![point_ty.clone(), point_ty.clone()]),
                ),
            ],
        };
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumTupleExact {
                    path: make_qualified_path(vec!["Shape", "Circle"]),
                    patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                    total_fields: 2,
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumTupleExact {
                    path: make_qualified_path(vec!["Shape", "Rectangle"]),
                    patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                    total_fields: 2,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &shape_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    #[test]
    fn test_nested_option_in_option() {
        let inner_option = make_enum_type(
            "Option",
            vec![
                ("None", EnumVariantType::Unit),
                ("Some", EnumVariantType::Tuple(vec![Type::Int])),
            ],
        );
        let outer_option = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![],
            variants: vec![
                ("None".to_string(), EnumVariantType::Unit),
                (
                    "Some".to_string(),
                    EnumVariantType::Tuple(vec![inner_option.clone()]),
                ),
            ],
        };
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::EnumUnit {
                    path: make_qualified_path(vec!["Option", "None"]),
                },
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::EnumTupleExact {
                    path: make_qualified_path(vec!["Option", "Some"]),
                    patterns: vec![TypedPattern::Wildcard],
                    total_fields: 1,
                },
                result: TypedExpr::Int(1),
            },
        ];
        let result = check_exhaustiveness(&arms, &outer_option, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== List Exact Length Tests ====================

    #[test]
    fn test_list_exact_lengths_exhaustive() {
        // Matching exactly 0, 1, and 2+ elements
        let list_ty = Type::List(Box::new(Type::Int));
        let arms = vec![
            make_list_empty_arm(),
            TypedMatchArm {
                pattern: TypedPattern::ListExact {
                    patterns: vec![TypedPattern::Wildcard],
                    len: 1,
                },
                result: TypedExpr::Int(1),
            },
            TypedMatchArm {
                pattern: TypedPattern::ListPrefix {
                    patterns: vec![TypedPattern::Wildcard, TypedPattern::Wildcard],
                    rest_binding: None,
                    min_len: 2,
                },
                result: TypedExpr::Int(2),
            },
        ];
        let result = check_exhaustiveness(&arms, &list_ty, &DefinitionLookup::empty());
        assert!(matches!(result, Exhaustiveness::Exhaustive));
    }

    // ==================== OrderedFloat Tests ====================

    #[test]
    fn test_float_duplicate_detection() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Float(1.5)),
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Float(1.5)), // duplicate
                result: TypedExpr::Int(1),
            },
            make_wildcard_arm(),
        ];
        let unreachable = check_usefulness(&arms, &Type::Float, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1]);
    }

    #[test]
    fn test_float_different_values_reachable() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Float(1.0)),
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::Float(2.0)),
                result: TypedExpr::Int(1),
            },
            make_wildcard_arm(),
        ];
        let unreachable = check_usefulness(&arms, &Type::Float, &DefinitionLookup::empty());
        assert!(unreachable.is_empty());
    }

    // ==================== String Duplicate Tests ====================

    #[test]
    fn test_string_duplicate_detection() {
        let arms = vec![
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::String("hello".to_string())),
                result: TypedExpr::Int(0),
            },
            TypedMatchArm {
                pattern: TypedPattern::Literal(TypedExpr::String("hello".to_string())), // duplicate
                result: TypedExpr::Int(1),
            },
            make_wildcard_arm(),
        ];
        let unreachable = check_usefulness(&arms, &Type::String, &DefinitionLookup::empty());
        assert_eq!(unreachable, vec![1]);
    }

    // ==================== check_patterns Integration Tests ====================

    #[test]
    fn test_check_patterns_exhaustive_no_unreachable() {
        let arms = vec![make_bool_arm(true), make_bool_arm(false)];
        let result = check_patterns(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_patterns_non_exhaustive_error() {
        let arms = vec![make_bool_arm(true)];
        let result = check_patterns(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("non-exhaustive"));
        assert!(err.message.contains("false"));
    }

    #[test]
    fn test_check_patterns_unreachable_error() {
        let arms = vec![make_wildcard_arm(), make_bool_arm(true)];
        let result = check_patterns(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unreachable"));
    }

    #[test]
    fn test_check_patterns_multiple_unreachable() {
        let arms = vec![
            make_wildcard_arm(),
            make_bool_arm(true),
            make_bool_arm(false),
        ];
        let result = check_patterns(&arms, &Type::Bool, &DefinitionLookup::empty());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unreachable"));
        // Should mention both patterns 2 and 3
        assert!(err.message.contains("2") && err.message.contains("3"));
    }

    // ==================== DefinitionLookup Tests ====================

    #[test]
    fn test_lookup_resolves_empty_enum_variants() {
        // Simulate a recursive type stub: enum with empty variants
        let stub_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![],
            variants: vec![], // empty stub
        };

        // Build a lookup with the real definition
        let mut lookup = DefinitionLookup::empty();
        lookup.enums.insert(
            "Option".to_string(),
            (
                vec![],
                vec![
                    ("None".to_string(), EnumVariantType::Unit),
                    ("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int])),
                ],
            ),
        );

        // from_type should return the real constructors
        let ctors = TypeCtors::from_type(&stub_ty, &lookup);
        match ctors {
            TypeCtors::Finite(cs) => {
                assert_eq!(cs.len(), 2);
            }
            _ => panic!("expected Finite"),
        }
    }

    #[test]
    fn test_lookup_resolves_empty_struct_fields() {
        // Simulate a recursive type stub: struct with empty fields
        let stub_ty = Type::Struct {
            name: "Node".to_string(),
            type_args: vec![],
            fields: vec![], // empty stub
        };

        // Build a lookup with the real definition
        let mut lookup = DefinitionLookup::empty();
        lookup.structs.insert(
            "Node".to_string(),
            (
                vec![],
                vec![
                    ("value".to_string(), Type::Int),
                    (
                        "children".to_string(),
                        Type::List(Box::new(stub_ty.clone())),
                    ),
                ],
            ),
        );

        // from_type should return the real constructor with correct arity
        let ctors = TypeCtors::from_type(&stub_ty, &lookup);
        match ctors {
            TypeCtors::Finite(cs) => {
                assert_eq!(cs.len(), 1);
                assert_eq!(cs[0].arity(&stub_ty), 2);
            }
            _ => panic!("expected Finite"),
        }
    }

    #[test]
    fn test_lookup_skips_non_empty_variants() {
        // Enum already has variants — lookup should NOT override them
        let full_ty = make_enum_type(
            "Color",
            vec![
                ("Red", EnumVariantType::Unit),
                ("Green", EnumVariantType::Unit),
            ],
        );

        let mut lookup = DefinitionLookup::empty();
        lookup.enums.insert(
            "Color".to_string(),
            (
                vec![],
                vec![
                    ("Red".to_string(), EnumVariantType::Unit),
                    ("Green".to_string(), EnumVariantType::Unit),
                    ("Blue".to_string(), EnumVariantType::Unit),
                ],
            ),
        );

        let ctors = TypeCtors::from_type(&full_ty, &lookup);
        match ctors {
            TypeCtors::Finite(cs) => {
                // Should use the 2-variant type, not the 3-variant lookup
                assert_eq!(cs.len(), 2);
            }
            _ => panic!("expected Finite"),
        }
    }

    #[test]
    fn test_lookup_resolves_generic_enum() {
        // Generic enum stub with type args
        let stub_ty = Type::Enum {
            name: "Option".to_string(),
            type_args: vec![Type::String],
            variants: vec![], // empty stub
        };

        let type_var = TypeVarId(99);
        let mut lookup = DefinitionLookup::empty();
        lookup.enums.insert(
            "Option".to_string(),
            (
                vec![type_var],
                vec![
                    ("None".to_string(), EnumVariantType::Unit),
                    (
                        "Some".to_string(),
                        EnumVariantType::Tuple(vec![Type::Var(type_var)]),
                    ),
                ],
            ),
        );

        let ctors = TypeCtors::from_type(&stub_ty, &lookup);
        match ctors {
            TypeCtors::Finite(cs) => {
                assert_eq!(cs.len(), 2);
                // The Some variant should have String field type (substituted)
                if let Constructor::EnumVariant {
                    kind: EnumVariantConstructorKind::Tuple { field_types, .. },
                    ..
                } = &cs[1]
                {
                    assert_eq!(field_types, &[Type::String]);
                } else {
                    panic!("expected Some to be a Tuple variant");
                }
            }
            _ => panic!("expected Finite"),
        }
    }
}
