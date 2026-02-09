use std::path::{Path, PathBuf};

use zoya_package::{ConfigError, PackageConfig};

/// Errors that can occur when resolving an entry point
#[derive(Debug)]
pub enum ResolveError {
    /// No package.toml found in the directory
    NoPackageToml { dir: PathBuf },
    /// Error loading package config
    Config(ConfigError),
    /// Main file specified in package.toml doesn't exist
    MainNotFound { main: PathBuf, package_dir: PathBuf },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NoPackageToml { dir } => {
                if dir == Path::new(".") {
                    write!(
                        f,
                        "no package.toml found in current directory\nhint: provide a file path or create a package with `zoya new`"
                    )
                } else {
                    write!(
                        f,
                        "no package.toml found in '{}'\nhint: provide a .zoya file path or create a package with `zoya new {}`",
                        dir.display(),
                        dir.display()
                    )
                }
            }
            ResolveError::Config(e) => write!(f, "{}", e),
            ResolveError::MainNotFound { main, package_dir } => {
                write!(
                    f,
                    "main file '{}' not found in package at '{}'",
                    main.display(),
                    package_dir.display()
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResolveError::Config(e) => Some(e),
            _ => None,
        }
    }
}

/// Resolve an entry point from an optional path.
///
/// - `None` → Look for `package.toml` in current directory
/// - `Some(dir)` where dir is a directory → Look for `package.toml` in that directory
/// - `Some(file)` where file is a .zoya file → Return it directly
pub fn resolve_entry_point(path: Option<&Path>) -> Result<PathBuf, ResolveError> {
    match path {
        None => resolve_from_directory(Path::new(".")),
        Some(p) if p.is_dir() => resolve_from_directory(p),
        Some(p) => Ok(p.to_path_buf()),
    }
}

fn resolve_from_directory(dir: &Path) -> Result<PathBuf, ResolveError> {
    let config_path = dir.join("package.toml");
    if !config_path.exists() {
        return Err(ResolveError::NoPackageToml {
            dir: dir.to_path_buf(),
        });
    }

    let config = PackageConfig::load(dir).map_err(ResolveError::Config)?;
    let main = config.main_path();
    let main_path = dir.join(&main);

    if !main_path.exists() {
        return Err(ResolveError::MainNotFound {
            main,
            package_dir: dir.to_path_buf(),
        });
    }

    Ok(main_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_direct_file() {
        // When given a path to a file (even nonexistent), return it directly
        let path = Path::new("some/file.zoya");
        let result = resolve_entry_point(Some(path));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("some/file.zoya"));
    }

    #[test]
    fn test_resolve_directory_with_package() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "test_project"
main = "src/main.zoya"
"#,
        )
        .unwrap();

        // Create main file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.zoya"), "fn main() { 1 }").unwrap();

        let result = resolve_entry_point(Some(dir.path()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dir.path().join("src/main.zoya"));
    }

    #[test]
    fn test_resolve_directory_with_default_main() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml without explicit main field
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "test_project"
"#,
        )
        .unwrap();

        // Create main file at default location
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.zoya"), "fn main() { 1 }").unwrap();

        let result = resolve_entry_point(Some(dir.path()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dir.path().join("src/main.zoya"));
    }

    #[test]
    fn test_resolve_directory_without_package() {
        let dir = tempfile::tempdir().unwrap();

        let result = resolve_entry_point(Some(dir.path()));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolveError::NoPackageToml { .. }
        ));
    }

    #[test]
    fn test_resolve_package_with_missing_main() {
        let dir = tempfile::tempdir().unwrap();

        // Create package.toml but no main file
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "test_project"
main = "src/main.zoya"
"#,
        )
        .unwrap();

        let result = resolve_entry_point(Some(dir.path()));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolveError::MainNotFound { .. }
        ));
    }

    #[test]
    fn test_resolve_none_without_package() {
        // This test would look in current directory, which may or may not have package.toml
        // We test the behavior by calling resolve_from_directory directly with a temp dir
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_from_directory(dir.path());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolveError::NoPackageToml { .. }
        ));
    }

    #[test]
    fn test_error_display_current_directory() {
        let err = ResolveError::NoPackageToml {
            dir: PathBuf::from("."),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("current directory"));
        assert!(msg.contains("zoya new"));
    }

    #[test]
    fn test_error_display_specific_directory() {
        let err = ResolveError::NoPackageToml {
            dir: PathBuf::from("my/project"),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("my/project"));
        assert!(msg.contains("zoya new my/project"));
    }

    #[test]
    fn test_error_display_main_not_found() {
        let err = ResolveError::MainNotFound {
            main: PathBuf::from("src/main.zoya"),
            package_dir: PathBuf::from("/path/to/project"),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("src/main.zoya"));
        assert!(msg.contains("/path/to/project"));
    }
}
