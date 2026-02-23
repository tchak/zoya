use zoya_ir::{CheckedPackage, HttpMethod, pretty_type};
use zoya_package::QualifiedPath;

/// All data needed to render the dashboard.
pub struct DashboardData {
    pub package_name: String,
    pub functions: Vec<FunctionInfo>,
    pub tests: Vec<TestInfo>,
    pub tasks: Vec<TaskInfo>,
    pub routes: Vec<RouteInfo>,
}

pub struct FunctionInfo {
    pub name: String,
    pub module: String,
    pub signature: String,
}

pub struct TestInfo {
    pub name: String,
    pub module: String,
}

pub struct TaskInfo {
    pub name: String,
    pub module: String,
    pub signature: String,
}

pub struct RouteInfo {
    pub method: String,
    pub pathname: String,
    pub handler: String,
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
fn format_signature(func: &zoya_ir::TypedFunction) -> String {
    let params: Vec<String> = func.params.iter().map(|(_, ty)| pretty_type(ty)).collect();
    let ret = pretty_type(&func.return_type);

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
    pub fn from_package(checked: &CheckedPackage) -> Self {
        let functions = checked
            .fns()
            .into_iter()
            .filter_map(|path| {
                let func = checked.items.get(&path)?;
                Some(FunctionInfo {
                    name: path.last().to_string(),
                    module: module_string(&path),
                    signature: format_signature(func),
                })
            })
            .collect();

        let tests = checked
            .tests()
            .into_iter()
            .map(|path| TestInfo {
                name: path.last().to_string(),
                module: module_string(&path),
            })
            .collect();

        let tasks = checked
            .tasks()
            .into_iter()
            .filter_map(|path| {
                let func = checked.items.get(&path)?;
                Some(TaskInfo {
                    name: path.last().to_string(),
                    module: module_string(&path),
                    signature: format_signature(func),
                })
            })
            .collect();

        let routes = checked
            .routes()
            .into_iter()
            .filter_map(|(path, method, pathname)| {
                let func = checked.items.get(&path)?;
                Some(RouteInfo {
                    method: method_to_string(method).to_string(),
                    pathname: pathname.to_string(),
                    handler: path.last().to_string(),
                    signature: format_signature(func),
                })
            })
            .collect();

        DashboardData {
            package_name: checked.name.clone(),
            functions,
            tests,
            tasks,
            routes,
        }
    }
}
