use zoya_check::check;
use zoya_loader::load_package;
use zoya_test::TestRunner;

fn run_tests(source: &str) -> zoya_test::TestReport {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.zy");
    std::fs::write(&file, source).unwrap();

    let std = zoya_std::std();
    let package = load_package(&file, zoya_loader::Mode::Test)
        .map_err(|e| e.map_path(|p| p.to_string()))
        .unwrap();
    let checked = check(&package, &[std]).unwrap();
    TestRunner::new(&checked, [std]).run().unwrap()
}

#[test]
fn test_all_pass() {
    let report = run_tests(
        r#"
        #[test]
        fn test_one() -> () { () }

        #[test]
        fn test_two() -> () { () }
        "#,
    );
    assert_eq!(report.total(), 2);
    assert_eq!(report.passed(), 2);
    assert_eq!(report.failed(), 0);
    assert!(report.is_success());
}

#[test]
fn test_mix_pass_fail() {
    let report = run_tests(
        r#"
        #[test]
        fn test_ok() -> () { () }

        #[test]
        fn test_panic() -> () { panic("boom") }
        "#,
    );
    assert_eq!(report.total(), 2);
    assert_eq!(report.passed(), 1);
    assert_eq!(report.failed(), 1);
    assert!(!report.is_success());
}

#[test]
fn test_no_tests() {
    let report = run_tests("pub fn main() -> Int { 42 }");
    assert_eq!(report.total(), 0);
    assert!(report.is_success());
}

#[test]
fn test_result_err_fails() {
    let report = run_tests(
        r#"
        #[test]
        fn test_err() -> Result<(), String> { Err("something wrong") }
        "#,
    );
    assert_eq!(report.total(), 1);
    assert_eq!(report.failed(), 1);
    assert!(!report.is_success());
    assert!(
        report.results[0]
            .outcome
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("something wrong")
    );
}

#[test]
fn test_result_ok_passes() {
    let report = run_tests(
        r#"
        #[test]
        fn test_ok() -> Result<(), String> { Ok(()) }
        "#,
    );
    assert_eq!(report.total(), 1);
    assert_eq!(report.passed(), 1);
    assert!(report.is_success());
}
