use std::path::Path;

use zoya_check::check;
use zoya_codegen::codegen;

/// Compile a file to JavaScript without executing
pub fn execute(path: &Path, output: Option<&Path>) -> Result<(), String> {
    // Load and parse package
    let pkg = zoya_loader::load_package(path).map_err(|e| format!("error: {}", e))?;

    // Type check entire package
    let checked_pkg = check(&pkg).map_err(|e| format!("error: {}", e))?;

    // Resolve output path: CLI arg > package.toml output > error
    let out_path = output
        .map(|p| p.to_path_buf())
        .or(checked_pkg.output.clone())
        .ok_or_else(|| {
            "no output path specified\nhint: use --output <path> or set output in package.toml"
                .to_string()
        })?;

    // Generate JS code
    let output_data = codegen(&checked_pkg);

    // Create parent directories if needed
    if let Some(parent) = out_path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "error: failed to create directory '{}': {}",
                parent.display(),
                e
            )
        })?;
    }

    // Write output
    std::fs::write(&out_path, &output_data.code)
        .map_err(|e| format!("error: failed to write file '{}': {}", out_path.display(), e))?;

    eprintln!("✓ Built: {}", out_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_execute_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.zoya");
        let output = dir.path().join("test.js");
        std::fs::write(&input, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&input, Some(&output));
        assert!(result.is_ok());
        assert!(output.exists());
        let js = std::fs::read_to_string(&output).unwrap();
        assert!(js.contains("function $root$main()"));
    }

    #[test]
    fn test_execute_no_output_errors() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&file, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no output path"));
    }

    #[test]
    fn test_execute_uses_package_output() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml with output
        std::fs::write(
            dir.path().join("package.toml"),
            "name = \"test-project\"\noutput = \"build/out.js\"\n",
        )
        .unwrap();

        // Create main file at default location
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.zoya"),
            "pub fn main() -> Int { 42 }",
        )
        .unwrap();

        let result = execute(dir.path(), None);
        assert!(result.is_ok());

        let output_path = dir.path().join("build/out.js");
        assert!(output_path.exists());
    }

    #[test]
    fn test_execute_cli_output_overrides_package() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml with output
        std::fs::write(
            dir.path().join("package.toml"),
            "name = \"test-project\"\noutput = \"build/pkg.js\"\n",
        )
        .unwrap();

        // Create main file at default location
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.zoya"),
            "pub fn main() -> Int { 42 }",
        )
        .unwrap();

        let cli_output = dir.path().join("custom.js");
        let result = execute(dir.path(), Some(&cli_output));
        assert!(result.is_ok());

        // CLI output should be used, not package.toml output
        assert!(cli_output.exists());
        let pkg_output = dir.path().join("build/pkg.js");
        assert!(!pkg_output.exists());
    }

    #[test]
    fn test_execute_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.zoya");
        let output = dir.path().join("deep/nested/out.js");
        std::fs::write(&input, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&input, Some(&output));
        assert!(result.is_ok());
        assert!(output.exists());
    }

    #[test]
    fn test_execute_type_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zoya");
        let output = dir.path().join("test.js");
        std::fs::write(&file, "pub fn main() -> Int { true }").unwrap();

        let result = execute(&file, Some(&output));
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_file_not_found() {
        let output = PathBuf::from("/tmp/out.js");
        let result = execute(Path::new("nonexistent.zoya"), Some(&output));
        assert!(result.is_err());
    }
}
