//! Module path resolution for cross-module function calls.
//!
//! Path resolution is purely structural - no TypeEnv lookup needed.
//! The actual lookup happens after resolution.

use std::collections::HashMap;

use zoya_ast::{Path, PathPrefix};
use zoya_ir::{Definition, QualifiedPath, TypeError, TypeScheme, Visibility};


/// Resolve an AST path to a fully qualified path.
///
/// # Path Resolution Rules
///
/// | Path | Resolution |
/// |------|------------|
/// | `foo()` | Check locals, then current module for `foo` |
/// | `foo::bar()` | Look for `bar` in child module `foo` (relative path) |
/// | `root::foo()` | Absolute path from root module |
/// | `self::foo()` | Explicit current module reference |
/// | `super::foo()` | Parent module reference |
pub fn resolve_path(path: &Path, current_module: &QualifiedPath) -> Result<QualifiedPath, TypeError> {
    match &path.prefix {
        PathPrefix::Root => {
            // root::foo::bar → root::foo::bar
            let mut segments = vec!["root".to_string()];
            segments.extend(path.segments.iter().cloned());
            Ok(QualifiedPath::new(segments))
        }
        PathPrefix::Self_ => {
            // self::foo → current_module::foo
            let mut segments = current_module
                .segments()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            segments.extend(path.segments.iter().cloned());
            Ok(QualifiedPath::new(segments))
        }
        PathPrefix::Super => {
            // super::foo → parent_module::foo
            let parent = current_module.parent().ok_or_else(|| TypeError {
                message: "cannot use super:: in root module".to_string(),
            })?;
            let mut segments = parent
                .segments()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            segments.extend(path.segments.iter().cloned());
            Ok(QualifiedPath::new(segments))
        }
        PathPrefix::Package(name) => {
            // std::option::Option → std::option::Option
            let mut segments = vec![name.clone()];
            segments.extend(path.segments.iter().cloned());
            Ok(QualifiedPath::new(segments))
        }
        PathPrefix::None => {
            // Relative path: check current module or child module
            resolve_relative_path(path, current_module)
        }
    }
}

/// Resolve a relative path (no prefix) to a fully qualified path.
fn resolve_relative_path(
    path: &Path,
    current_module: &QualifiedPath,
) -> Result<QualifiedPath, TypeError> {
    if path.segments.is_empty() {
        return Err(TypeError {
            message: "empty path".to_string(),
        });
    }

    // Build segments from current module + path segments
    let mut segments = current_module
        .segments()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    segments.extend(path.segments.iter().cloned());
    Ok(QualifiedPath::new(segments))
}

/// Check if an item is visible from the accessor module.
///
/// Visibility rules:
/// - Public items are always visible
/// - Private items are visible if the accessor is in the same module or a descendant
fn check_item_visibility(
    def: &Definition,
    item_name: &str,
    accessor_module: &QualifiedPath,
) -> Result<(), TypeError> {
    let visibility = def.visibility();

    if visibility == Visibility::Public {
        return Ok(());
    }

    let target_module = def.module();
    let target_segments: Vec<&str> = target_module.segments().iter().map(|s| s.as_str()).collect();

    let accessor: Vec<&str> = accessor_module
        .segments()
        .iter()
        .map(|s| s.as_str())
        .collect();

    // Private visible if accessor is same module or descendant
    let is_visible =
        accessor.len() >= target_segments.len() && accessor[..target_segments.len()] == target_segments[..];

    if is_visible {
        Ok(())
    } else {
        Err(TypeError {
            message: format!(
                "{} '{}' is private to module '{}'",
                def.kind_name(),
                item_name,
                target_module,
            ),
        })
    }
}

