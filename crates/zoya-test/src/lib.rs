use zoya_ir::CheckedPackage;
use zoya_package::QualifiedPath;
use zoya_run::{EvalError, Runner, Value, ValueData};

/// Structured error for test failures.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TestError {
    #[error("panic: {0}")]
    Panic(String),
    #[error("runtime error: {0}")]
    RuntimeError(String),
    #[error("{0}")]
    Failed(String),
    #[error("unexpected test return value: {0}")]
    UnexpectedReturn(String),
}

/// A single test result.
#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    pub path: QualifiedPath,
    pub outcome: Result<(), TestError>,
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
pub struct TestRunner<'a> {
    pub tests: Vec<QualifiedPath>,
    package: &'a CheckedPackage,
    deps: Vec<&'a CheckedPackage>,
}

impl<'a> TestRunner<'a> {
    /// Create a new test runner from a checked package and its dependencies.
    pub fn new(
        package: &'a CheckedPackage,
        deps: impl IntoIterator<Item = &'a CheckedPackage>,
    ) -> Self {
        let deps: Vec<_> = deps.into_iter().collect();
        let tests = package.tests();
        TestRunner {
            tests,
            package,
            deps,
        }
    }

    /// Run all tests, returning a report.
    pub fn run(self) -> Result<TestReport, EvalError> {
        self.execute(|_| {})
    }

    /// Run all tests, calling `on_result` after each one completes.
    pub fn execute(self, mut on_result: impl FnMut(&TestResult)) -> Result<TestReport, EvalError> {
        let runner = Runner::new(self.package, self.deps);
        let mut results = Vec::new();
        for path in self.tests {
            let outcome = run_single_test(&runner, &path);
            let result = TestResult { path, outcome };
            on_result(&result);
            results.push(result);
        }
        Ok(TestReport { results })
    }
}

/// Run a single test function and interpret its result.
fn run_single_test(runner: &Runner, path: &QualifiedPath) -> Result<(), TestError> {
    match runner.run(path.clone(), vec![]) {
        Ok(value) => interpret_test_value(&value),
        Err(EvalError::Panic(msg)) => Err(TestError::Panic(msg)),
        Err(EvalError::RuntimeError(msg)) => Err(TestError::RuntimeError(msg)),
        Err(EvalError::LoadError(e)) => Err(TestError::RuntimeError(e.to_string())),
        Err(EvalError::TypeError(e)) => Err(TestError::RuntimeError(e.to_string())),
    }
}

/// Interpret a test function's return value as pass/fail.
fn interpret_test_value(value: &Value) -> Result<(), TestError> {
    match value {
        Value::Tuple(elems) if elems.is_empty() => Ok(()),
        Value::EnumVariant {
            enum_name,
            variant_name,
            data: ValueData::Tuple(_),
            ..
        } if enum_name == "Result" && variant_name == "Ok" => Ok(()),
        Value::EnumVariant {
            enum_name,
            variant_name,
            data: ValueData::Tuple(values),
            ..
        } if enum_name == "Result" && variant_name == "Err" => {
            let msg = values
                .first()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "test failed".to_string());
            Err(TestError::Failed(msg))
        }
        Value::Task(inner) => interpret_test_value(inner),
        _ => Err(TestError::UnexpectedReturn(format!("{value}"))),
    }
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
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert_eq!(interpret_test_value(&value), Ok(()));
    }

    #[test]
    fn test_interpret_result_err() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            data: ValueData::Tuple(vec![Value::String("something failed".to_string())]),
        };
        assert!(interpret_test_value(&value).is_err());
        assert!(
            interpret_test_value(&value)
                .unwrap_err()
                .to_string()
                .contains("something failed")
        );
    }

    #[test]
    fn test_interpret_wrong_enum_name() {
        let value = Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Option".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        };
        assert!(interpret_test_value(&value).is_err());
    }

    #[test]
    fn test_interpret_task_unit() {
        let value = Value::Task(Box::new(Value::Tuple(vec![])));
        assert_eq!(interpret_test_value(&value), Ok(()));
    }

    #[test]
    fn test_interpret_task_result_ok() {
        let value = Value::Task(Box::new(Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Ok".to_string(),
            data: ValueData::Tuple(vec![Value::Tuple(vec![])]),
        }));
        assert_eq!(interpret_test_value(&value), Ok(()));
    }

    #[test]
    fn test_interpret_task_result_err() {
        let value = Value::Task(Box::new(Value::EnumVariant {
            module: QualifiedPath::root(),
            enum_name: "Result".to_string(),
            variant_name: "Err".to_string(),
            data: ValueData::Tuple(vec![Value::String("async failed".to_string())]),
        }));
        let err = interpret_test_value(&value).unwrap_err();
        assert!(err.to_string().contains("async failed"));
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
                    outcome: Err(TestError::Failed("failed".to_string())),
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
