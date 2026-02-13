use std::path::Path;

use console::{Term, style};
use zoya_check::check;
use zoya_codegen::codegen;
use zoya_loader::Mode;

/// Compile a file to JavaScript without executing
pub fn execute(path: &Path, output: Option<&Path>, mode: Mode) -> Result<(), String> {
    let term = Term::stderr();

    // Load and parse package
    let pkg = zoya_loader::load_package(path, mode).map_err(|e| e.to_string())?;

    // Type check entire package with std
    let std = zoya_std::std();
    let checked_pkg = check(&pkg, &[std]).map_err(|e| e.to_string())?;

    // Resolve output path: CLI arg > package.toml output > error
    let out_path = output
        .map(|p| p.to_path_buf())
        .or(checked_pkg.output.clone())
        .ok_or_else(|| {
            "no output path specified\nhint: use --output <path> or set output in package.toml"
                .to_string()
        })?;

    // Generate single concatenated JS
    let js_output = codegen(&checked_pkg, &[std]);

    // Create output directory if needed
    if !out_path.exists() {
        std::fs::create_dir_all(&out_path)
            .map_err(|e| format!("failed to create directory '{}': {}", out_path.display(), e))?;
    }

    // Write single JS file
    let filename = format!("{}.js", checked_pkg.name);
    let file_path = out_path.join(&filename);

    std::fs::write(&file_path, &js_output.code)
        .map_err(|e| format!("failed to write file '{}': {}", file_path.display(), e))?;
    let _ = term.write_line(&format!("  {}", style(&filename).dim()));

    let _ = term.write_line(&format!(
        "{} Built: {}",
        style("✓").green(),
        style(out_path.display()).bold()
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_execute_to_directory() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.zy");
        let output = dir.path().join("build");
        std::fs::write(&input, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&input, Some(&output), Mode::Dev);
        assert!(result.is_ok());
        assert!(output.is_dir());

        // Should contain exactly 1 file (single concatenated JS)
        let files: Vec<_> = std::fs::read_dir(&output)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1, "expected 1 file, got {}", files.len());

        // Check that the file is named {pkg_name}.js and contains expected content
        let js_file = &files[0];
        let name = js_file.file_name().to_str().unwrap().to_string();
        assert_eq!(name, "test.js");
        let js = std::fs::read_to_string(js_file.path()).unwrap();
        assert!(js.contains("function $test$main()"));
    }

    #[test]
    fn test_execute_no_output_errors() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&file, None, Mode::Dev);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no output path"));
    }

    #[test]
    fn test_execute_uses_package_output() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml with output (now a directory)
        std::fs::write(
            dir.path().join("package.toml"),
            "[package]\nname = \"test-project\"\noutput = \"build\"\n",
        )
        .unwrap();

        // Create main file at default location
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.zy"),
            "pub fn main() -> Int { 42 }",
        )
        .unwrap();

        let result = execute(dir.path(), None, Mode::Dev);
        assert!(result.is_ok());

        let output_path = dir.path().join("build");
        assert!(output_path.is_dir());
    }

    #[test]
    fn test_execute_cli_output_overrides_package() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml with output
        std::fs::write(
            dir.path().join("package.toml"),
            "[package]\nname = \"test-project\"\noutput = \"build\"\n",
        )
        .unwrap();

        // Create main file at default location
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.zy"),
            "pub fn main() -> Int { 42 }",
        )
        .unwrap();

        let cli_output = dir.path().join("custom");
        let result = execute(dir.path(), Some(&cli_output), Mode::Dev);
        assert!(result.is_ok());

        // CLI output should be used, not package.toml output
        assert!(cli_output.is_dir());
        let pkg_output = dir.path().join("build");
        assert!(!pkg_output.exists());
    }

    #[test]
    fn test_execute_creates_output_dir() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.zy");
        let output = dir.path().join("deep/nested/out");
        std::fs::write(&input, "pub fn main() -> Int { 42 }").unwrap();

        let result = execute(&input, Some(&output), Mode::Dev);
        assert!(result.is_ok());
        assert!(output.is_dir());
    }

    #[test]
    fn test_execute_type_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        let output = dir.path().join("build");
        std::fs::write(&file, "pub fn main() -> Int { true }").unwrap();

        let result = execute(&file, Some(&output), Mode::Dev);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_file_not_found() {
        let output = PathBuf::from("/tmp/build");
        let result = execute(Path::new("nonexistent.zy"), Some(&output), Mode::Dev);
        assert!(result.is_err());
    }
}
