use leptos::IntoView;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::data::{DashboardData, FunctionInfo, RouteInfo, TaskInfo, TestInfo};

/// Group items by module path, sorted with root (empty module) first, then alphabetically.
/// Items within each group are sorted by the provided sort function.
fn group_by_module<T>(
    mut items: Vec<T>,
    module_fn: impl Fn(&T) -> &str,
    sort_fn: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> Vec<(String, Vec<T>)> {
    items.sort_by(|a, b| {
        let ma = module_fn(a);
        let mb = module_fn(b);
        match (ma.is_empty(), mb.is_empty()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => ma.cmp(mb).then_with(|| sort_fn(a, b)),
        }
    });

    let mut groups: Vec<(String, Vec<T>)> = Vec::new();
    for item in items {
        let module = module_fn(&item).to_string();
        if let Some(last) = groups.last_mut() {
            if last.0 == module {
                last.1.push(item);
                continue;
            }
        }
        groups.push((module, vec![item]));
    }
    groups
}

/// Render the full dashboard page to an HTML string.
pub fn render_page(data: &DashboardData) -> String {
    let package_name = data.package_name.clone();
    let fn_count = data.functions.len();
    let test_count = data.tests.len();
    let task_count = data.tasks.len();
    let route_count = data.routes.len();

    let functions: Vec<FunctionInfo> = data
        .functions
        .iter()
        .map(|f| FunctionInfo {
            name: f.name.clone(),
            module: f.module.clone(),
            signature: f.signature.clone(),
        })
        .collect();
    let tests: Vec<TestInfo> = data
        .tests
        .iter()
        .map(|t| TestInfo {
            name: t.name.clone(),
            module: t.module.clone(),
        })
        .collect();
    let tasks: Vec<TaskInfo> = data
        .tasks
        .iter()
        .map(|t| TaskInfo {
            name: t.name.clone(),
            module: t.module.clone(),
            signature: t.signature.clone(),
        })
        .collect();
    let routes: Vec<RouteInfo> = data
        .routes
        .iter()
        .map(|r| RouteInfo {
            method: r.method.clone(),
            pathname: r.pathname.clone(),
            handler: r.handler.clone(),
            module: r.module.clone(),
            signature: r.signature.clone(),
        })
        .collect();

    let body = view! {
        <header class="mb-8">
            <h1 class="text-3xl font-bold text-gray-900">{package_name}</h1>
            <p class="text-gray-500 mt-1">"Package Dashboard"</p>
        </header>

        <div class="flex gap-3 mb-8">
            <Badge label="Functions" count=fn_count />
            <Badge label="Tests" count=test_count />
            <Badge label="Tasks" count=task_count />
            <Badge label="Routes" count=route_count />
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <FunctionsCard functions=functions />
            <TestsCard tests=tests />
            <TasksCard tasks=tasks />
            <RoutesCard routes=routes />
        </div>
    }
    .to_html();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Zoya Dashboard</title>
<script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-gray-50 min-h-screen p-8 max-w-5xl mx-auto">
{body}
</body>
</html>"#
    )
}

#[component]
fn Badge(label: &'static str, count: usize) -> impl IntoView {
    view! {
        <span class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-sm font-medium bg-white border border-gray-200 text-gray-700">
            {label}
            <span class="bg-gray-100 text-gray-600 px-1.5 py-0.5 rounded-full text-xs font-semibold">
                {count}
            </span>
        </span>
    }
}

#[component]
fn Card(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class="bg-white rounded-lg border border-gray-200 p-5">
            <h2 class="text-lg font-semibold text-gray-900 mb-3">{title}</h2>
            {children()}
        </div>
    }
}

#[component]
fn EmptyState(message: &'static str) -> impl IntoView {
    view! {
        <p class="text-gray-400 text-sm italic">{message}</p>
    }
}

#[component]
fn ModuleHeader(module: String) -> impl IntoView {
    (!module.is_empty()).then(|| {
        view! {
            <div class="text-xs font-medium text-gray-400 mb-1.5">{module}</div>
        }
    })
}

