use std::path::Path;

use anyhow::{Context, Result};
use console::{Term, style};
use zoya_check::check;
use zoya_codegen::codegen;
use zoya_loader::Mode;

/// Compile a file to JavaScript without executing
pub fn execute(path: &Path, output: Option<&Path>, mode: Mode) -> Result<()> {
    let term = Term::stderr();

    // Load and parse package
    let pkg = zoya_loader::load_package(path, mode)?;

    // Type check entire package with std
    let std = zoya_std::std();
    let checked_pkg = check(&pkg, &[std])?;

    // Resolve output path: CLI arg > default "build" relative to package dir
    let base_dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };
    let out_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| base_dir.join("build"));

    // Generate single concatenated JS
    let js_output = codegen(&checked_pkg, &[std]);

    // Create output directory if needed
    if !out_path.exists() {
        std::fs::create_dir_all(&out_path)
            .with_context(|| format!("failed to create directory '{}'", out_path.display()))?;
    }

    // Write single JS file
    let filename = format!("{}.js", checked_pkg.name);
    let file_path = out_path.join(&filename);

    std::fs::write(&file_path, &js_output.code)
        .with_context(|| format!("failed to write file '{}'", file_path.display()))?;
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
    fn test_execute_defaults_to_build_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        std::fs::write(&file, "pub fn main() -> Int { 42 }").unwrap();

        // Without --output, defaults to "build" (relative to CWD).
        // We test this via the package test below; here just verify no error.
        let build_dir = dir.path().join("build");
        let result = execute(&file, Some(&build_dir), Mode::Dev);
        assert!(result.is_ok());
        assert!(build_dir.is_dir());
    }

    #[test]
    fn test_execute_uses_package_default_build() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml without output field
        std::fs::write(
            dir.path().join("package.toml"),
            "[package]\nname = \"test-project\"\n",
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
        assert!(dir.path().join("build").is_dir());
    }

    #[test]
    fn test_execute_cli_output_overrides_default() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml without output field
        std::fs::write(
            dir.path().join("package.toml"),
            "[package]\nname = \"test-project\"\n",
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

        // CLI output should be used, not default "build"
        assert!(cli_output.is_dir());
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
