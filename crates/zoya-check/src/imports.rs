//! Import resolution for use declarations.
//!
//! Resolves `use` statements into qualified paths that can be looked up during type checking.

use std::collections::HashMap;

use zoya_ast::{PathPrefix, UseDecl, UseTarget};
use zoya_ir::{Definition, QualifiedPath, TypeError, Visibility};

use crate::resolution::{
    check_module_path_visible, follow_reexports, is_module_ancestor, resolve_prefix_path,
};

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
pub(crate) fn resolve_use_path(
    use_decl: &UseDecl,
    current_module: &QualifiedPath,
) -> Result<QualifiedPath, TypeError> {
    let path = &use_decl.path;
    if path.prefix == PathPrefix::None {
        return Err(TypeError::ImportValidation {
            message: "use declarations require a prefix (root::, self::, or super::)".to_string(),
        });
    }
    resolve_prefix_path(&path.prefix, &path.segments, current_module)
}

/// Resolve a use path's segments to a QualifiedPath (for Glob/Group targets where
/// segments is the module path, not including an item name).
pub(crate) fn resolve_use_module_path(
    use_decl: &UseDecl,
    current_module: &QualifiedPath,
) -> Result<QualifiedPath, TypeError> {
    let path = &use_decl.path;
    if path.prefix == PathPrefix::None {
        return Err(TypeError::ImportValidation {
            message: "use declarations require a prefix (root::, self::, or super::)".to_string(),
        });
    }
    resolve_prefix_path(&path.prefix, &path.segments, current_module)
}

/// Check if an import target is visible from the importing module.
fn check_import_visible(
    qualified: &QualifiedPath,
    accessor_module: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
) -> Result<(), TypeError> {
    let def = definitions
        .get(qualified)
        .ok_or_else(|| TypeError::UnboundImport {
            name: qualified.to_string(),
        })?;

    if def.visibility() == Visibility::Public {
        return Ok(());
    }

    let target_module = def.module();
    if is_module_ancestor(target_module, accessor_module) {
        Ok(())
    } else {
        Err(TypeError::PrivateAccess {
            kind: "item".to_string(),
            name: qualified.to_string(),
            module: accessor_module.to_string(),
        })
    }
}

