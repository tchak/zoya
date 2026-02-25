use zoya_build::BuildOutput;
use zoya_package::QualifiedPath;
use zoya_run::{EvalError, TerminationError};

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
    output: &'a BuildOutput,
}

impl<'a> TestRunner<'a> {
    /// Create a new test runner from a build output.
    pub fn new(output: &'a BuildOutput) -> Self {
        let tests = output.tests.clone();
        TestRunner { tests, output }
    }

    /// Run all tests, returning a report.
    pub fn run(self) -> Result<TestReport, EvalError> {
        self.execute(|_| {})
    }

    /// Run all tests, calling `on_result` after each one completes.
    pub fn execute(self, mut on_result: impl FnMut(&TestResult)) -> Result<TestReport, EvalError> {
        let mut results = Vec::new();
        for path in self.tests {
            let outcome = run_single_test(self.output, &path);
            let result = TestResult { path, outcome };
            on_result(&result);
            results.push(result);
        }
        Ok(TestReport { results })
    }
}

/// Run a single test function and interpret its result.
fn run_single_test(output: &BuildOutput, path: &QualifiedPath) -> Result<(), TestError> {
    match zoya_run::run(output, path, &[]) {
        Ok((value, _jobs)) => value.termination().map_err(|e| match e {
            TerminationError::Failed(msg) => TestError::Failed(msg),
            TerminationError::UnexpectedReturn(msg) => TestError::UnexpectedReturn(msg),
        }),
        Err(EvalError::Panic(msg)) => Err(TestError::Panic(msg)),
        Err(EvalError::RuntimeError(msg)) => Err(TestError::RuntimeError(msg)),
        Err(EvalError::LoadError(e)) => Err(TestError::RuntimeError(e.to_string())),
        Err(EvalError::TypeError(e)) => Err(TestError::RuntimeError(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
