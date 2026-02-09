use std::fmt::{Debug, Display};

use zoya_package::QualifiedPath;

// ============================================================================
// Source Error Types
// ============================================================================

/// Error kind for source operations
#[derive(Debug, Clone, PartialEq)]
pub enum SourceErrorKind {
    NotFound,
    PermissionDenied,
    IoError,
    Other,
}

/// Error from a module source operation
#[derive(Debug, Clone, PartialEq)]
pub struct SourceError {
    pub kind: SourceErrorKind,
    pub message: String,
}

impl SourceError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: SourceErrorKind::NotFound,
            message: message.into(),
        }
    }

    pub fn io_error(message: impl Into<String>) -> Self {
        Self {
            kind: SourceErrorKind::IoError,
            message: message.into(),
        }
    }
}

impl Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SourceError {}

// ============================================================================
// ModuleSource Trait
// ============================================================================

/// Trait for abstracting module source backends (filesystem, memory, etc.)
pub trait ModuleSource {
    /// The path type used by this source (e.g., PathBuf for filesystem, String for memory)
    type Path: Clone + Debug + Display;

    /// Read the source code at the given path
    fn read(&self, path: &Self::Path) -> Result<String, SourceError>;

    /// Check if a module exists at the given path
    fn exists(&self, path: &Self::Path) -> bool;

    /// Resolve the path for a submodule given the current module's logical path and the submodule name
    fn resolve_submodule(&self, module_path: &QualifiedPath, mod_name: &str) -> Self::Path;
}
