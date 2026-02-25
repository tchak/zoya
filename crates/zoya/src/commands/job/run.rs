use std::path::Path;

use anyhow::{Result, anyhow, bail};
use zoya_build::{Mode, build_from_path};
use zoya_package::QualifiedPath;
use zoya_run::Value;

/// Run a `#[job]` function with CLI arguments parsed by type
pub fn execute(path: &Path, job_name: &str, args: &[String], mode: Mode) -> Result<()> {
    let output = build_from_path(path, mode)?;

    // Build qualified path from job name (e.g. "deploy" or "utils::migrate")
    let mut job_path = QualifiedPath::root();
    for segment in job_name.split("::") {
        job_path = job_path.child(segment);
    }

    // Verify this is a known job
    if !output.jobs.iter().any(|(p, _)| p == &job_path) {
        let available: Vec<String> = output
            .jobs
            .iter()
            .map(|(p, _)| {
                p.segments()
                    .iter()
                    .skip_while(|s| s.as_str() == "root")
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect();
        let hint = if available.is_empty() {
            "no jobs found in this package".to_string()
        } else {
            format!("available jobs: {}", available.join(", "))
        };
        bail!("job '{job_name}' not found ({hint})");
    }

    // Get the function's typed signature
    let func = output
        .definitions
        .get_function(&job_path)
        .ok_or_else(|| anyhow!("job '{job_name}' not found in definitions"))?;

    // Validate argument count
    if args.len() != func.params.len() {
        bail!(
            "job '{job_name}' expects {} argument(s), got {}",
            func.params.len(),
            args.len()
        );
    }

    // Parse each argument guided by the parameter type
    let mut parsed_args = Vec::with_capacity(args.len());
    for (i, (arg_str, param_type)) in args.iter().zip(func.params.iter()).enumerate() {
        let value = Value::parse(arg_str, param_type, &output.definitions)
            .map_err(|e| anyhow!("argument {} : {e}", i + 1))?;
        parsed_args.push(value);
    }

    // Run the job function
    let (result, _jobs) = zoya_run::run(&output, &job_path, &parsed_args)?;

    // Handle result: jobs return () or Result<(), E>
    use zoya_run::ValueData;
    match &result {
        Value::Tuple(elems) if elems.is_empty() => Ok(()),
        Value::EnumVariant {
            variant_name,
            data: ValueData::Tuple(fields),
            ..
        } if variant_name == "Err" && fields.len() == 1 => {
            bail!("{}", fields[0]);
        }
        Value::EnumVariant { variant_name, .. } if variant_name == "Ok" => Ok(()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_job_simple() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn add(x: Int, y: Int) -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "add", &["1".into(), "2".into()], Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_job_string_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn greet(name: String) -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "greet", &["world".into()], Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_job_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn deploy() -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "missing", &[], Mode::Dev);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_run_job_wrong_arg_count() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn add(x: Int, y: Int) -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "add", &["1".into()], Mode::Dev);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("expects 2 argument(s), got 1")
        );
    }

    #[test]
    fn test_run_job_invalid_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn double(n: Int) -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "double", &["abc".into()], Mode::Dev);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("argument 1"));
    }

    #[test]
    fn test_run_job_no_args() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn deploy() -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "deploy", &[], Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_job_bool_arg() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn identity(flag: Bool) -> () { () }
            "#,
        )
        .unwrap();

        let result = execute(&file, "identity", &["true".into()], Mode::Dev);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_job_result_err_propagates() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn failing() -> Result<(), String> { Result::Err("something went wrong") }
            "#,
        )
        .unwrap();

        let result = execute(&file, "failing", &[], Mode::Dev);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("something went wrong")
        );
    }

    #[test]
    fn test_run_job_result_ok_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(
            &file,
            r#"
            #[job]
            pub fn succeeding() -> Result<(), String> { Result::Ok(()) }
            "#,
        )
        .unwrap();

        let result = execute(&file, "succeeding", &[], Mode::Dev);
        assert!(result.is_ok());
    }
}
