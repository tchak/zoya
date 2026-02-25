use serde::Serialize;
use zoya_build::BuildOutput;
use zoya_package::QualifiedPath;

/// All data needed to render the dashboard.
#[derive(Clone, Serialize)]
pub struct DashboardData {
    pub package_name: String,
    pub functions: Vec<FunctionInfo>,
    pub tests: Vec<TestInfo>,
    pub jobs: Vec<JobInfo>,
    pub routes: Vec<RouteInfo>,
}

#[derive(Clone, Serialize)]
pub struct FunctionInfo {
    pub name: String,
    pub module: String,
    pub signature: String,
}

#[derive(Clone, Serialize)]
pub struct TestInfo {
    pub name: String,
    pub module: String,
}

#[derive(Clone, Serialize)]
pub struct JobInfo {
    pub name: String,
    pub module: String,
    pub signature: String,
}

#[derive(Clone, Serialize)]
pub struct RouteInfo {
    pub method: String,
    pub pathname: String,
    pub handler: String,
    pub module: String,
    pub signature: String,
}

/// Extract the module portion of a qualified path (segments between "root" and the item name).
fn module_string(path: &QualifiedPath) -> String {
    let segments = path.segments();
    // segments: ["root", ...modules..., "item_name"]
    if segments.len() <= 2 {
        String::new()
    } else {
        segments[1..segments.len() - 1].join("::")
    }
}

impl DashboardData {
    pub fn from_output(output: &BuildOutput) -> Self {
        let functions = output
            .functions
            .iter()
            .filter_map(|(path, _)| {
                let func = output.definitions.get_function(path)?;
                Some(FunctionInfo {
                    name: path.last().to_string(),
                    module: module_string(path),
                    signature: func.pretty(),
                })
            })
            .collect();

        let tests = output
            .tests
            .iter()
            .map(|path| TestInfo {
                name: path.last().to_string(),
                module: module_string(path),
            })
            .collect();

        let jobs = output
            .jobs
            .iter()
            .filter_map(|(path, _)| {
                let func = output.definitions.get_function(path)?;
                Some(JobInfo {
                    name: path.last().to_string(),
                    module: module_string(path),
                    signature: func.pretty(),
                })
            })
            .collect();

        let routes = output
            .routes
            .iter()
            .filter_map(|(path, method, pathname)| {
                let func = output.definitions.get_function(path)?;
                Some(RouteInfo {
                    method: method.to_string(),
                    pathname: pathname.to_string(),
                    handler: path.last().to_string(),
                    module: module_string(path),
                    signature: func.pretty(),
                })
            })
            .collect();

        DashboardData {
            package_name: output.name.clone(),
            functions,
            tests,
            jobs,
            routes,
        }
    }
}
