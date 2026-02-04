use std::collections::HashMap;

use zoya_package::ModulePath;

use crate::source::{ModuleSource, SourceError};

// ============================================================================
// Memory Source
// ============================================================================

/// In-memory module source for testing
pub struct MemorySource {
    modules: HashMap<String, String>,
}

impl MemorySource {
    /// Create a new empty MemorySource
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Add a module with the given path and source
    pub fn with_module(mut self, path: impl Into<String>, source: impl Into<String>) -> Self {
        self.modules.insert(path.into(), source.into());
        self
    }

    /// Add a module with the given path and source (mutable version)
    pub fn add_module(&mut self, path: impl Into<String>, source: impl Into<String>) {
        self.modules.insert(path.into(), source.into());
    }
}

impl Default for MemorySource {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleSource for MemorySource {
    type Path = String;

    fn read(&self, path: &Self::Path) -> Result<String, SourceError> {
        self.modules
            .get(path)
            .cloned()
            .ok_or_else(|| SourceError::not_found(format!("module not found: {}", path)))
    }

    fn exists(&self, path: &Self::Path) -> bool {
        self.modules.contains_key(path)
    }

    fn resolve_submodule(&self, module_path: &ModulePath, mod_name: &str) -> Self::Path {
        // Build path from module path segments (skipping "root")
        let segments: Vec<&str> = module_path.0[1..].iter().map(|s| s.as_str()).collect();
        if segments.is_empty() {
            mod_name.to_string()
        } else {
            format!("{}/{}", segments.join("/"), mod_name)
        }
    }
}
