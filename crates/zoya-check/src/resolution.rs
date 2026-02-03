//! Module path resolution for cross-module function calls.
//!
//! Path resolution is purely structural - no TypeEnv lookup needed.
//! The actual lookup happens after resolution.

use std::collections::HashMap;

use zoya_ast::{Path, PathPrefix};
use zoya_ir::{Definition, QualifiedPath, TypeError, TypeScheme, Visibility};
use zoya_module::ModulePath;

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
pub fn resolve_path(path: &Path, current_module: &ModulePath) -> Result<QualifiedPath, TypeError> {
    match path.prefix {
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
        PathPrefix::None => {
            // Relative path: check current module or child module
            resolve_relative_path(path, current_module)
        }
    }
}

/// Resolve a relative path (no prefix) to a fully qualified path.
fn resolve_relative_path(
    path: &Path,
    current_module: &ModulePath,
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

/// Check if a function is visible from the accessor module.
///
/// Visibility rules:
/// - Public functions are always visible
/// - Private functions are visible if the accessor is in the same module or a descendant
fn check_function_visibility(
    visibility: Visibility,
    target_path: &QualifiedPath,
    accessor_module: &ModulePath,
) -> Result<(), TypeError> {
    if visibility == Visibility::Public {
        return Ok(());
    }

    // Get target's module (all segments except last, which is the function name)
    let target_module: Vec<&str> = target_path.segments[..target_path.segments.len() - 1]
        .iter()
        .map(|s| s.as_str())
        .collect();

    let accessor: Vec<&str> = accessor_module
        .segments()
        .iter()
        .map(|s| s.as_str())
        .collect();

    // Private visible if accessor is same module or descendant
    let is_visible =
        accessor.len() >= target_module.len() && accessor[..target_module.len()] == target_module[..];

    if is_visible {
        Ok(())
    } else {
        Err(TypeError {
            message: format!(
                "function '{}' is private to module '{}'",
                target_path.segments.last().unwrap(),
                target_module.join("::")
            ),
        })
    }
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
    current_module: &ModulePath,
    locals: &'a HashMap<String, TypeScheme>,
    imports: &'a HashMap<ModulePath, ImportTable>,
    definitions: &'a HashMap<QualifiedPath, Definition>,
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
        if let Some(module_imports) = imports.get(current_module)
            && let Some(qualified) = module_imports.get(name)
            && let Some(def) = definitions.get(qualified)
        {
            return Ok(ResolvedPath::Definition {
                qualified_path: qualified.clone(),
                def,
            });
        }
    }

    // Priority 3: Resolve the full path in module-level definitions
    let qualified_path = resolve_path(path, current_module)?;

    // Try exact match in definitions
    if let Some(def) = definitions.get(&qualified_path) {
        // Check visibility for functions
        if let Definition::Function(func_type) = def {
            check_function_visibility(func_type.visibility, &qualified_path, current_module)?;
        }
        return Ok(ResolvedPath::Definition {
            qualified_path,
            def,
        });
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

/// Resolve a path in pattern context (no locals, only imports and definitions).
///
/// This is similar to `resolve_expr_path` but doesn't check locals since patterns
/// don't have access to local variables.
pub fn resolve_pattern_path<'a>(
    path: &Path,
    current_module: &ModulePath,
    imports: &'a HashMap<ModulePath, ImportTable>,
    definitions: &'a HashMap<QualifiedPath, Definition>,
) -> Result<ResolvedPath<'a>, TypeError> {
    // For single-segment paths without prefix, check imports first
    if path.prefix == PathPrefix::None && path.segments.len() == 1 {
        let name = &path.segments[0];

        // Check imports
        if let Some(module_imports) = imports.get(current_module)
            && let Some(qualified) = module_imports.get(name)
            && let Some(def) = definitions.get(qualified)
        {
            return Ok(ResolvedPath::Definition {
                qualified_path: qualified.clone(),
                def,
            });
        }
    }

    // Resolve the full path
    let qualified_path = resolve_path(path, current_module)?;

    // Try exact match in definitions
    if let Some(def) = definitions.get(&qualified_path) {
        return Ok(ResolvedPath::Definition {
            qualified_path,
            def,
        });
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
        let current = ModulePath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo");
    }

    #[test]
    fn test_resolve_simple_path_in_nested_module() {
        let path = path_from_segments(PathPrefix::None, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_resolve_root_prefix() {
        let path = path_from_segments(PathPrefix::Root, &["utils", "helper"]);
        let current = ModulePath::root().child("other");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::helper");
    }

    #[test]
    fn test_resolve_self_prefix() {
        let path = path_from_segments(PathPrefix::Self_, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_resolve_super_prefix() {
        let path = path_from_segments(PathPrefix::Super, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo");
    }

    #[test]
    fn test_resolve_super_in_root_module_error() {
        let path = path_from_segments(PathPrefix::Super, &["foo"]);
        let current = ModulePath::root();
        let result = resolve_path(&path, &current);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("super::"));
    }

    #[test]
    fn test_resolve_qualified_relative_path() {
        let path = path_from_segments(PathPrefix::None, &["Option", "Some"]);
        let current = ModulePath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::Option::Some");
    }

    #[test]
    fn test_resolve_deeply_nested() {
        let path = path_from_segments(PathPrefix::Root, &["a", "b", "c", "foo"]);
        let current = ModulePath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result.to_string(), "root::a::b::c::foo");
    }

    #[test]
    fn test_qualified_path_from_module() {
        let module = ModulePath::root().child("utils");
        let result = QualifiedPath::from_module(&module, "helper");
        assert_eq!(result.to_string(), "root::utils::helper");
    }

    // ========================================================================
    // Visibility Tests
    // ========================================================================

    use zoya_ir::{FunctionType, Type, Definition};

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
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["utils", "helper"]);
        let current = ModulePath::root(); // calling from root
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions);
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_function_same_module() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["helper"]);
        let current = ModulePath::root().child("utils"); // calling from same module
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions);
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_function_child_can_access_parent() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Super, &["helper"]);
        let current = ModulePath::root().child("utils"); // child accessing parent
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions);
        assert!(result.is_ok());
    }

    #[test]
    fn test_visibility_private_function_parent_cannot_access_child() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::utils::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::None, &["utils", "helper"]);
        let current = ModulePath::root(); // parent trying to access child's private
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions);
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
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["a", "helper"]);
        let current = ModulePath::root().child("b"); // sibling trying to access
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions);
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
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        let locals = HashMap::new();
        let imports = HashMap::new();
        let path = path_from_segments(PathPrefix::Root, &["helper"]);
        let current = ModulePath::root().child("a").child("b").child("c"); // deeply nested
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions);
        assert!(result.is_ok());
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
        imports.insert(ModulePath::root(), root_imports);

        let path = path_from_segments(PathPrefix::None, &["helper"]);
        let current = ModulePath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions).unwrap();

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
        imports.insert(ModulePath::root(), root_imports);

        let path = path_from_segments(PathPrefix::None, &["helper"]);
        let current = ModulePath::root();
        let result = resolve_expr_path(&path, &current, &locals, &imports, &definitions).unwrap();

        // Should resolve to the local variable, not the import
        match result {
            ResolvedPath::Local { name, .. } => {
                assert_eq!(name, "helper");
            }
            _ => panic!("expected local"),
        }
    }
}
