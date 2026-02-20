use std::path::Path;

use console::{Term, style};
use zoya_package::PackageConfig;

/// Create a new Zoya project
pub fn execute(path: &Path, name_override: Option<&str>) -> Result<(), NewError> {
    // Check if path already exists
    if path.exists() {
        return Err(NewError::AlreadyExists(path.to_path_buf()));
    }

    // Determine package name
    let name = match name_override {
        Some(n) => {
            if !PackageConfig::is_valid_name(n) {
                return Err(NewError::InvalidName(n.to_string()));
            }
            n.to_string()
        }
        None => {
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| NewError::InvalidPath(path.to_path_buf()))?;
            PackageConfig::sanitize_name(dir_name)
        }
    };

    // Create project directory
    std::fs::create_dir_all(path).map_err(|e| NewError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Create src directory
    let src_dir = path.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| NewError::Io {
        path: src_dir.clone(),
        source: e,
    })?;

    // Create package.toml
    let config = PackageConfig {
        name: name.clone(),
        main: None,
    };
    let config_path = path.join("package.toml");
    std::fs::write(&config_path, config.to_toml()).map_err(|e| NewError::Io {
        path: config_path,
        source: e,
    })?;

    // Create .gitignore
    let gitignore_path = path.join(".gitignore");
    std::fs::write(&gitignore_path, "build/\n").map_err(|e| NewError::Io {
        path: gitignore_path,
        source: e,
    })?;

    // Create src/main.zy
    let main_path = src_dir.join("main.zy");
    std::fs::write(&main_path, "pub fn main() { \"hello world\" }\n").map_err(|e| {
        NewError::Io {
            path: main_path,
            source: e,
        }
    })?;

    let term = Term::stderr();
    let _ = term.write_line(&format!(
        "{} Created project '{}' at {}",
        style("✓").green(),
        style(&name).bold(),
        path.display()
    ));

    Ok(())
}

/// Errors that can occur when creating a new project
#[derive(Debug)]
pub enum NewError {
    /// Path already exists
    AlreadyExists(std::path::PathBuf),
    /// Invalid project path
    InvalidPath(std::path::PathBuf),
    /// Invalid package name provided
    InvalidName(String),
    /// IO error
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for NewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewError::AlreadyExists(path) => {
                write!(f, "path '{}' already exists", path.display())
            }
            NewError::InvalidPath(path) => {
                write!(f, "invalid project path '{}'", path.display())
            }
            NewError::InvalidName(name) => write!(
                f,
                "invalid package name '{}': must be lowercase alphanumeric with underscores or hyphens, starting with a letter, and must not be a reserved name (root, self, super, std, zoya)",
                name
            ),
            NewError::Io { path, source } => {
                write!(f, "failed to create '{}': {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for NewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NewError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_creates_project() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("my_project");

        let result = execute(&project_path, None);
        assert!(result.is_ok());

        // Check files were created
        assert!(project_path.join("package.toml").exists());
        assert!(project_path.join("src").exists());
        assert!(project_path.join("src/main.zy").exists());
        assert!(project_path.join(".gitignore").exists());

        // Check package.toml content — main should be omitted (uses default)
        let config = PackageConfig::load(&project_path).unwrap();
        assert_eq!(config.name, "my_project");
        assert_eq!(config.main, None);
        assert_eq!(config.main_path(), std::path::PathBuf::from("src/main.zy"));

        // Check .gitignore content
        let gitignore = std::fs::read_to_string(project_path.join(".gitignore")).unwrap();
        assert!(gitignore.contains("build/"));

        // Check main.zy content
        let main_content = std::fs::read_to_string(project_path.join("src/main.zy")).unwrap();
        assert!(main_content.contains("pub fn main()"));
    }

    #[test]
    fn test_execute_with_name_override() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("some-directory");

        let result = execute(&project_path, Some("custom_name"));
        assert!(result.is_ok());

        let config = PackageConfig::load(&project_path).unwrap();
        assert_eq!(config.name, "custom_name");
    }

    #[test]
    fn test_execute_sanitizes_directory_name() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("My-Project");

        let result = execute(&project_path, None);
        assert!(result.is_ok());

        let config = PackageConfig::load(&project_path).unwrap();
        // Hyphens are preserved in sanitized names
        assert_eq!(config.name, "my-project");
    }

    #[test]
    fn test_execute_with_hyphenated_name() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("test");

        let result = execute(&project_path, Some("my-cool-app"));
        assert!(result.is_ok());

        let config = PackageConfig::load(&project_path).unwrap();
        assert_eq!(config.name, "my-cool-app");
        assert_eq!(config.module_name(), "my_cool_app");
    }

    #[test]
    fn test_execute_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("existing");
        std::fs::create_dir(&project_path).unwrap();

        let result = execute(&project_path, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NewError::AlreadyExists(_)));
    }

    #[test]
    fn test_execute_invalid_name_override() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("test");

        let result = execute(&project_path, Some("Invalid Name"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NewError::InvalidName(_)));
    }

    #[test]
    fn test_execute_reserved_name_rejected() {
        let dir = tempfile::tempdir().unwrap();
        for name in &["std", "zoya", "root", "self", "super"] {
            let project_path = dir.path().join(format!("test_{}", name));
            let result = execute(&project_path, Some(name));
            assert!(
                matches!(&result, Err(NewError::InvalidName(_))),
                "reserved name '{}' should be rejected, got {:?}",
                name,
                result
            );
        }
    }

    #[test]
    fn test_execute_sanitizes_reserved_directory_name() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("std");

        let result = execute(&project_path, None);
        assert!(result.is_ok());

        let config = PackageConfig::load(&project_path).unwrap();
        assert_eq!(config.name, "pkg_std");
    }
}
