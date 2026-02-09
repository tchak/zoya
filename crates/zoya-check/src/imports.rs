//! Import resolution for use declarations.
//!
//! Resolves `use` statements into qualified paths that can be looked up during type checking.

use std::collections::HashMap;

use zoya_ast::{PathPrefix, UseDecl, UseTarget};
use zoya_ir::{Definition, QualifiedPath, TypeError, Visibility};
use zoya_package::Package;

/// Resolved import entry: maps a local name to a qualified path
pub type ImportTable = HashMap<String, QualifiedPath>;

/// Module import table: maps a local module alias to a module path
pub type ModuleImportTable = HashMap<String, QualifiedPath>;

/// Resolve a use path to a fully qualified path.
///
/// # Path Resolution Rules
///
/// | Path | Resolution |
/// |------|------------|
/// | `use root::foo::bar` | Absolute path from root module |
/// | `use self::foo` | Explicit current module reference |
/// | `use super::foo` | Parent module reference |
pub(crate) fn resolve_use_path(
    use_decl: &UseDecl,
    current_module: &QualifiedPath,
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

/// Resolve a use path's segments to a QualifiedPath (for Glob/Group targets where
/// segments is the module path, not including an item name).
pub(crate) fn resolve_use_module_path(
    use_decl: &UseDecl,
    current_module: &QualifiedPath,
) -> Result<QualifiedPath, TypeError> {
    let path = &use_decl.path;

    match path.prefix {
        PathPrefix::Root => {
            let mut segments = vec!["root".to_string()];
            segments.extend(path.segments.iter().cloned());
            Ok(QualifiedPath::new(segments))
        }
        PathPrefix::Self_ => {
            let mut segments = current_module
                .segments()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            segments.extend(path.segments.iter().cloned());
            Ok(QualifiedPath::new(segments))
        }
        PathPrefix::Super => {
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
        PathPrefix::None => Err(TypeError {
            message: "use declarations require a prefix (root::, self::, or super::)".to_string(),
        }),
    }
}

/// Check if an import target is visible from the importing module.
fn check_import_visible(
    qualified: &QualifiedPath,
    accessor_module: &QualifiedPath,
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
    accessor_module: &QualifiedPath,
    pkg: &Package,
) -> Result<(), TypeError> {
    let segments = qualified.segments();

    if segments.len() < 3 {
        return Ok(());
    }

    for i in 1..segments.len() - 1 {
        let parent_module_path = QualifiedPath::new(segments[0..i].to_vec());
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

/// Insert an import into the table, checking for duplicates.
fn insert_import(
    imports: &mut ImportTable,
    local_name: String,
    qualified: QualifiedPath,
) -> Result<(), TypeError> {
    if let Some(existing) = imports.get(&local_name) {
        return Err(TypeError {
            message: format!(
                "'{}' is already imported (from '{}')",
                local_name, existing
            ),
        });
    }
    imports.insert(local_name, qualified);
    Ok(())
}

/// Check that a pub use re-export target is public.
fn check_pub_reexport_visible(
    qualified: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
) -> Result<(), TypeError> {
    let def = definitions.get(qualified).expect("already checked above");
    let target_visibility = match def {
        Definition::Function(f) => f.visibility,
        Definition::Struct(s) => s.visibility,
        Definition::Enum(e) => e.visibility,
        Definition::TypeAlias(a) => a.visibility,
        Definition::EnumVariant(parent_enum, _) => parent_enum.visibility,
    };
    if target_visibility != Visibility::Public {
        return Err(TypeError {
            message: format!("pub use cannot re-export private item '{}'", qualified),
        });
    }
    Ok(())
}

/// Get the visibility of a definition.
fn definition_visibility(def: &Definition) -> Visibility {
    match def {
        Definition::Function(f) => f.visibility,
        Definition::Struct(s) => s.visibility,
        Definition::Enum(e) => e.visibility,
        Definition::TypeAlias(a) => a.visibility,
        Definition::EnumVariant(parent_enum, _) => parent_enum.visibility,
    }
}

/// Resolve a module path, following module re-exports if needed.
/// Returns the real module path if found (either directly or through re-exports).
fn resolve_target_module(
    target: &QualifiedPath,
    pkg: &Package,
    module_reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Option<QualifiedPath> {
    if pkg.get(target).is_some() {
        return Some(target.clone());
    }
    let mut current = target.clone();
    while let Some(real) = module_reexports.get(&current) {
        current = real.clone();
        if pkg.get(&current).is_some() {
            return Some(current);
        }
    }
    None
}

/// Resolve all imports for a module and return item imports and module imports.
///
/// The import table maps local names (the last segment of each use path)
/// to their fully qualified paths.
/// The module import table maps local module aliases to module paths.
pub fn resolve_module_imports(
    uses: &[UseDecl],
    current_module: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
    pkg: &Package,
    module_reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Result<(ImportTable, ModuleImportTable), TypeError> {
    let mut imports = HashMap::new();
    let mut module_imports = HashMap::new();

    for use_decl in uses {
        match &use_decl.path.target {
            UseTarget::Single { alias } => {
                let qualified = resolve_use_path(use_decl, current_module)?;

                // Check intermediate modules are visible
                check_import_module_path_visible(&qualified, current_module, pkg)?;

                // Try as item import first
                if definitions.contains_key(&qualified) {
                    check_import_visible(&qualified, current_module, definitions)?;

                    if use_decl.visibility == Visibility::Public {
                        check_pub_reexport_visible(&qualified, definitions)?;
                    }

                    let local_name = alias.clone().unwrap_or_else(|| {
                        use_decl.path.segments.last().unwrap().clone()
                    });
                    insert_import(&mut imports, local_name, qualified)?;
                } else {
                    // Try as module import: resolve segments as a module path
                    let target_module = resolve_use_module_path(use_decl, current_module)?;
                    if let Some(resolved) = resolve_target_module(&target_module, pkg, module_reexports) {
                        let local_name = alias.clone().unwrap_or_else(|| {
                            use_decl.path.segments.last().unwrap().clone()
                        });

                        if module_imports.contains_key(&local_name) {
                            return Err(TypeError {
                                message: format!(
                                    "module '{}' is already imported",
                                    local_name
                                ),
                            });
                        }
                        // Also check for name collision with item imports
                        if imports.contains_key(&local_name) {
                            return Err(TypeError {
                                message: format!(
                                    "'{}' is already imported",
                                    local_name
                                ),
                            });
                        }

                        module_imports.insert(local_name, resolved);
                    } else {
                        // Try as item import through module re-export
                        // e.g., `use root::b::a::helper` where `root::b::a` → `root::a`
                        let mut found = false;
                        let segments = qualified.segments();
                        for prefix_len in (2..segments.len()).rev() {
                            let candidate = QualifiedPath::new(segments[..prefix_len].to_vec());
                            if let Some(real_module) = resolve_target_module(&candidate, pkg, module_reexports) {
                                // Rewrite the qualified path through the real module
                                let mut new_segments = real_module.segments().to_vec();
                                new_segments.extend_from_slice(&segments[prefix_len..]);
                                let resolved_qualified = QualifiedPath::new(new_segments);

                                if definitions.contains_key(&resolved_qualified) {
                                    check_import_visible(&resolved_qualified, current_module, definitions)?;

                                    if use_decl.visibility == Visibility::Public {
                                        check_pub_reexport_visible(&resolved_qualified, definitions)?;
                                    }

                                    let local_name = alias.clone().unwrap_or_else(|| {
                                        use_decl.path.segments.last().unwrap().clone()
                                    });
                                    insert_import(&mut imports, local_name, resolved_qualified)?;
                                    found = true;
                                    break;
                                }
                            }
                        }
                        if !found {
                            return Err(TypeError {
                                message: format!("cannot find '{}' to import", qualified),
                            });
                        }
                    }
                }
            }
            UseTarget::Glob => {
                let target_module = resolve_use_module_path(use_decl, current_module)?;

                // Verify the module exists (following re-exports)
                let resolved_module = resolve_target_module(&target_module, pkg, module_reexports)
                    .ok_or_else(|| TypeError {
                        message: format!("cannot find module '{}'", target_module),
                    })?;

                // Check module path visibility
                let module_qpath = QualifiedPath::new(resolved_module.segments().to_vec());
                check_import_module_path_visible(&module_qpath, current_module, pkg)?;

                // Find all definitions in the resolved module (exactly one segment deeper)
                let module_segments = resolved_module.segments();
                for (qpath, def) in definitions {
                    // Check if this definition is directly in the target module
                    if qpath.len() == module_segments.len() + 1
                        && qpath.segments()[..module_segments.len()] == module_segments[..]
                    {
                        let item_name = qpath.last();

                        // Skip private items silently
                        if definition_visibility(def) != Visibility::Public {
                            continue;
                        }

                        // Skip enum variants (they're imported via the enum itself)
                        if matches!(def, Definition::EnumVariant(..)) {
                            continue;
                        }

                        if use_decl.visibility == Visibility::Public {
                            check_pub_reexport_visible(qpath, definitions)?;
                        }

                        insert_import(&mut imports, item_name.to_string(), qpath.clone())?;
                    }
                }
            }
            UseTarget::Group(items) => {
                let target_module = resolve_use_module_path(use_decl, current_module)?;

                // Verify the module exists (following re-exports)
                let resolved_module = resolve_target_module(&target_module, pkg, module_reexports)
                    .ok_or_else(|| TypeError {
                        message: format!("cannot find module '{}'", target_module),
                    })?;

                // Check module path visibility
                let module_qpath = QualifiedPath::new(resolved_module.segments().to_vec());
                check_import_module_path_visible(&module_qpath, current_module, pkg)?;

                for group_item in items {
                    let qualified = resolved_module.child(&group_item.name);

                    check_import_visible(&qualified, current_module, definitions)?;

                    if use_decl.visibility == Visibility::Public {
                        check_pub_reexport_visible(&qualified, definitions)?;
                    }

                    let local_name = group_item
                        .alias
                        .clone()
                        .unwrap_or_else(|| group_item.name.clone());
                    insert_import(&mut imports, local_name, qualified)?;
                }
            }
        }
    }

    Ok((imports, module_imports))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::{UsePath, UseTarget};
    use zoya_ir::{EnumType, FunctionType, StructType, Type};

    fn make_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
        UseDecl {
            visibility: Visibility::Private,
            path: UsePath {
                prefix,
                segments: segments.iter().map(|s| s.to_string()).collect(),
                target: UseTarget::Single { alias: None },
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
        let current = QualifiedPath::root();
        let result = resolve_use_path(&use_decl, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo::bar");
    }

    #[test]
    fn test_resolve_use_path_self() {
        let use_decl = make_use(PathPrefix::Self_, &["foo"]);
        let current = QualifiedPath::root().child("utils");
        let result = resolve_use_path(&use_decl, &current).unwrap();
        assert_eq!(result.to_string(), "root::utils::foo");
    }

    #[test]
    fn test_resolve_use_path_super() {
        let use_decl = make_use(PathPrefix::Super, &["foo"]);
        let current = QualifiedPath::root().child("utils");
        let result = resolve_use_path(&use_decl, &current).unwrap();
        assert_eq!(result.to_string(), "root::foo");
    }

    #[test]
    fn test_resolve_use_path_super_from_root_fails() {
        let use_decl = make_use(PathPrefix::Super, &["foo"]);
        let current = QualifiedPath::root();
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
                module: QualifiedPath::root().child("foo"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["foo", "bar"])];
        let current = QualifiedPath::root();
        let (imports, _) = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new()).unwrap();

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
                module: QualifiedPath::root().child("foo"),
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
                module: QualifiedPath::root().child("baz"),
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
        let current = QualifiedPath::root();
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("already imported"));
    }

    #[test]
    fn test_resolve_module_imports_not_found_error() {
        let definitions = HashMap::new();

        let uses = vec![make_use(PathPrefix::Root, &["foo", "bar"])];
        let current = QualifiedPath::root();
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

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
                module: QualifiedPath::root().child("other"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "secret"])];
        let current = QualifiedPath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

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
                module: QualifiedPath::root().child("other"),
                name: "Point".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                fields: vec![],
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "Point"])];
        let current = QualifiedPath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

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
                module: QualifiedPath::root().child("other"),
                name: "Color".to_string(),
                type_params: vec![],
                type_var_ids: vec![],
                variants: vec![],
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "Color"])];
        let current = QualifiedPath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("private"));
    }

    fn make_pub_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
        UseDecl {
            visibility: Visibility::Public,
            path: UsePath {
                prefix,
                segments: segments.iter().map(|s| s.to_string()).collect(),
                target: UseTarget::Single { alias: None },
            },
        }
    }

    #[test]
    fn test_pub_use_reexport_private_function_error() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::other::secret"),
            Definition::Function(FunctionType {
                visibility: Visibility::Private,
                module: QualifiedPath::root().child("other"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        // pub use from same module (so import visibility passes), but target is private
        let uses = vec![make_pub_use(PathPrefix::Root, &["other", "secret"])];
        let current = QualifiedPath::root().child("other"); // same module
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("pub use cannot re-export private"));
    }

    #[test]
    fn test_pub_use_reexport_public_function_ok() {
        let mut definitions = HashMap::new();
        definitions.insert(
            qpath("root::other::helper"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: QualifiedPath::root().child("other"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![make_pub_use(PathPrefix::Root, &["other", "helper"])];
        let current = QualifiedPath::root().child("reexporter");
        let result = resolve_module_imports(&uses, &current, &definitions, &empty_pkg(), &HashMap::new());

        assert!(result.is_ok());
        let (imports, _) = result.unwrap();
        assert_eq!(imports.get("helper"), Some(&qpath("root::other::helper")));
    }
}
