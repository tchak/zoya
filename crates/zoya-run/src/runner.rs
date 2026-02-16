use std::path::Path;

use zoya_check::check;
use zoya_codegen::{codegen, format_export_path};
use zoya_ir::{CheckedPackage, DefinitionLookup};
use zoya_loader::{MemorySource, load_memory_package, load_package};
use zoya_package::QualifiedPath;

use crate::eval::{self, EnumValueFields, EvalError, Value};

/// Which function to invoke inside a checked package.
enum EntryPoint {
    /// Run `main()` in the root module or a named submodule.
    Main(Option<String>),
    /// Run an arbitrary function by its full qualified path.
    Entry(QualifiedPath),
}

/// Entry point for building a run configuration.
///
/// Use `Runner::new()` then choose an input source:
/// - `.package(pkg, deps)` → `PackageRunner`
/// - `.path(p)` → `PathRunner`
/// - `.source(s)` → `SourceRunner`
#[derive(Default)]
pub struct Runner;

impl Runner {
    /// Create a new runner.
    pub fn new() -> Self {
        Runner
    }

    /// Run an already-checked package with its dependencies.
    pub fn package<'a>(
        self,
        package: &'a CheckedPackage,
        deps: impl IntoIterator<Item = &'a CheckedPackage>,
    ) -> PackageRunner<'a> {
        PackageRunner {
            package,
            deps: deps.into_iter().collect(),
            entry_point: EntryPoint::Main(None),
        }
    }

    /// Load, check, and run a `.zy` file at the given path.
    pub fn path(self, path: &Path) -> PathRunner {
        PathRunner {
            path: path.to_path_buf(),
            mode: zoya_loader::Mode::Dev,
        }
    }

    /// Load, check, and discover tests in a `.zy` file or package.
    pub fn test(self, path: &Path) -> Result<TestRunner, EvalError> {
        let std = zoya_std::std();
        let package = load_package(path, zoya_loader::Mode::Test)
            .map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;
        let checked =
            check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;

        let mut tests: Vec<QualifiedPath> = checked
            .items
            .iter()
            .filter(|(_, func)| func.is_test)
            .map(|(path, _)| path.clone())
            .collect();
        tests.sort_by_key(|a| a.to_string());

        Ok(TestRunner {
            tests,
            checked,
            std,
        })
    }

    /// Load, check, and run source code from a string.
    pub fn source(self, source: &str) -> SourceRunner {
        SourceRunner {
            source: source.to_string(),
            mode: zoya_loader::Mode::Dev,
        }
    }
}

/// Runner configured with a pre-checked package.
pub struct PackageRunner<'a> {
    package: &'a CheckedPackage,
    deps: Vec<&'a CheckedPackage>,
    entry_point: EntryPoint,
}

impl<'a> PackageRunner<'a> {
    /// Select a submodule whose `main()` to run (e.g., `"repl"`).
    pub fn main_module(mut self, module: impl Into<String>) -> Self {
        self.entry_point = EntryPoint::Main(Some(module.into()));
        self
    }

    /// Select an arbitrary function to run by its full qualified path.
    pub fn entry(mut self, path: QualifiedPath) -> Self {
        self.entry_point = EntryPoint::Entry(path);
        self
    }

    /// Execute the package and return the result.
    pub fn run(self) -> Result<Value, EvalError> {
        run_checked(self.package, &self.deps, &self.entry_point)
    }
}

/// Runner configured to load and run a file.
pub struct PathRunner {
    path: std::path::PathBuf,
    mode: zoya_loader::Mode,
}

