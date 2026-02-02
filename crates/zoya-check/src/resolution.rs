//! Module path resolution for cross-module function calls.
//!
//! Path resolution is purely structural - no TypeEnv lookup needed.
//! The actual lookup happens after resolution.

use std::collections::HashMap;

use zoya_ast::{Path, PathPrefix};
use zoya_ir::{Definition, EnumType, EnumVariantType, TypeError, TypeScheme};
use zoya_module::ModulePath;

/// Resolve an AST path to a fully qualified module path string.
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
pub fn resolve_path(path: &Path, current_module: &ModulePath) -> Result<String, TypeError> {
    match path.prefix {
        PathPrefix::Root => {
            // root::foo::bar → root::foo::bar
            Ok(format!("root::{}", path.segments.join("::")))
        }
        PathPrefix::Self_ => {
            // self::foo → current_module::foo
            Ok(format!("{}::{}", current_module, path.segments.join("::")))
        }
        PathPrefix::Super => {
            // super::foo → parent_module::foo
            let parent = current_module.parent().ok_or_else(|| TypeError {
                message: "cannot use super:: in root module".to_string(),
            })?;
            Ok(format!("{}::{}", parent, path.segments.join("::")))
        }
        PathPrefix::None => {
            // Relative path: check current module or child module
            resolve_relative_path(path, current_module)
        }
    }
}

/// Resolve a relative path (no prefix) to a fully qualified module path string.
fn resolve_relative_path(path: &Path, current_module: &ModulePath) -> Result<String, TypeError> {
    match path.segments.as_slice() {
        [name] => {
            // Single segment: resolve in current module
            // (locals are checked separately before this)
            Ok(format!("{}::{}", current_module, name))
        }
        [first, rest @ ..] => {
            // Multi-segment: first segment could be:
            // 1. Child module (foo::bar → current_module::foo::bar)
            // 2. Enum name (Option::Some → current_module::Option::Some)
            // For now, treat all as current_module relative
            let item = std::iter::once(first.as_str())
                .chain(rest.iter().map(|s| s.as_str()))
                .collect::<Vec<_>>()
                .join("::");
            Ok(format!("{}::{}", current_module, item))
        }
        [] => Err(TypeError {
            message: "empty path".to_string(),
        }),
    }
}

/// Format a full qualified name from a module path and item name.
pub fn qualified_name(module: &ModulePath, name: &str) -> String {
    format!("{}::{}", module, name)
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
        qualified_name: String,
        def: &'a Definition,
    },
    /// Enum variant (nested inside enum definition)
    EnumVariant {
        qualified_name: String,
        def: &'a EnumType,
        variant: (String, &'a EnumVariantType),
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
    definitions: &'a HashMap<String, Definition>,
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
    let qualified = resolve_path(path, current_module)?;

    // Try exact match in definitions
    if let Some(def) = definitions.get(&qualified) {
        return Ok(ResolvedPath::Definition {
            qualified_name: qualified,
            def,
        });
    }

    // If path has 2+ segments, try interpreting last segment as enum variant
    // E.g., "Option::Some" -> look up "Option" as enum, then "Some" as variant
    // E.g., "root::module::Option::Some" -> look up "root::module::Option", then "Some"
    if path.segments.len() >= 2 {
        // Try to split off the last segment as variant name
        let variant_name = path.segments.last().unwrap();

        // Build the enum path (all segments except last)
        let enum_path = Path {
            prefix: path.prefix,
            segments: path.segments[..path.segments.len() - 1].to_vec(),
            type_args: None,
        };
        let qualified_name = resolve_path(&enum_path, current_module)?;

        if let Some(Definition::Enum(def)) = definitions.get(&qualified_name) {
            // Find the variant
            if let Some((_, variant)) = def.variants.iter().find(|(name, _)| name == variant_name) {
                return Ok(ResolvedPath::EnumVariant {
                    qualified_name,
                    def,
                    variant: (variant_name.clone(), variant),
                });
            }
            // Enum exists but variant doesn't
            return Err(TypeError {
                message: format!(
                    "enum '{}' has no variant '{}'",
                    enum_path
                        .segments
                        .last()
                        .unwrap_or(&"<unknown>".to_string()),
                    variant_name
                ),
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

/// Resolve a path in pattern context (no locals, only definitions and enum variants).
///
/// This is similar to `resolve_expr_path` but doesn't check locals since patterns
/// don't have access to local variables.
pub fn resolve_pattern_path<'a>(
    path: &Path,
    current_module: &ModulePath,
    definitions: &'a HashMap<String, Definition>,
) -> Result<ResolvedPath<'a>, TypeError> {
    // Resolve the full path
    let qualified = resolve_path(path, current_module)?;

    // Try exact match in definitions
    if let Some(def) = definitions.get(&qualified) {
        return Ok(ResolvedPath::Definition {
            qualified_name: qualified,
            def,
        });
    }

    // If path has 2+ segments, try interpreting last segment as enum variant
    if path.segments.len() >= 2 {
        let variant_name = path.segments.last().unwrap();

        // Build the enum path (all segments except last)
        let enum_path = Path {
            prefix: path.prefix,
            segments: path.segments[..path.segments.len() - 1].to_vec(),
            type_args: None,
        };
        let qualified_name = resolve_path(&enum_path, current_module)?;

        if let Some(Definition::Enum(def)) = definitions.get(&qualified_name) {
            if let Some((_, variant)) = def.variants.iter().find(|(name, _)| name == variant_name) {
                return Ok(ResolvedPath::EnumVariant {
                    qualified_name,
                    def,
                    variant: (variant_name.clone(), variant),
                });
            }
            return Err(TypeError {
                message: format!(
                    "enum '{}' has no variant '{}'",
                    enum_path
                        .segments
                        .last()
                        .unwrap_or(&"<unknown>".to_string()),
                    variant_name
                ),
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
        let current = ModulePath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result, "root::foo");
    }

    #[test]
    fn test_resolve_simple_path_in_nested_module() {
        let path = path_from_segments(PathPrefix::None, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result, "root::utils::foo");
    }

    #[test]
    fn test_resolve_root_prefix() {
        let path = path_from_segments(PathPrefix::Root, &["utils", "helper"]);
        let current = ModulePath::root().child("other");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result, "root::utils::helper");
    }

    #[test]
    fn test_resolve_self_prefix() {
        let path = path_from_segments(PathPrefix::Self_, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result, "root::utils::foo");
    }

    #[test]
    fn test_resolve_super_prefix() {
        let path = path_from_segments(PathPrefix::Super, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result, "root::foo");
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
        assert_eq!(result, "root::Option::Some");
    }

    #[test]
    fn test_resolve_deeply_nested() {
        let path = path_from_segments(PathPrefix::Root, &["a", "b", "c", "foo"]);
        let current = ModulePath::root();
        let result = resolve_path(&path, &current).unwrap();
        assert_eq!(result, "root::a::b::c::foo");
    }

    #[test]
    fn test_qualified_name() {
        let module = ModulePath::root().child("utils");
        let result = qualified_name(&module, "helper");
        assert_eq!(result, "root::utils::helper");
    }
}
