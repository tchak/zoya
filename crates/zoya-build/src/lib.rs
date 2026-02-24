use zoya_codegen::{CodegenOutput, codegen};
use zoya_ir::{DefinitionLookup, HttpMethod, Pathname, QualifiedPath, TypeError};
use zoya_package::Package;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("{0}")]
    Check(#[from] TypeError),
}

pub struct BuildOutput {
    pub name: String,
    pub output: CodegenOutput,
    pub definitions: DefinitionLookup,
    pub functions: Vec<QualifiedPath>,
    pub tests: Vec<QualifiedPath>,
    pub tasks: Vec<QualifiedPath>,
    pub routes: Vec<(QualifiedPath, HttpMethod, Pathname)>,
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
    let functions = checked.fns();
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
