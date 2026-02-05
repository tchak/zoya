//! Package configuration for Zoya projects.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Package configuration loaded from `package.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageConfig {
    /// Package name (must be valid identifier: lowercase alphanumeric and underscores)
    pub name: String,
    /// Relative path to the main entry file
    pub main: PathBuf,
}

impl PackageConfig {
    /// Load package config from a directory's `package.toml`.
    pub fn load(dir: &Path) -> Result<Self, ConfigError> {
        Self::load_from(&dir.join("package.toml"))
    }

    /// Load package config from a specific file path.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let config: PackageConfig =
            toml::from_str(&content).map_err(|e| ConfigError::Parse { source: e })?;

        if !Self::is_valid_name(&config.name) {
            return Err(ConfigError::InvalidName {
                name: config.name.clone(),
            });
        }

        Ok(config)
    }

    /// Serialize the config to a TOML string.
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("PackageConfig should always serialize")
    }

    /// Check if a name is a valid package name.
    ///
    /// Valid names are lowercase, alphanumeric with underscores,
    /// and must not start with a digit or underscore.
    pub fn is_valid_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        // Must start with a lowercase letter
        if !first.is_ascii_lowercase() {
            return false;
        }

        // Rest must be lowercase alphanumeric or underscore
        chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// Sanitize an input string into a valid package name.
    ///
    /// - Converts to lowercase
    /// - Replaces non-alphanumeric characters with underscores
    /// - Collapses multiple underscores
    /// - Prepends `pkg_` if starts with digit or underscore
    pub fn sanitize_name(input: &str) -> String {
        if input.is_empty() {
            return "pkg".to_string();
        }

        // Convert to lowercase and replace invalid chars with underscores
        let mut result: String = input
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();

        // Collapse multiple underscores
        while result.contains("__") {
            result = result.replace("__", "_");
        }

        // Trim leading/trailing underscores
        result = result.trim_matches('_').to_string();

        if result.is_empty() {
            return "pkg".to_string();
        }

        // Prepend pkg_ if starts with digit
        if result.chars().next().unwrap().is_ascii_digit() {
            result = format!("pkg_{}", result);
        }

        result
    }
}

/// Errors that can occur when loading package configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// IO error reading the config file
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// TOML parsing error
    Parse { source: toml::de::Error },
    /// Invalid package name
    InvalidName { name: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "failed to read '{}': {}", path.display(), source)
            }
            ConfigError::Parse { source } => write!(f, "invalid TOML: {}", source),
            ConfigError::InvalidName { name } => write!(
                f,
                "invalid package name '{}': must be lowercase alphanumeric with underscores, starting with a letter",
                name
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Parse { source } => Some(source),
            ConfigError::InvalidName { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_name() {
        // Valid names
        assert!(PackageConfig::is_valid_name("myproject"));
        assert!(PackageConfig::is_valid_name("my_project"));
        assert!(PackageConfig::is_valid_name("project123"));
        assert!(PackageConfig::is_valid_name("a"));

        // Invalid names
        assert!(!PackageConfig::is_valid_name("")); // empty
        assert!(!PackageConfig::is_valid_name("123project")); // starts with digit
        assert!(!PackageConfig::is_valid_name("_project")); // starts with underscore
        assert!(!PackageConfig::is_valid_name("MyProject")); // uppercase
        assert!(!PackageConfig::is_valid_name("my-project")); // hyphen
        assert!(!PackageConfig::is_valid_name("my project")); // space
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(PackageConfig::sanitize_name("my-project"), "my_project");
        assert_eq!(PackageConfig::sanitize_name("MyProject"), "myproject");
        assert_eq!(PackageConfig::sanitize_name("123project"), "pkg_123project");
        assert_eq!(PackageConfig::sanitize_name("my--project"), "my_project");
        assert_eq!(PackageConfig::sanitize_name("  spaces  "), "spaces");
        assert_eq!(PackageConfig::sanitize_name(""), "pkg");
        assert_eq!(PackageConfig::sanitize_name("---"), "pkg");
        assert_eq!(PackageConfig::sanitize_name("_leading"), "leading");
        assert_eq!(PackageConfig::sanitize_name("trailing_"), "trailing");
    }

    #[test]
    fn test_sanitize_name_produces_valid_names() {
        let inputs = [
            "my-project",
            "MyProject",
            "123project",
            "a",
            "---",
            "",
            "UPPERCASE",
            "with spaces",
            "_underscore_",
        ];

        for input in inputs {
            let sanitized = PackageConfig::sanitize_name(input);
            assert!(
                PackageConfig::is_valid_name(&sanitized),
                "sanitize_name({:?}) = {:?} should be valid",
                input,
                sanitized
            );
        }
    }

    #[test]
    fn test_to_toml() {
        let config = PackageConfig {
            name: "my_project".to_string(),
            main: PathBuf::from("src/main.zoya"),
        };

        let toml = config.to_toml();
        assert!(toml.contains("name = \"my_project\""));
        assert!(toml.contains("main = \"src/main.zoya\""));
    }

    #[test]
    fn test_load_from() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("package.toml");

        std::fs::write(
            &config_path,
            r#"
name = "test_project"
main = "src/main.zoya"
"#,
        )
        .unwrap();

        let config = PackageConfig::load_from(&config_path).unwrap();
        assert_eq!(config.name, "test_project");
        assert_eq!(config.main, PathBuf::from("src/main.zoya"));
    }

    #[test]
    fn test_load() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "loaded_project"
main = "src/main.zoya"
"#,
        )
        .unwrap();

        let config = PackageConfig::load(dir.path()).unwrap();
        assert_eq!(config.name, "loaded_project");
    }

    #[test]
    fn test_load_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "Invalid-Name"
main = "src/main.zoya"
"#,
        )
        .unwrap();

        let result = PackageConfig::load(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::InvalidName { .. }));
    }

    #[test]
    fn test_load_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = PackageConfig::load(dir.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Io { .. }));
    }

    #[test]
    fn test_load_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.toml"), "not valid toml {{{{").unwrap();

        let result = PackageConfig::load(dir.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Parse { .. }));
    }
}