/// Insert an import into the table, checking for duplicates.
fn insert_import(
    imports: &mut ImportTable,
    local_name: String,
    qualified: QualifiedPath,
) -> Result<(), TypeError> {
    if let Some(existing) = imports.get(&local_name) {
        return Err(TypeError::DuplicateImport {
            name: local_name.clone(),
            original: existing.to_string(),
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
    let target_visibility = def.visibility();
    if target_visibility != Visibility::Public {
        return Err(TypeError::PrivateReExport {
            name: qualified.to_string(),
        });
    }
    Ok(())
}

/// Resolve a module path, following re-exports if needed.
/// Returns the real (canonical) module path if found.
fn resolve_target_module(
    target: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
    reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Result<Option<QualifiedPath>, TypeError> {
    if let Some(Definition::Module(_)) = definitions.get(target) {
        return Ok(Some(follow_reexports(target, reexports)?));
    }
    Ok(None)
}

/// The kind of container a glob/group path can resolve to.
enum ContainerKind {
    /// A module — glob/group operates on its direct children definitions.
    Module(QualifiedPath),
    /// An enum — glob/group operates on its variants.
    Enum(QualifiedPath),
}

/// Resolve a path that may refer to a module or an enum.
/// Used for glob (`use path::*`) and group (`use path::{a, b}`) imports
/// where the target can be either a module (importing items) or an enum (importing variants).
fn resolve_target_container(
    target: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
    reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Result<Option<ContainerKind>, TypeError> {
    if let Some(def) = definitions.get(target) {
        let resolved = follow_reexports(target, reexports)?;
        match def {
            Definition::Module(_) => return Ok(Some(ContainerKind::Module(resolved))),
            Definition::Enum(_) => return Ok(Some(ContainerKind::Enum(resolved))),
            _ => {}
        }
    }
    Ok(None)
}

/// Resolve all imports for a module and return a unified import table.
///
/// The import table maps local names (the last segment of each use path)
/// to their fully qualified paths. This includes both item imports and module imports.
pub fn resolve_module_imports(
    uses: &[UseDecl],
    current_module: &QualifiedPath,
    definitions: &HashMap<QualifiedPath, Definition>,
    reexports: &HashMap<QualifiedPath, QualifiedPath>,
) -> Result<ImportTable, TypeError> {
    let mut imports = HashMap::new();

    for use_decl in uses {
        match &use_decl.path.target {
            UseTarget::Single { alias } => {
                let qualified = resolve_use_path(use_decl, current_module)?;

                // Check intermediate modules are visible
                check_module_path_visible(&qualified, current_module, definitions)?;

                // Try as item or module import (both are in definitions now)
                if definitions.contains_key(&qualified) {
                    check_import_visible(&qualified, current_module, definitions)?;

                    // Module re-exports are handled in Phase 1.5a; only check pub reexport
                    // visibility for non-module definitions
                    if use_decl.visibility == Visibility::Public
                        && !matches!(definitions.get(&qualified), Some(Definition::Module(..)))
                    {
                        check_pub_reexport_visible(&qualified, definitions)?;
                    }

                    let local_name = alias
                        .clone()
                        .unwrap_or_else(|| use_decl.path.segments.last().unwrap().clone());
                    insert_import(&mut imports, local_name, qualified)?;
                } else {
                    // Try as import through module re-export
                    // e.g., `use root::b::a::helper` where `root::b::a` → `root::a`
                    let mut found = false;
                    let segments = qualified.segments();
                    for prefix_len in (2..segments.len()).rev() {
                        let candidate = QualifiedPath::new(segments[..prefix_len].to_vec());
                        if let Some(real_module) =
                            resolve_target_module(&candidate, definitions, reexports)?
                        {
                            // Rewrite the qualified path through the real module
                            let mut new_segments = real_module.segments().to_vec();
                            new_segments.extend_from_slice(&segments[prefix_len..]);
                            let resolved_qualified = QualifiedPath::new(new_segments);

                            if definitions.contains_key(&resolved_qualified) {
                                check_import_visible(
                                    &resolved_qualified,
                                    current_module,
                                    definitions,
                                )?;

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
                        return Err(TypeError::UnboundImport {
                            name: qualified.to_string(),
                        });
                    }
                }
            }
            UseTarget::Glob => {
                let target_path = resolve_use_module_path(use_decl, current_module)?;

                // Resolve target as module or enum
                let container = resolve_target_container(&target_path, definitions, reexports)?
                    .ok_or_else(|| TypeError::UnboundPath {
                        path: target_path.to_string(),
                    })?;

                match container {
                    ContainerKind::Module(resolved_module) => {
                        // Check module path visibility
                        let module_qpath = QualifiedPath::new(resolved_module.segments().to_vec());
                        check_module_path_visible(&module_qpath, current_module, definitions)?;

                        // Find all definitions in the resolved module (exactly one segment deeper)
                        let module_segments = resolved_module.segments();
                        for (qpath, def) in definitions {
                            if qpath.len() == module_segments.len() + 1
                                && qpath.segments()[..module_segments.len()] == module_segments[..]
                            {
                                let item_name = qpath.last();

                                // Skip private items silently
                                if def.visibility() != Visibility::Public {
                                    continue;
                                }

                                // Skip enum variants unless they were re-exported to module level
                                // (e.g. via `pub use self::Enum::*`)
                                if matches!(def, Definition::EnumVariant(..))
                                    && !reexports.contains_key(qpath)
                                {
                                    continue;
                                }

                                if use_decl.visibility == Visibility::Public {
                                    check_pub_reexport_visible(qpath, definitions)?;
                                }

                                insert_import(&mut imports, item_name.to_string(), qpath.clone())?;
                            }
                        }
                    }
                    ContainerKind::Enum(resolved_enum) => {
                        // Check enum path visibility
                        check_import_visible(&resolved_enum, current_module, definitions)?;

                        // Find all variants of this enum
                        let enum_segments = resolved_enum.segments();
                        for (qpath, def) in definitions {
                            if qpath.len() == enum_segments.len() + 1
                                && qpath.segments()[..enum_segments.len()] == enum_segments[..]
                                && matches!(def, Definition::EnumVariant(..))
                            {
                                let variant_name = qpath.last();

                                if use_decl.visibility == Visibility::Public {
                                    check_pub_reexport_visible(qpath, definitions)?;
                                }

                                insert_import(
                                    &mut imports,
                                    variant_name.to_string(),
                                    qpath.clone(),
                                )?;
                            }
                        }
                    }
                }
            }
            UseTarget::Group(items) => {
                let target_path = resolve_use_module_path(use_decl, current_module)?;

                // Resolve target as module or enum
                let container = resolve_target_container(&target_path, definitions, reexports)?
                    .ok_or_else(|| TypeError::UnboundPath {
                        path: target_path.to_string(),
                    })?;

                match container {
                    ContainerKind::Module(resolved_module) => {
                        // Check module path visibility
                        let module_qpath = QualifiedPath::new(resolved_module.segments().to_vec());
                        check_module_path_visible(&module_qpath, current_module, definitions)?;

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
                    ContainerKind::Enum(resolved_enum) => {
                        // Check enum path visibility
                        check_import_visible(&resolved_enum, current_module, definitions)?;

                        for group_item in items {
                            let qualified = resolved_enum.child(&group_item.name);

                            // Variant visibility is inherited from enum (already checked above)
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
        }
    }

    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ast::{UsePath, UseTarget};
    use zoya_ir::{EnumType, FunctionType, StructType, StructTypeKind, Type};

    fn make_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
        UseDecl {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Private,
            path: UsePath {
                prefix,
                segments: segments.iter().map(|s| s.to_string()).collect(),
                target: UseTarget::Single { alias: None },
            },
        }
    }

    fn qpath(path: &str) -> QualifiedPath {
        QualifiedPath::from(path)
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
        assert!(result.unwrap_err().to_string().contains("super"));
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
        let imports =
            resolve_module_imports(&uses, &current, &definitions, &HashMap::new()).unwrap();

        assert_eq!(imports.len(), 1);
        assert_eq!(imports.get("bar"), Some(&qpath("root::foo::bar")));
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
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already imported"));
    }

    #[test]
    fn test_resolve_module_imports_not_found_error() {
        let definitions = HashMap::new();

        let uses = vec![make_use(PathPrefix::Root, &["foo", "bar"])];
        let current = QualifiedPath::root();
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot find"));
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
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
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
                kind: StructTypeKind::Unit,
                fields: vec![],
            }),
        );

        let uses = vec![make_use(PathPrefix::Root, &["other", "Point"])];
        let current = QualifiedPath::root().child("mine"); // sibling module
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
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
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
    }

    fn make_pub_use(prefix: PathPrefix, segments: &[&str]) -> UseDecl {
        UseDecl {
            leading_comments: vec![],
            attributes: vec![],
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
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("pub use cannot re-export private")
        );
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
        let result = resolve_module_imports(&uses, &current, &definitions, &HashMap::new());

        assert!(result.is_ok());
        let imports = result.unwrap();
        assert_eq!(imports.get("helper"), Some(&qpath("root::other::helper")));
    }

    #[test]
    fn test_package_import_resolves() {
        let mut definitions = HashMap::new();
        // Register a definition under the package prefix (as if injected from a dep)
        definitions.insert(
            qpath("serde::Deserialize"),
            Definition::Function(FunctionType {
                visibility: Visibility::Public,
                module: qpath("serde"),
                type_params: vec![],
                type_var_ids: vec![],
                params: vec![],
                return_type: Type::Int,
            }),
        );

        let uses = vec![UseDecl {
            leading_comments: vec![],
            attributes: vec![],
            visibility: Visibility::Private,
            path: UsePath {
                prefix: PathPrefix::Package("serde".to_string()),
                segments: vec!["Deserialize".to_string()],
                target: UseTarget::Single { alias: None },
            },
        }];
        let current = QualifiedPath::root();
        let result =
            resolve_module_imports(&uses, &current, &definitions, &HashMap::new()).unwrap();
        assert_eq!(
            result.get("Deserialize"),
            Some(&qpath("serde::Deserialize"))
        );
    }
}