#[component]
fn FunctionsCard(functions: Vec<FunctionInfo>) -> impl IntoView {
    view! {
        <Card title="Functions">
            {if functions.is_empty() {
                view! { <EmptyState message="No functions" /> }.into_any()
            } else {
                let groups = group_by_module(
                    functions,
                    |f| f.module.as_str(),
                    |a, b| a.name.cmp(&b.name),
                );
                view! {
                    <div class="space-y-4">
                        {groups
                            .into_iter()
                            .map(|(module, items)| {
                                view! {
                                    <div>
                                        <ModuleHeader module=module />
                                        <ul class="space-y-1.5">
                                            {items
                                                .into_iter()
                                                .map(|f| {
                                                    view! {
                                                        <li class="flex items-baseline">
                                                            <code class="text-sm text-indigo-600 font-mono">{f.name}</code>
                                                            <code class="text-xs text-gray-400 font-mono ml-2">{f.signature}</code>
                                                        </li>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()}
                    </div>
                }
                .into_any()
            }}
        </Card>
    }
}

#[component]
fn TestsCard(tests: Vec<TestInfo>) -> impl IntoView {
    view! {
        <Card title="Tests">
            {if tests.is_empty() {
                view! { <EmptyState message="No tests" /> }.into_any()
            } else {
                let groups = group_by_module(
                    tests,
                    |t| t.module.as_str(),
                    |a, b| a.name.cmp(&b.name),
                );
                view! {
                    <div class="space-y-4">
                        {groups
                            .into_iter()
                            .map(|(module, items)| {
                                view! {
                                    <div>
                                        <ModuleHeader module=module />
                                        <ul class="space-y-1.5">
                                            {items
                                                .into_iter()
                                                .map(|t| {
                                                    view! {
                                                        <li class="flex items-baseline">
                                                            <code class="text-sm text-gray-700 font-mono">{t.name}</code>
                                                        </li>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()}
                    </div>
                }
                .into_any()
            }}
        </Card>
    }
}

#[component]
fn TasksCard(tasks: Vec<TaskInfo>) -> impl IntoView {
    view! {
        <Card title="Tasks">
            {if tasks.is_empty() {
                view! { <EmptyState message="No tasks" /> }.into_any()
            } else {
                let groups = group_by_module(
                    tasks,
                    |t| t.module.as_str(),
                    |a, b| a.name.cmp(&b.name),
                );
                view! {
                    <div class="space-y-4">
                        {groups
                            .into_iter()
                            .map(|(module, items)| {
                                view! {
                                    <div>
                                        <ModuleHeader module=module />
                                        <ul class="space-y-1.5">
                                            {items
                                                .into_iter()
                                                .map(|t| {
                                                    view! {
                                                        <li class="flex items-baseline">
                                                            <code class="text-sm text-amber-600 font-mono">{t.name}</code>
                                                            <code class="text-xs text-gray-400 font-mono ml-2">{t.signature}</code>
                                                        </li>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()}
                    </div>
                }
                .into_any()
            }}
        </Card>
    }
}

#[component]
fn RoutesCard(routes: Vec<RouteInfo>) -> impl IntoView {
    view! {
        <Card title="Routes">
            {if routes.is_empty() {
                view! { <EmptyState message="No routes" /> }.into_any()
            } else {
                let groups = group_by_module(
                    routes,
                    |r| r.module.as_str(),
                    |a, b| a.pathname.cmp(&b.pathname),
                );
                view! {
                    <div class="space-y-4">
                        {groups
                            .into_iter()
                            .map(|(module, items)| {
                                view! {
                                    <div>
                                        <ModuleHeader module=module />
                                        <ul class="space-y-1.5">
                                            {items
                                                .into_iter()
                                                .map(|r| {
                                                    let badge_class = match r.method.as_str() {
                                                        "GET" => "bg-green-100 text-green-700",
                                                        "POST" => "bg-blue-100 text-blue-700",
                                                        "PUT" => "bg-yellow-100 text-yellow-700",
                                                        "PATCH" => "bg-orange-100 text-orange-700",
                                                        "DELETE" => "bg-red-100 text-red-700",
                                                        _ => "bg-gray-100 text-gray-700",
                                                    };
                                                    view! {
                                                        <li class="flex items-baseline gap-2">
                                                            <span class=format!(
                                                                "inline-block px-1.5 py-0.5 rounded text-xs font-bold {badge_class}",
                                                            )>{r.method}</span>
                                                            <code class="text-sm text-gray-900 font-mono">{r.pathname}</code>
                                                            <code class="text-xs text-gray-400 font-mono">{r.handler}</code>
                                                            <code class="text-xs text-gray-400 font-mono">{r.signature}</code>
                                                        </li>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                }
                            })
                            .collect::<Vec<_>>()}
                    </div>
                }
                .into_any()
            }}
        </Card>
    }
}
