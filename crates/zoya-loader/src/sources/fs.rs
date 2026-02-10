use std::fmt::Display;
use std::path::{Path, PathBuf};

use zoya_package::QualifiedPath;

use crate::source::{ModuleSource, SourceError, SourceErrorKind};

// ============================================================================
// Path Wrapper (PathBuf with Display)
// ============================================================================

/// A wrapper around PathBuf that implements Display
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilePath(pub PathBuf);

impl FilePath {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().to_path_buf())
    }

    pub fn exists(&self) -> bool {
        self.0.exists()
    }
}

impl Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<PathBuf> for FilePath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

impl From<&Path> for FilePath {
    fn from(path: &Path) -> Self {
        Self(path.to_path_buf())
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

// ============================================================================
// Filesystem Source
// ============================================================================

/// Filesystem-based module source
pub struct FsSource {
    base_dir: PathBuf,
}

impl FsSource {
    /// Create a new FsSource with the given base directory
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a new FsSource from a file path, using its parent directory as base
    pub fn from_file(file_path: &Path) -> Self {
        let base_dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Self { base_dir }
    }

}

impl ModuleSource for FsSource {
    type Path = FilePath;

    fn read(&self, path: &Self::Path) -> Result<String, SourceError> {
        std::fs::read_to_string(&path.0).map_err(|e| {
            let kind = match e.kind() {
                std::io::ErrorKind::NotFound => SourceErrorKind::NotFound,
                std::io::ErrorKind::PermissionDenied => SourceErrorKind::PermissionDenied,
                _ => SourceErrorKind::IoError,
            };
            SourceError {
                kind,
                message: e.to_string(),
            }
        })
    }

    fn exists(&self, path: &Self::Path) -> bool {
        path.exists()
    }

    fn resolve_submodule(&self, module_path: &QualifiedPath, mod_name: &str) -> Self::Path {
        // Build path from base_dir using module path segments (skipping "root")
        let mut path = self.base_dir.clone();
        for segment in module_path.tail() {
            // Skip "root"
            path.push(segment);
        }
        path.push(format!("{}.zoya", mod_name));
        FilePath::new(path)
    }
}
