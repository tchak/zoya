use std::path::Path;

use zoya_test::TestRunner;

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn test_examples_algorithms() {
    let dir = project_root().join("examples/algorithms");
    let output = zoya_build::build_from_path(&dir, zoya_build::Mode::Test).unwrap();
    let report = TestRunner::new(&output).run().unwrap();
    assert!(
        report.is_success(),
        "{} test(s) failed out of {}",
        report.failed(),
        report.results.len()
    );
}

#[test]
fn test_examples_std_tests() {
    let dir = project_root().join("examples/std-tests");
    let output = zoya_build::build_from_path(&dir, zoya_build::Mode::Test).unwrap();
    let report = TestRunner::new(&output).run().unwrap();
    assert!(
        report.is_success(),
        "{} test(s) failed out of {}",
        report.failed(),
        report.results.len()
    );
}

#[test]
fn test_examples_tests() {
    let dir = project_root().join("examples/tests");
    let output = zoya_build::build_from_path(&dir, zoya_build::Mode::Test).unwrap();
    let report = TestRunner::new(&output).run().unwrap();
    assert!(
        report.is_success(),
        "{} test(s) failed out of {}",
        report.failed(),
        report.results.len()
    );
}