impl PathRunner {
    /// Set the compilation mode (default: `Mode::Dev`).
    pub fn mode(mut self, mode: zoya_loader::Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Load, check, and execute the file.
    pub fn run(self) -> Result<Value, EvalError> {
        let std = zoya_std::std();
        let package = load_package(&self.path, self.mode)
            .map_err(|e| EvalError::RuntimeError(format!("error: {}", e)))?;
        let checked =
            check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        run_checked(&checked, &[std], &EntryPoint::Main(None))
    }
}

/// Runner configured to compile and run a source string.
pub struct SourceRunner {
    source: String,
    mode: zoya_loader::Mode,
}

impl SourceRunner {
    /// Set the compilation mode (default: `Mode::Dev`).
    pub fn mode(mut self, mode: zoya_loader::Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Compile and execute the source string.
    pub fn run(self) -> Result<Value, EvalError> {
        let std = zoya_std::std();
        let mem_source = MemorySource::new().with_module("root", &self.source);
        let package = load_memory_package(&mem_source, self.mode)
            .map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        let checked =
            check(&package, &[std]).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        run_checked(&checked, &[std], &EntryPoint::Main(None))
    }
}

/// Load, check, and run source code from a string (convenience function).
pub fn run_source(source: &str) -> Result<Value, EvalError> {
    Runner::new().source(source).run()
}

/// Load, check, and run a `.zy` file (convenience function).
pub fn run_path(path: &Path) -> Result<Value, EvalError> {
    Runner::new().path(path).run()
}

/// A single test result.
#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    pub path: QualifiedPath,
    pub outcome: Result<(), String>,
}

/// Summary of all test results.
#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    pub results: Vec<TestResult>,
}

impl TestReport {
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.outcome.is_ok()).count()
    }

    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| r.outcome.is_err()).count()
    }

    pub fn total(&self) -> usize {
        self.results.len()
    }

    pub fn is_success(&self) -> bool {
        self.results.iter().all(|r| r.outcome.is_ok())
    }
}

/// A test run: tests discovered, ready to execute.
pub struct TestRunner {
    pub tests: Vec<QualifiedPath>,
    checked: CheckedPackage,
    std: &'static CheckedPackage,
}

impl TestRunner {
    /// Run all tests, returning a report.
    pub fn run(self) -> Result<TestReport, EvalError> {
        self.execute(|_| {})
    }

    /// Run all tests, calling `on_result` after each one completes.
    pub fn execute(self, mut on_result: impl FnMut(&TestResult)) -> Result<TestReport, EvalError> {
        let mut results = Vec::new();
        for path in self.tests {
            let outcome = run_single_test(&self.checked, &[self.std], &path);
            let result = TestResult { path, outcome };
            on_result(&result);
            results.push(result);
        }
        Ok(TestReport { results })
    }
}

/// Run a single test function and interpret its result.
fn run_single_test(
    package: &CheckedPackage,
    deps: &[&CheckedPackage],
    path: &QualifiedPath,
) -> Result<(), String> {
    match run_checked(package, deps, &EntryPoint::Entry(path.clone())) {
        Ok(value) => interpret_test_value(&value),
        Err(EvalError::Panic(msg)) => Err(format!("panic: {msg}")),
        Err(EvalError::RuntimeError(msg)) => Err(format!("runtime error: {msg}")),
    }
}

/// Interpret a test function's return value as pass/fail.
fn interpret_test_value(value: &Value) -> Result<(), String> {
    match value {
        Value::Tuple(elems) if elems.is_empty() => Ok(()),
        Value::Enum {
            enum_name,
            variant_name,
            fields: EnumValueFields::Tuple(_),
            ..
        } if enum_name == "Result" && variant_name == "Ok" => Ok(()),
        Value::Enum {
            enum_name,
            variant_name,
            fields: EnumValueFields::Tuple(values),
            ..
        } if enum_name == "Result" && variant_name == "Err" => {
            let msg = values
                .first()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "test failed".to_string());
            Err(msg)
        }
        _ => Err(format!("unexpected test return value: {value}")),
    }
}