/// Check that all intermediate modules in a qualified path are visible from the accessor module.
///
/// For a path like `root::a::b::item`, this checks that module `a` (declared in `root`)
/// and module `b` (declared in `root::a`) are both visible from the accessor.
///
/// A private module is visible from its declaring (parent) module and all descendants.
fn check_module_path_visible(
    qualified: &QualifiedPath,
    accessor_module: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
) -> Result<(), TypeError> {
    let segments = qualified.segments();

    // We need at least 3 segments (root::module::item) to have an intermediate module to check.
    // For each intermediate module segment (not root, not the final item), check visibility.
    // Segments: [root, mod1, mod2, ..., item]
    // We check mod1, mod2, etc. - each is a child of the previous module.
    if segments.len() < 3 {
        return Ok(());
    }

    // Check each intermediate module (segments[1] through segments[len-2])
    for i in 1..segments.len() - 1 {
        let parent_module_path = QualifiedPath::new(segments[0..i].to_vec());
        let child_name = &segments[i];
        let child_module_path = parent_module_path.child(child_name);

        if let Some(Definition::Module(m)) = definitions.get(&child_module_path) {
            if m.visibility == Visibility::Public {
                continue;
            }

            // Private module: check if accessor is the declaring module or a descendant
            let accessor: Vec<&str> = accessor_module
                .segments()
                .iter()
                .map(|s| s.as_str())
                .collect();
            let parent_segments: Vec<&str> =
                parent_module_path.segments().iter().map(|s| s.as_str()).collect();

            let is_visible = accessor.len() >= parent_segments.len()
                && accessor[..parent_segments.len()] == parent_segments[..];

            if !is_visible {
                return Err(TypeError {
                    message: format!(
                        "module '{}' is private to module '{}'",
                        child_name, parent_module_path
                    ),
                });
            }
        }
    }

    Ok(())
}

/// Result of resolving a path in expression context
#[derive(Debug)]
pub enum ResolvedPath<'a> {
    /// Local variable from env.locals
    Local {
        name: String,
        scheme: &'a TypeScheme,
    },
    /// Top-level definition (function, struct, enum, type alias)
    Definition {
        qualified_path: QualifiedPath,
        def: &'a Definition,
    },
}

/// Per-module import table type alias
pub type ImportTable = HashMap<String, QualifiedPath>;

/// Resolve a path in expression context.
///
/// This handles:
/// 1. Single-segment paths without prefix: check locals first, then imports, then definitions
/// 2. Multi-segment paths: resolve as qualified name, then try as enum variant
/// 3. Paths with prefixes (root::, self::, super::)
///
/// Priority order for single-segment paths:
/// 1. Locals (let bindings, function parameters)
/// 2. Imports (use declarations)
/// 3. Module-level definitions (functions, types in current module)
pub fn resolve_expr_path<'a>(
    path: &Path,
    current_module: &QualifiedPath,
    locals: &'a HashMap<String, TypeScheme>,
    imports: &'a HashMap<QualifiedPath, ImportTable>,
    definitions: &'a HashMap<QualifiedPath, Definition>,
    reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Result<ResolvedPath<'a>, TypeError> {
    // Single-segment path with no prefix: check locals first, then imports
    if path.prefix == PathPrefix::None && path.segments.len() == 1 {
        let name = &path.segments[0];

        // Priority 1: Locals
        if let Some(scheme) = locals.get(name) {
            return Ok(ResolvedPath::Local {
                name: name.clone(),
                scheme,
            });
        }

        // Priority 2: Imports
        if let Some(item_imports) = imports.get(current_module)
            && let Some(qualified) = item_imports.get(name)
            && let Some(def) = definitions.get(qualified)
        {
            // Follow re-export chain to the original definition
            let canonical = follow_reexports(qualified, reexports);
            let canonical_def = definitions.get(&canonical).unwrap_or(def);
            return Ok(ResolvedPath::Definition {
                qualified_path: canonical,
                def: canonical_def,
            });
        }
    }

    // Multi-segment path with no prefix: try item imports and module imports
    if path.prefix == PathPrefix::None && path.segments.len() > 1 {
        let first = &path.segments[0];

        // Try imports: e.g., `Color::Red` where `Color` is imported, or `bar::add` where `bar` is a module import
        if let Some(item_imports) = imports.get(current_module)
            && let Some(imported_path) = item_imports.get(first)
        {
            let canonical_base = follow_reexports(imported_path, reexports);
            let mut segments = canonical_base.segments().to_vec();
            segments.extend(path.segments[1..].iter().cloned());
            let qualified_path = QualifiedPath::new(segments);

            if let Some(def) = definitions.get(&qualified_path) {
                check_module_path_visible(&qualified_path, current_module, definitions)?;
                check_item_visibility(
                    def,
                    qualified_path.last(),
                    current_module,
                )?;
                let canonical = follow_reexports(&qualified_path, reexports);
                let canonical_def = definitions.get(&canonical).unwrap_or(def);
                return Ok(ResolvedPath::Definition {
                    qualified_path: canonical,
                    def: canonical_def,
                });
            }
        }
    }

    // Priority 3: Resolve the full path in module-level definitions
    let qualified_path = resolve_path(path, current_module)?;

    // Try exact match in definitions
    if let Some(def) = definitions.get(&qualified_path) {
        check_module_path_visible(&qualified_path, current_module, definitions)?;
        check_item_visibility(def, qualified_path.last(), current_module)?;
        // Follow re-export chain to the original definition
        let canonical = follow_reexports(&qualified_path, reexports);
        let canonical_def = definitions.get(&canonical).unwrap_or(def);
        return Ok(ResolvedPath::Definition {
            qualified_path: canonical,
            def: canonical_def,
        });
    }

    // Priority 4: Package fallback — treat raw segments as a package-rooted path
    // e.g., std::option::Some → QualifiedPath(["std", "option", "Some"])
    if path.prefix == PathPrefix::None && path.segments.len() > 1 {
        let package_path = QualifiedPath::new(path.segments.clone());
        if let Some(def) = definitions.get(&package_path) {
            check_module_path_visible(&package_path, current_module, definitions)?;
            check_item_visibility(def, package_path.last(), current_module)?;
            let canonical = follow_reexports(&package_path, reexports);
            let canonical_def = definitions.get(&canonical).unwrap_or(def);
            return Ok(ResolvedPath::Definition {
                qualified_path: canonical,
                def: canonical_def,
            });
        }
    }

    // Nothing found - generate appropriate error
    if path.segments.len() == 1 {
        Err(TypeError {
            message: format!("unknown identifier: {}", path.segments[0]),
        })
    } else {
        Err(TypeError {
            message: format!("unknown path: {}", path),
        })
    }
}

