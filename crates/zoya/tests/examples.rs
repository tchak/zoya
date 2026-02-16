use std::path::Path;

use zoya_run::Runner;

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
    let runner = Runner::new().test(&dir).unwrap();
    let report = runner.run().unwrap();
    assert!(
        report.is_success(),
        "{} test(s) failed out of {}",
        report.failed(),
        report.results.len()
    );
}
