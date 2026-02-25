use std::path::Path;

use zoya_codegen::{CodegenOutput, codegen};
use zoya_ir::{DefinitionLookup, FunctionKind, QualifiedPath, TypeError, TypedPattern};
use zoya_package::Package;

pub use zoya_ir::{FunctionType, HttpMethod, Pathname};
pub use zoya_loader::Mode;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("{0}")]
    Load(#[from] zoya_loader::LoaderError),
    #[error("{0}")]
    Check(#[from] TypeError),
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct BuildOutput {
    pub name: String,
    pub output: CodegenOutput,
    pub definitions: DefinitionLookup,
    pub functions: Vec<(QualifiedPath, Vec<String>)>,
    pub tests: Vec<QualifiedPath>,
    pub jobs: Vec<(QualifiedPath, String)>,
    pub routes: Vec<(QualifiedPath, HttpMethod, Pathname)>,
}

pub fn check_from_path(path: &Path, mode: Mode) -> Result<(), BuildError> {
    let package = zoya_loader::load_package(path, mode)?;
    check(&package)
}

pub fn build_from_path(path: &Path, mode: Mode) -> Result<BuildOutput, BuildError> {
    let package = zoya_loader::load_package(path, mode)?;
    build(&package)
}

pub fn check(package: &Package) -> Result<(), BuildError> {
    let std = zoya_std::std();
    zoya_check::check(package, &[std])?;
    Ok(())
}

pub fn build(package: &Package) -> Result<BuildOutput, BuildError> {
    let std = zoya_std::std();
    let checked = zoya_check::check(package, &[std])?;
    let output = codegen(&checked, &[std]);
    let definitions = DefinitionLookup::from_packages(&checked, &[std]);
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    let mut jobs = Vec::new();
    let mut routes = Vec::new();
    for (path, func) in &checked.items {
        match &func.kind {
            FunctionKind::Regular | FunctionKind::Builtin => {
                if checked.definitions.contains_key(path) {
                    let param_names = func
                        .params
                        .iter()
                        .map(|(pattern, _)| match pattern {
                            TypedPattern::Var { name, .. } => name.clone(),
                            _ => "_".to_string(),
                        })
                        .collect();
                    functions.push((path.clone(), param_names));
                }
            }
            FunctionKind::Test => tests.push(path.clone()),
            FunctionKind::Job(variant_name) => jobs.push((path.clone(), variant_name.clone())),
            FunctionKind::Http(method, pathname) => {
                routes.push((path.clone(), *method, pathname.clone()));
            }
        }
    }
    functions.sort_by_key(|(p, _)| p.to_string());
    tests.sort_by_key(|p| p.to_string());
    jobs.sort_by_key(|(p, _)| p.to_string());
    routes.sort_by_key(|(p, _, _)| p.to_string());
    Ok(BuildOutput {
        name: checked.name.clone(),
        output,
        definitions,
        functions,
        tests,
        jobs,
        routes,
    })
}
