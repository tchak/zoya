//! Import resolution for use declarations.
//!
//! Resolves `use` statements into qualified paths that can be looked up during type checking.

use std::collections::HashMap;

use zoya_ast::{PathPrefix, UseDecl};
use zoya_ir::{Definition, QualifiedPath, TypeError, Visibility};
use zoya_package::{ModulePath, Package};

/// Resolved import entry: maps a local name to a qualified path
pub type ImportTable = HashMap<String, QualifiedPath>;

/// Resolve a use path to a fully qualified path.
///
/// # Path Resolution Rules
///
/// | Path | Resolution |
/// |------|------------|
/// | `use root::foo::bar` | Absolute path from root module |
/// | `use self::foo` | Explicit current module reference |
/// | `use super::foo` | Parent module reference |
fn resolve_use_path(
    use_decl: &UseDecl,
    current_module: &ModulePath,
) -> Result<QualifiedPath, TypeError> {
    let path = &use_decl.path;

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
            // This should not happen - parser rejects paths without prefix
            Err(TypeError {
                message: "use declarations require a prefix (root::, self::, or super::)"
                    .to_string(),
            })
        }
    }
}

/// Check if an import target is visible from the importing module.
fn check_import_visible(
    qualified: &QualifiedPath,
    accessor_module: &ModulePath,
    definitions: &HashMap<QualifiedPath, Definition>,
) -> Result<(), TypeError> {
    // Look up the definition
    let def = definitions.get(qualified).ok_or_else(|| TypeError {
        message: format!("cannot find '{}' to import", qualified),
    })?;

    // Get visibility
    let visibility = match def {
        Definition::Function(f) => f.visibility,
        Definition::Struct(s) => s.visibility,
        Definition::Enum(e) => e.visibility,
        Definition::TypeAlias(a) => a.visibility,
        Definition::EnumVariant(parent_enum, _) => parent_enum.visibility,
    };

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
                "'{}' is private and cannot be imported from '{}'",
                qualified, accessor_module
            ),
        })
    }
}

/// Check that all intermediate modules in a qualified path are visible from the accessor module.
fn check_import_module_path_visible(
    qualified: &QualifiedPath,
    accessor_module: &ModulePath,
    pkg: &Package,
) -> Result<(), TypeError> {
    let segments = &qualified.segments;

    if segments.len() < 3 {
        return Ok(());
    }

    for i in 1..segments.len() - 1 {
        let parent_module_path = ModulePath(segments[0..i].to_vec());
        let child_name = &segments[i];

        if let Some(parent_module) = pkg.get(&parent_module_path)
            && let Some((_, visibility)) = parent_module.children.get(child_name)
        {
            if *visibility == Visibility::Public {
                continue;
            }

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

/// Resolve all imports for a module and return an import table.
///
/// The import table maps local names (the last segment of each use path)
/// to their fully qualified paths.
pub fn resolve_module_imports(
    uses: &[UseDecl],
    current_module: &ModulePath,
    definitions: &HashMap<QualifiedPath, Definition>,
    pkg: &Package,
) -> Result<ImportTable, TypeError> {
    let mut imports = HashMap::new();

    for use_decl in uses {
        let qualified = resolve_use_path(use_decl, current_module)?;

        // Check intermediate modules are visible
        check_import_module_path_visible(&qualified, current_module, pkg)?;

        // Check target exists and is visible
        check_import_visible(&qualified, current_module, definitions)?;

        // Import uses the last segment as local name
        let local_name = use_decl
            .path
            .segments
            .last()
            .ok_or_else(|| TypeError {
                message: "use declaration has empty path".to_string(),
            })?
            .clone();

        // Check for duplicate imports
        if let Some(existing) = imports.get(&local_name) {
            return Err(TypeError {
                message: format!(
                    "'{}' is already imported (from '{}')",
                    local_name, existing
                ),
            });
        }

        imports.insert(local_name, qualified);
    }

    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::UsePath;
    use zoya_ir::{EnumType, FunctionType, StructType, Type};

    fn make_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
        UseDecl {
            visibility: Visibility::Private,
            path: UsePath {
                prefix,
                segments: segments.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn qpath(path: &str) -> QualifiedPath {
        QualifiedPath::new(path.split("::").map(|s| s.to_string()).collect())
    }

    fn empty_pkg() -> Package {
        Package {
            modules: HashMap::new(),
        }
    }

    #[test]
    fn test_resolve_use_path_root() {
        let use_decl = make_use(PathPrefix::Root, &["foo", "bar"]);
        let current = ModulePath::root();
        let result = resolve_use_path(&use_decl, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo::bar");
    }

    #[test]
    fn test_resolve_use_path_self() {
        let use_decl = make_use(PathPrefix::Self_, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_use_path(&use_decl, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_resolve_use_path_super() {
        let use_decl = make_use(PathPrefix::Super, &["foo"]);
        let current = ModulePath::root().child("utils");
        let result = resolve_use_path(&use_decl, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo");
    }

    #[test]
    fn test_resolve_use_path_super_from_root_fails() {
        let use_decl = make_use(PathPrefix::Super, &["foo"]);
        let current = ModulePath::root();
        let result = resolve_use_path(&use_decl, &current);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("super"));
    }

    #[test]
    fn test_resolve_module_imports_basic() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::foo::bar"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: ModulePath::root().child("foo"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["foo", "bar"])];
        let current = ModulePath::root();
        let imports = resolve_module_imports(&uses, &current, &definitions, &empty_pkg()).unwrap();

        assert_eq!(imports.len(), 1);
        assert_eq!(
            imports.get("bar"),
            Some(&qpath("root::foo::bar"))
        );
    }

    #[test]
    fn test_resolve_module_imports_duplicate_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::foo::bar"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: ModulePath::root().child("foo"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );
        definitions.insert(
            qpath("root::baz::bar"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: ModulePath::root().child("baz"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![
            make_use(PathPrefix::Root, &["foo", "bar"]),
            make_use(PathPrefix::Root, &["baz", "bar"]),
        ];
        let current = ModulePath::root();
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("already imported"));
    }

    #[test]
    fn test_resolve_module_imports_not_found_error() {
        let definitions = HashMap::new();

        let uses = vec![make_use(PathPrefix::Root, &["foo", "bar"])];
        let current = ModulePath::root();
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("cannot find"));
    }

    #[test]
    fn test_resolve_module_imports_private_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::other::secret"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: ModulePath::root().child("other"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "secret"])];
        let current = ModulePath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    #[test]
    fn test_import_private_struct_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::other::Point"),
            Definition::Struct(StructType {
                visibility: Visibility::Private,
                module: ModulePath::root().child("other"),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![],
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "Point"])];
        let current = ModulePath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    #[test]
    fn test_import_private_enum_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::other::Color"),
            Definition::Enum(EnumType {
                visibility: Visibility::Private,
                module: ModulePath::root().child("other"),
                name: "Color".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                variants: vec![],
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "Color"])];
        let current = ModulePath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }
}
