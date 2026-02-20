use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use console::{Term, style};

use crate::diagnostic;

/// Format .zy source files
pub fn execute(path: &Path, check: bool) -> Result<()> {
    let term = Term::stderr();

    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        let files = collect_zy_files(path);
        if files.is_empty() {
            bail!("no .zy files found in '{}'", path.display());
        }
        files
    } else {
        bail!("path not found: '{}'", path.display());
    };

    let mut formatted_count = 0;
    let mut error_count = 0;
    let mut unformatted: Vec<PathBuf> = Vec::new();

    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                let _ = term.write_line(&format!(
                    "{}: failed to read '{}': {}",
                    style("error").red().bold(),
                    file.display(),
                    e
                ));
                error_count += 1;
                continue;
            }
        };

        let tokens = match zoya_lexer::lex(&source) {
            Ok(t) => t,
            Err(e) => {
                diagnostic::render_lex_error(&file.display().to_string(), &source, &e);
                error_count += 1;
                continue;
            }
        };

        let items = match zoya_parser::parse_module(tokens) {
            Ok(parsed) => parsed,
            Err(e) => {
                diagnostic::render_parse_error(&file.display().to_string(), &source, &e);
                error_count += 1;
                continue;
            }
        };

        let formatted = zoya_fmt::fmt(items);

        if formatted != source {
            if check {
                unformatted.push(file.clone());
            } else {
                if let Err(e) = fs::write(file, &formatted) {
                    let _ = term.write_line(&format!(
                        "{}: failed to write '{}': {}",
                        style("error").red().bold(),
                        file.display(),
                        e
                    ));
                    error_count += 1;
                    continue;
                }
                formatted_count += 1;
            }
        }
    }

    if check {
        if unformatted.is_empty() {
            let _ = term.write_line(&format!(
                "{} All {} file(s) formatted",
                style("✓").green(),
                files.len() - error_count
            ));
            Ok(())
        } else {
            let mut msg = String::from("the following files are not formatted:\n");
            for f in &unformatted {
                msg.push_str(&format!("  {}\n", f.display()));
            }
            bail!("{}", msg);
        }
    } else {
        let skipped = files.len() - formatted_count - error_count;
        if formatted_count > 0 {
            let _ = term.write_line(&format!(
                "{} Formatted {} file(s)",
                style("✓").green(),
                formatted_count
            ));
        }
        if skipped > 0 {
            let _ = term.write_line(&format!(
                "  {}",
                style(format!("{} file(s) already formatted", skipped)).dim()
            ));
        }
        if error_count > 0 {
            let _ = term.write_line(&format!(
                "  {}",
                style(format!("{} file(s) had errors", error_count)).red()
            ));
        }
        Ok(())
    }
}

fn collect_zy_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_zy_files_recursive(dir, &mut files);
    files.sort();
    files
}

fn collect_zy_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_zy_files_recursive(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "zy") {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        // Unformatted: private before pub
        fs::write(&file, "fn bar() -> Int 1\npub fn foo() -> Int 2\n").unwrap();

        let result = execute(&file, false);
        assert!(result.is_ok());

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "fn bar() -> Int 1\n\npub fn foo() -> Int 2\n");
    }

    #[test]
    fn test_format_already_formatted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        let formatted = "pub fn main() -> Int 42\n";
        fs::write(&file, formatted).unwrap();

        let result = execute(&file, false);
        assert!(result.is_ok());

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, formatted);
    }

    #[test]
    fn test_check_mode_passes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        let formatted = "pub fn main() -> Int 42\n";
        fs::write(&file, formatted).unwrap();

        let result = execute(&file, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_mode_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.zy");
        fs::write(&file, "fn bar() -> Int 1\npub fn foo() -> Int 2\n").unwrap();

        let result = execute(&file, true);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not formatted"));
        assert!(err.contains("test.zy"));

        // File should not have been modified
        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "fn bar() -> Int 1\npub fn foo() -> Int 2\n");
    }

    #[test]
    fn test_format_directory() {
        let dir = tempfile::tempdir().unwrap();

        let file1 = dir.path().join("a.zy");
        fs::write(&file1, "fn bar() -> Int 1\npub fn foo() -> Int 2\n").unwrap();

        let file2 = dir.path().join("b.zy");
        fs::write(&file2, "pub fn main() -> Int 42\n").unwrap();

        let subdir = dir.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        let file3 = subdir.join("c.zy");
        fs::write(&file3, "fn baz() -> Int 3\npub fn qux() -> Int 4\n").unwrap();

        let result = execute(dir.path(), false);
        assert!(result.is_ok());

        // file1 should be reformatted
        let content1 = fs::read_to_string(&file1).unwrap();
        assert_eq!(content1, "fn bar() -> Int 1\n\npub fn foo() -> Int 2\n");

        // file2 was already formatted
        let content2 = fs::read_to_string(&file2).unwrap();
        assert_eq!(content2, "pub fn main() -> Int 42\n");

        // file3 in subdir should be reformatted
        let content3 = fs::read_to_string(&file3).unwrap();
        assert_eq!(content3, "fn baz() -> Int 3\n\npub fn qux() -> Int 4\n");
    }

    #[test]
    fn test_file_not_found() {
        let result = execute(Path::new("nonexistent.zy"), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path not found"));
    }

    #[test]
    fn test_parse_error_continues() {
        let dir = tempfile::tempdir().unwrap();
        let bad_file = dir.path().join("bad.zy");
        fs::write(&bad_file, "this is not valid zoya syntax !!!").unwrap();

        let good_file = dir.path().join("good.zy");
        fs::write(&good_file, "pub fn main() -> Int 42\n").unwrap();

        let result = execute(dir.path(), false);
        // Should succeed overall (errors are reported but don't abort)
        assert!(result.is_ok());

        // Good file should still be fine
        let content = fs::read_to_string(&good_file).unwrap();
        assert_eq!(content, "pub fn main() -> Int 42\n");
    }

    #[test]
    fn test_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let result = execute(dir.path(), false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("no .zy files found")
        );
    }
}
