use std::path::Path;

use zoya_codegen::{CodegenOutput, codegen};
use zoya_ir::{DefinitionLookup, QualifiedPath, TypeError, TypedPattern};
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

#[derive(serde::Serialize, serde::Deserialize)]
pub struct BuildOutput {
    pub name: String,
    pub output: CodegenOutput,
    pub definitions: DefinitionLookup,
    pub functions: Vec<(QualifiedPath, Vec<String>)>,
    pub tests: Vec<QualifiedPath>,
    pub tasks: Vec<QualifiedPath>,
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
    let functions = checked
        .fns()
        .into_iter()
        .map(|path| {
            let param_names = checked.items[&path]
                .params
                .iter()
                .map(|(pattern, _)| match pattern {
                    TypedPattern::Var { name, .. } => name.clone(),
                    _ => "_".to_string(),
                })
                .collect();
            (path, param_names)
        })
        .collect();
    let tests = checked.tests();
    let tasks = checked.tasks();
    let routes = checked
        .routes()
        .into_iter()
        .map(|(p, m, pn)| (p, *m, pn.clone()))
        .collect();
    Ok(BuildOutput {
        name: checked.name.clone(),
        output,
        definitions,
        functions,
        tests,
        tasks,
        routes,
    })
}
