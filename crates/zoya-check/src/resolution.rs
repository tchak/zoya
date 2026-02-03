//! Module path resolution for cross-module function calls.
//!
//! Path resolution is purely structural - no TypeEnv lookup needed.
//! The actual lookup happens after resolution.

use std::collections::HashMap;

use zoya_ast::{Path, PathPrefix};
use zoya_ir::{Definition, QualifiedPath, TypeError, TypeScheme};
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

/// Resolve a path in expression context.
///
/// This handles:
/// 1. Single-segment paths without prefix: check locals first, then definitions
/// 2. Multi-segment paths: resolve as qualified name, then try as enum variant
/// 3. Paths with prefixes (root::, self::, super::)
pub fn resolve_expr_path<'a>(
    path: &Path,
    current_module: &ModulePath,
    locals: &'a HashMap<String, TypeScheme>,
    definitions: &'a HashMap<QualifiedPath, Definition>,
) -> Result<ResolvedPath<'a>, TypeError> {
    // Single-segment path with no prefix: check locals first
    if path.prefix == PathPrefix::None && path.segments.len() == 1 {
        let name = &path.segments[0];
        if let Some(scheme) = locals.get(name) {
            return Ok(ResolvedPath::Local {
                name: name.clone(),
                scheme,
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

/// Resolve a path in pattern context (no locals, only definitions and enum variants).
///
/// This is similar to `resolve_expr_path` but doesn't check locals since patterns
/// don't have access to local variables.
pub fn resolve_pattern_path<'a>(
    path: &Path,
    current_module: &ModulePath,
    definitions: &'a HashMap<QualifiedPath, Definition>,
) -> Result<ResolvedPath<'a>, TypeError> {
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
}