/// Internal: execute an already-checked package.
fn run_checked(
    package: &CheckedPackage,
    deps: &[&CheckedPackage],
    entry_point: &EntryPoint,
) -> Result<Value, EvalError> {
    // Resolve the function path from the entry point
    let function_path = match entry_point {
        EntryPoint::Main(module) => {
            let module_path = match module {
                Some(m) => QualifiedPath::root().child(m),
                None => QualifiedPath::root(),
            };
            module_path.child("main")
        }
        EntryPoint::Entry(path) => path.clone(),
    };

    // Find the function in the package definitions
    let func_def = package
        .definitions
        .get(&function_path)
        .and_then(|d| d.as_function())
        .ok_or_else(|| {
            EvalError::RuntimeError(match entry_point {
                EntryPoint::Main(_) => {
                    let module_path = function_path.parent().unwrap_or_else(QualifiedPath::root);
                    format!("no pub fn main() found in {}", module_path)
                }
                EntryPoint::Entry(path) => format!("function {} not found", path),
            })
        })?;

    if !func_def.params.is_empty() {
        return Err(EvalError::RuntimeError(format!(
            "{}() must not take any parameters",
            function_path.last()
        )));
    }

    let return_type = func_def.return_type.clone();

    // Build type lookup for resolving recursive type stubs
    let type_lookup = DefinitionLookup::from_packages(package, deps);

    // Generate single concatenated JS
    let output = codegen(package, deps);

    // Build the entry function name using the package name
    let entry_func = format_export_path(&function_path, &package.name);

    // Create runtime (no module system needed)
    let (_runtime, context) =
        eval::create_runtime().map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    // Evaluate the script and call the entry function
    context
        .with(|ctx| eval::eval_script(&ctx, &output.code, &entry_func, return_type, &type_lookup))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpret_unit_return() {
        let value = Value::Tuple(vec![]);
        assert_eq!(interpret_test_value(&value), Ok(()));
    }

    #[test]
    fn test_interpret_result_ok_unit() {
        let value = Value::Enum {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            fields: EnumValueFields::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert_eq!(interpret_test_value(&value), Ok(()));
    }

    #[test]
    fn test_interpret_result_err() {
        let value = Value::Enum {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            fields: EnumValueFields::Tuple(vec![Value::String("something failed".to_string())]),
        };
        assert!(interpret_test_value(&value).is_err());
        assert!(
            interpret_test_value(&value)
                .unwrap_err()
                .contains("something failed")
        );
    }

    #[test]
    fn test_interpret_wrong_enum_name() {
        let value = Value::Enum {
            module: QualifiedPath::root(),
            enum_name: "Option".to_string(),
            variant_name: "Ok".to_string(),
            fields: EnumValueFields::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert!(interpret_test_value(&value).is_err());
    }

    #[test]
    fn test_interpret_unexpected_value() {
        let value = Value::Int(42);
        assert!(interpret_test_value(&value).is_err());
    }

    #[test]
    fn test_report_all_pass() {
        let report = TestReport {
            results: vec![
                TestResult {
                    path: QualifiedPath::root().child("test_a"),
                    outcome: Ok(()),
                },
                TestResult {
                    path: QualifiedPath::root().child("test_b"),
                    outcome: Ok(()),
                },
            ],
        };
        assert_eq!(report.passed(), 2);
        assert_eq!(report.failed(), 0);
        assert_eq!(report.total(), 2);
        assert!(report.is_success());
    }

    #[test]
    fn test_report_with_failure() {
        let report = TestReport {
            results: vec![
                TestResult {
                    path: QualifiedPath::root().child("test_a"),
                    outcome: Ok(()),
                },
                TestResult {
                    path: QualifiedPath::root().child("test_b"),
                    outcome: Err("failed".to_string()),
                },
            ],
        };
        assert_eq!(report.passed(), 1);
        assert_eq!(report.failed(), 1);
        assert_eq!(report.total(), 2);
        assert!(!report.is_success());
    }

    #[test]
    fn test_report_empty() {
        let report = TestReport { results: vec![] };
        assert_eq!(report.passed(), 0);
        assert_eq!(report.failed(), 0);
        assert_eq!(report.total(), 0);
        assert!(report.is_success());
    }
}