/// Follow re-export chain to the original definition path.
fn follow_reexports(
    path: &QualifiedPath,
    reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> QualifiedPath {
    let mut current = path.clone();
    // Follow re-export chain (supports transitive re-exports)
    while let Some(original) = reexports.get(&current) {
        current = original.clone();
    }
    current
}

/// Resolve a path in pattern context (no locals, only imports and definitions).
///
/// This is similar to `resolve_expr_path` but doesn't check locals since patterns
/// don't have access to local variables.
pub fn resolve_pattern_path<'a>(
    path: &Path,
    current_module: &QualifiedPath,
    imports: &'a HashMap<QualifiedPath, ImportTable>,
    definitions: &'a HashMap<QualifiedPath, Definition>,
    reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Result<ResolvedPath<'a>, TypeError> {
    // For single-segment paths without prefix, check imports first
    if path.prefix == PathPrefix::None && path.segments.len() == 1 {
        let name = &path.segments[0];

        // Check imports
        if let Some(item_imports) = imports.get(current_module)
            && let Some(qualified) = item_imports.get(name)
            && let Some(def) = definitions.get(qualified)
        {
            // Follow re-export chain to the original definition
            let canonical = follow_reexports(qualified, reexports);
            let canonical_def = definitions.get(&canonical).unwrap_or(def);
            return Ok(ResolvedPath::Definition {
                qualified_path: canonical,
                def: canonical_def,
            });
        }
    }

    // Multi-segment path with no prefix: try item imports and module imports
    if path.prefix == PathPrefix::None && path.segments.len() > 1 {
        let first = &path.segments[0];

        // Try imports: e.g., `Color::Red` where `Color` is imported, or `bar::add` where `bar` is a module import
        if let Some(item_imports) = imports.get(current_module)
            && let Some(imported_path) = item_imports.get(first)
        {
            let canonical_base = follow_reexports(imported_path, reexports);
            let mut segments = canonical_base.segments().to_vec();
            segments.extend(path.segments[1..].iter().cloned());
            let qualified_path = QualifiedPath::new(segments);

            if let Some(def) = definitions.get(&qualified_path) {
                check_module_path_visible(&qualified_path, current_module, definitions)?;
                check_item_visibility(
                    def,
                    qualified_path.last(),
                    current_module,
                )?;
                let canonical = follow_reexports(&qualified_path, reexports);
                let canonical_def = definitions.get(&canonical).unwrap_or(def);
                return Ok(ResolvedPath::Definition {
                    qualified_path: canonical,
                    def: canonical_def,
                });
            }
        }
    }

    // Resolve the full path
    let qualified_path = resolve_path(path, current_module)?;

    // Try exact match in definitions
    if let Some(def) = definitions.get(&qualified_path) {
        check_module_path_visible(&qualified_path, current_module, definitions)?;
        check_item_visibility(def, qualified_path.last(), current_module)?;
        // Follow re-export chain to the original definition
        let canonical = follow_reexports(&qualified_path, reexports);
        let canonical_def = definitions.get(&canonical).unwrap_or(def);
        return Ok(ResolvedPath::Definition {
            qualified_path: canonical,
            def: canonical_def,
        });
    }

    // Priority 4: Package fallback — treat raw segments as a package-rooted path
    // e.g., std::option::Some → QualifiedPath(["std", "option", "Some"])
    if path.prefix == PathPrefix::None && path.segments.len() > 1 {
        let package_path = QualifiedPath::new(path.segments.clone());
        if let Some(def) = definitions.get(&package_path) {
            check_module_path_visible(&package_path, current_module, definitions)?;
            check_item_visibility(def, package_path.last(), current_module)?;
            let canonical = follow_reexports(&package_path, reexports);
            let canonical_def = definitions.get(&canonical).unwrap_or(def);
            return Ok(ResolvedPath::Definition {
                qualified_path: canonical,
                def: canonical_def,
            });
        }
    }

    // Nothing found
    if path.segments.len() == 1 {
        Err(TypeError {
            message: format!("unknown identifier: {}", path.segments[0]),
        })
    } else {
        Err(TypeError {
            message: format!("unknown path: {}", path),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::Path as AstPath;

    fn path_from_segments(prefix: PathPrefix, segments: &[&str]) -> AstPath {
        AstPath {
            prefix,
            segments: segments.iter().map(|s| s.to_string()).collect(),
            type_args: None,
        }
    }

    #[test]
    fn test_resolve_simple_path_in_root() {
        let path = path_from_segments(PathPrefix::None, &["foo"]);
        let current = QualifiedPath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo");
    }

    #[test]
    fn test_resolve_simple_path_in_nested_module() {
        let path = path_from_segments(PathPrefix::None, &["foo"]);
        let current = QualifiedPath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_resolve_root_prefix() {
        let path = path_from_segments(PathPrefix::Root, &["utils", "helper"]);
        let current = QualifiedPath::root().child("other");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::helper");
    }

    #[test]
    fn test_resolve_self_prefix() {
        let path = path_from_segments(PathPrefix::Self_, &["foo"]);
        let current = QualifiedPath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_resolve_super_prefix() {
        let path = path_from_segments(PathPrefix::Super, &["foo"]);
        let current = QualifiedPath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo");
    }

    #[test]
    fn test_resolve_super_in_root_module_error() {
        let path = path_from_segments(PathPrefix::Super, &["foo"]);
        let current = QualifiedPath::root();
        let result = resolve_path(&path, &current);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("super::"));
    }

    #[test]
    fn test_resolve_qualified_relative_path() {
        let path = path_from_segments(PathPrefix::None, &["Option", "Some"]);
        let current = QualifiedPath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::Option::Some");
    }

    #[test]
    fn test_resolve_deeply_nested() {
        let path = path_from_segments(PathPrefix::Root, &["a", "b", "c", "foo"]);
        let current = QualifiedPath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::a::b::c::foo");
    }

    #[test]
    fn test_qualified_path_from_module() {
        let module = QualifiedPath::root().child("utils");
        let result = module.child("helper");
        assert_eq!(result.to_string(), "root::utils::helper");
    }

    // ========================================================================
    // Visibility Tests
    // ========================================================================

    use zoya_ir::{EnumType, EnumVariantType, FunctionType, StructType, Type, TypeAliasType, Definition};

    fn qpath(path: &str) -> QualifiedPath {
        QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
    }

    #[test]
    fn test_visibility_public_function_accessible() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: QualifiedPath::root().child("utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["utils", "helper"]);
        let current = QualifiedPath::root(); // calling from root
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_function_same_module() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["helper"]);
        let current = QualifiedPath::root().child("utils"); // calling from same module
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_function_child_can_access_parent() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: QualifiedPath::root(),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Super, &["helper"]);
        let current = QualifiedPath::root().child("utils"); // child accessing parent
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_function_parent_cannot_access_child() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["utils", "helper"]);
        let current = QualifiedPath::root(); // parent trying to access child's private
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("private"));
    }

    #[test]
    fn test_visibility_private_function_sibling_cannot_access() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("a"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "helper"]);
        let current = QualifiedPath::root().child("b"); // sibling trying to access
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("private"));
    }

    #[test]
    fn test_visibility_private_function_deep_descendant_can_access() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: QualifiedPath::root(),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["helper"]);
        let current = QualifiedPath::root().child("a").child("b").child("c"); // deeply nested
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_ok());
    }

    // ========================================================================
    // Struct/Enum/TypeAlias Visibility Tests
    // ========================================================================

    #[test]
    fn test_visibility_private_struct_from_sibling_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::Point"),
            Definition::Struct(StructType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("a"),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![],
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "Point"]);
        let current = QualifiedPath::root().child("b"); // sibling
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    #[test]
    fn test_visibility_private_enum_from_sibling_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::Color"),
            Definition::Enum(EnumType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("a"),
                name: "Color".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                variants: vec![],
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "Color"]);
        let current = QualifiedPath::root().child("b");
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    #[test]
    fn test_visibility_private_type_alias_from_sibling_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::MyInt"),
            Definition::TypeAlias(TypeAliasType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("a"),
                name: "MyInt".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                typ: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "MyInt"]);
        let current = QualifiedPath::root().child("b");
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    #[test]
    fn test_visibility_private_enum_variant_from_sibling_error() {
        let parent_enum = EnumType {
            visibility: Visibility::Private,
            module: QualifiedPath::root().child("a"),
            name: "Color".to_string(),
            type_params: vec![],
            type_var_ids: vec![],
            variants: vec![("Red".to_string(), EnumVariantType::Unit)],
        };
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::Color::Red"),
            Definition::EnumVariant(parent_enum, EnumVariantType::Unit),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "Color", "Red"]);
        let current = QualifiedPath::root().child("b");
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    #[test]
    fn test_visibility_public_struct_accessible() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::Point"),
            Definition::Struct(StructType {
                visibility: Visibility::Public,
                module: QualifiedPath::root().child("a"),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![],
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "Point"]);
        let current = QualifiedPath::root().child("b");
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_struct_from_descendant_ok() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::Point"),
            Definition::Struct(StructType {
                visibility: Visibility::Private,
                module: QualifiedPath::root(),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![],
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["Point"]);
        let current = QualifiedPath::root().child("child"); // descendant
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_struct_in_pattern_from_sibling_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::a::Point"),
            Definition::Struct(StructType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("a"),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![],
            }),
        );
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "Point"]);
        let current = QualifiedPath::root().child("b");
        let result = resolve_pattern_path(&path, &current, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    // ========================================================================
    // Import Resolution Tests
    // ========================================================================

    #[test]
    fn test_imports_take_priority_over_definitions() {
        let mut definitions = HashMap::new();
        // Two functions named 'helper' in different modules
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: QualifiedPath::root().child("utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        definitions.insert(
            qpath("root::helper"), // Would be the local one if no import
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: QualifiedPath::root(),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Bool,
            }),
        );

        let locals = HashMap::new();
        let mut imports = HashMap::new();
        let mut root_imports = ImportTable::new();
        root_imports.insert("helper".to_string(), qpath("root::utils::helper"));
        imports.insert(QualifiedPath::root(), root_imports);

        let path = path_from_segments(PathPrefix::None, &["helper"]);
        let current = QualifiedPath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new()).unwrap();

        // Should resolve to the imported version (root::utils::helper)
        match result {
            ResolvedPath::Definition { qualified_path, .. } => {
                assert_eq!(qualified_path.to_string(), "root::utils::helper");
            }
            _ => panic!("expected definition"),
        }
    }

    #[test]
    fn test_locals_take_priority_over_imports() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: QualifiedPath::root().child("utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let mut locals = HashMap::new();
        locals.insert(
            "helper".to_string(),
            TypeScheme {
                quantified: vec![],
                ty: Type::Bool,
            },
        );

        let mut imports = HashMap::new();
        let mut root_imports = ImportTable::new();
        root_imports.insert("helper".to_string(), qpath("root::utils::helper"));
        imports.insert(QualifiedPath::root(), root_imports);

        let path = path_from_segments(PathPrefix::None, &["helper"]);
        let current = QualifiedPath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new()).unwrap();

        // Should resolve to the local variable, not the import
        match result {
            ResolvedPath::Local { name, .. } => {
                assert_eq!(name, "helper");
            }
            _ => panic!("expected local"),
        }
    }

    #[test]
    fn test_unresolved_multi_segment_no_prefix_gives_unknown_path() {
        let definitions = HashMap::new();
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["serde", "Deserialize"]);
        let current = QualifiedPath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown path"));
    }

    #[test]
    fn test_resolve_package_prefix() {
        let path = path_from_segments(PathPrefix::Package("serde".to_string()), &["Deserialize"]);
        let current = QualifiedPath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "serde::Deserialize");
    }

    // ========================================================================
    // Package Fallback Tests
    // ========================================================================

    #[test]
    fn test_package_fallback_resolves_expr_path() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("std::option::Some"),
            Definition::EnumVariant(
                EnumType {
                    visibility: Visibility::Public,
                    module: qpath("std::option"),
                    name: "Option".to_string(),
                    type_params: vec![],
                    type_var_ids: vec![],
                    variants: vec![("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int]))],
                },
                EnumVariantType::Tuple(vec![Type::Int]),
            ),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        // Path: std::option::Some with PathPrefix::None (no use import)
        let path = path_from_segments(PathPrefix::None, &["std", "option", "Some"]);
        let current = QualifiedPath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new()).unwrap();
        match result {
            ResolvedPath::Definition { qualified_path, .. } => {
                assert_eq!(qualified_path.to_string(), "std::option::Some");
            }
            _ => panic!("expected definition"),
        }
    }

    #[test]
    fn test_package_fallback_resolves_pattern_path() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("std::option::Some"),
            Definition::EnumVariant(
                EnumType {
                    visibility: Visibility::Public,
                    module: qpath("std::option"),
                    name: "Option".to_string(),
                    type_params: vec![],
                    type_var_ids: vec![],
                    variants: vec![("Some".to_string(), EnumVariantType::Tuple(vec![Type::Int]))],
                },
                EnumVariantType::Tuple(vec![Type::Int]),
            ),
        );
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["std", "option", "Some"]);
        let current = QualifiedPath::root();
        let result = resolve_pattern_path(&path, &current, &imports, &definitions, &HashMap::new()).unwrap();
        match result {
            ResolvedPath::Definition { qualified_path, .. } => {
                assert_eq!(qualified_path.to_string(), "std::option::Some");
            }
            _ => panic!("expected definition"),
        }
    }

    #[test]
    fn test_relative_module_path_takes_priority_over_package_fallback() {
        let mut definitions = HashMap::new();
        // Package-level definition
        definitions.insert(
            qpath("std::option::Some"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: qpath("std::option"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        // Relative module definition (root::std::option::Some) — should win
        definitions.insert(
            qpath("root::std::option::Some"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: qpath("root::std::option"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Bool,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["std", "option", "Some"]);
        let current = QualifiedPath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new()).unwrap();
        match result {
            ResolvedPath::Definition { qualified_path, .. } => {
                // Relative module path (Priority 3) should win over package fallback (Priority 4)
                assert_eq!(qualified_path.to_string(), "root::std::option::Some");
            }
            _ => panic!("expected definition"),
        }
    }

    #[test]
    fn test_import_takes_priority_over_package_fallback() {
        let mut definitions = HashMap::new();
        // Package-level definition
        definitions.insert(
            qpath("pkg::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: qpath("pkg::utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        // Imported definition that maps "pkg" to something else
        definitions.insert(
            qpath("root::other::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: qpath("root::other::utils"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Bool,
            }),
        );
        let locals = HashMap::new();
        let mut imports = HashMap::new();
        let mut root_imports = ImportTable::new();
        root_imports.insert("pkg".to_string(), qpath("root::other"));
        imports.insert(QualifiedPath::root(), root_imports);
        let path = path_from_segments(PathPrefix::None, &["pkg", "utils", "helper"]);
        let current = QualifiedPath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions, &HashMap::new()).unwrap();
        match result {
            ResolvedPath::Definition { qualified_path, .. } => {
                // Import (Priority 2) should win over package fallback (Priority 4)
                assert_eq!(qualified_path.to_string(), "root::other::utils::helper");
            }
            _ => panic!("expected definition"),
        }
    }
}
