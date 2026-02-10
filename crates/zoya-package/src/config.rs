//! Package configuration for Zoya projects.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Package configuration loaded from `package.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageConfig {
    /// Package name (lowercase alphanumeric with underscores or hyphens)
    pub name: String,
    /// Relative path to the main entry file (defaults to "src/main.zoya")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<PathBuf>,
    /// Output path for build artifacts (defaults to "build/{name}.js")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,
}

impl PackageConfig {
    /// Get the main entry file path, using default if not specified.
    pub fn main_path(&self) -> PathBuf {
        self.main
            .clone()
            .unwrap_or_else(|| PathBuf::from("src/main.zoya"))
    }

    /// Get the output file path, using default if not specified.
    pub fn output_path(&self) -> PathBuf {
        self.output
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("build/{}.js", self.name)))
    }

    /// Get the module name (hyphens replaced with underscores).
    pub fn module_name(&self) -> String {
        zoya_naming::package_name_to_module_name(&self.name)
    }

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
    /// Valid names are lowercase, alphanumeric with underscores or hyphens,
    /// must not start with a digit, underscore, or hyphen, and must not be
    /// a reserved name (`root`, `self`, `super`, `std`, `zoya`).
    pub fn is_valid_name(name: &str) -> bool {
        zoya_naming::is_valid_package_name(name)
    }

    /// Sanitize an input string into a valid package name.
    ///
    /// - Converts to lowercase
    /// - Preserves hyphens
    /// - Replaces other non-alphanumeric characters with underscores
    /// - Collapses all consecutive separators
    /// - Unifies separator style based on the first separator encountered
    /// - Prepends `pkg_` if starts with digit or underscore
    pub fn sanitize_name(input: &str) -> String {
        zoya_naming::sanitize_package_name(input)
    }
}

/// Errors that can occur when loading package configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// IO error reading the config file
    #[error("failed to read '{}': {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// TOML parsing error
    #[error("invalid TOML: {source}")]
    Parse { source: toml::de::Error },
    /// Invalid package name
    #[error(
        "invalid package name '{name}': must be lowercase alphanumeric with underscores or hyphens, starting with a letter, and must not be a reserved name (root, self, super, std, zoya)"
    )]
    InvalidName { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "std",
            "zoya",
            "root",
            "self",
            "super",
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
            main: Some(PathBuf::from("src/main.zoya")),
            output: Some(PathBuf::from("dist/out.js")),
        };

        let toml = config.to_toml();
        assert!(toml.contains("name = \"my_project\""));
        assert!(toml.contains("main = \"src/main.zoya\""));
        assert!(toml.contains("output = \"dist/out.js\""));
    }

    #[test]
    fn test_to_toml_omits_none_fields() {
        let config = PackageConfig {
            name: "my_project".to_string(),
            main: None,
            output: None,
        };

        let toml = config.to_toml();
        assert!(toml.contains("name = \"my_project\""));
        assert!(!toml.contains("main"));
        assert!(!toml.contains("output"));
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
        assert_eq!(config.main, Some(PathBuf::from("src/main.zoya")));
    }

    #[test]
    fn test_load_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("package.toml");

        std::fs::write(
            &config_path,
            r#"
name = "test_project"
"#,
        )
        .unwrap();

        let config = PackageConfig::load_from(&config_path).unwrap();
        assert_eq!(config.name, "test_project");
        assert_eq!(config.main, None);
        assert_eq!(config.output, None);
        assert_eq!(config.main_path(), PathBuf::from("src/main.zoya"));
        assert_eq!(config.output_path(), PathBuf::from("build/test_project.js"));
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
    fn test_load_hyphenated_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "my-project"
"#,
        )
        .unwrap();

        let config = PackageConfig::load(dir.path()).unwrap();
        assert_eq!(config.name, "my-project");
        assert_eq!(config.module_name(), "my_project");
        assert_eq!(config.output_path(), PathBuf::from("build/my-project.js"));
    }

    #[test]
    fn test_load_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.toml"),
            r#"
name = "Invalid Name"
"#,
        )
        .unwrap();

        let result = PackageConfig::load(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::InvalidName { .. }));
    }

    #[test]
    fn test_main_path() {
        let config = PackageConfig {
            name: "test".to_string(),
            main: None,
            output: None,
        };
        assert_eq!(config.main_path(), PathBuf::from("src/main.zoya"));

        let config = PackageConfig {
            name: "test".to_string(),
            main: Some(PathBuf::from("lib/app.zoya")),
            output: None,
        };
        assert_eq!(config.main_path(), PathBuf::from("lib/app.zoya"));
    }

    #[test]
    fn test_output_path() {
        let config = PackageConfig {
            name: "my-app".to_string(),
            main: None,
            output: None,
        };
        assert_eq!(config.output_path(), PathBuf::from("build/my-app.js"));

        let config = PackageConfig {
            name: "my-app".to_string(),
            main: None,
            output: Some(PathBuf::from("dist/bundle.js")),
        };
        assert_eq!(config.output_path(), PathBuf::from("dist/bundle.js"));
    }

    #[test]
    fn test_module_name() {
        let config = PackageConfig {
            name: "my-project".to_string(),
            main: None,
            output: None,
        };
        assert_eq!(config.module_name(), "my_project");

        let config = PackageConfig {
            name: "simple".to_string(),
            main: None,
            output: None,
        };
        assert_eq!(config.module_name(), "simple");

        let config = PackageConfig {
            name: "foo-bar-baz".to_string(),
            main: None,
            output: None,
        };
        assert_eq!(config.module_name(), "foo_bar_baz");
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
