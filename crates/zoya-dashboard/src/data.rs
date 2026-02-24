use serde::Serialize;
use zoya_build::{BuildOutput, FunctionType, HttpMethod};
use zoya_package::QualifiedPath;

/// All data needed to render the dashboard.
#[derive(Clone, Serialize)]
pub struct DashboardData {
    pub package_name: String,
    pub functions: Vec<FunctionInfo>,
    pub tests: Vec<TestInfo>,
    pub tasks: Vec<TaskInfo>,
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
pub struct TaskInfo {
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

/// Format function parameters and return type into a signature string.
fn format_signature(func: &FunctionType) -> String {
    let params: Vec<String> = func.params.iter().map(|ty| ty.pretty()).collect();
    let ret = func.return_type.pretty();

    if params.is_empty() {
        format!("() -> {ret}")
    } else {
        format!("({}) -> {ret}", params.join(", "))
    }
}

fn method_to_string(method: &HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
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
                    signature: format_signature(func),
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

        let tasks = output
            .tasks
            .iter()
            .filter_map(|path| {
                let func = output.definitions.get_function(path)?;
                Some(TaskInfo {
                    name: path.last().to_string(),
                    module: module_string(path),
                    signature: format_signature(func),
                })
            })
            .collect();

        let routes = output
            .routes
            .iter()
            .filter_map(|(path, method, pathname)| {
                let func = output.definitions.get_function(path)?;
                Some(RouteInfo {
                    method: method_to_string(method).to_string(),
                    pathname: pathname.to_string(),
                    handler: path.last().to_string(),
                    module: module_string(path),
                    signature: format_signature(func),
                })
            })
            .collect();

        DashboardData {
            package_name: output.name.clone(),
            functions,
            tests,
            tasks,
            routes,
        }
    }
}
