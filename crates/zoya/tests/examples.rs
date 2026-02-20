use std::path::Path;

use zoya_check::check;
use zoya_loader::load_package;
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
    let std = zoya_std::std();
    let package = load_package(&dir, zoya_loader::Mode::Test)
        .map_err(|e| e.map_path(|p| p.to_string()))
        .unwrap();
    let checked = check(&package, &[std]).unwrap();
    let report = TestRunner::new(&checked, [std]).run().unwrap();
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
    let std = zoya_std::std();
    let package = load_package(&dir, zoya_loader::Mode::Test)
        .map_err(|e| e.map_path(|p| p.to_string()))
        .unwrap();
    let checked = check(&package, &[std]).unwrap();
    let report = TestRunner::new(&checked, [std]).run().unwrap();
    assert!(
        report.is_success(),
        "{} test(s) failed out of {}",
        report.failed(),
        report.results.len()
    );
}
