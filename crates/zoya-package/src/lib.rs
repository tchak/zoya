//! Package data structures for Zoya.
//!
//! This crate provides the core package-related types used across the Zoya compiler:
//! - `QualifiedPath`: Qualified path to a module, definition, or variant
//! - `Module`: A loaded module containing parsed items
//! - `Package`: The complete package of loaded modules
//! - `PackageConfig`: Configuration from `package.toml`

mod config;

pub use config::{ConfigError, PackageConfig};
pub use zoya_naming::RESERVED_NAMES;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use zoya_ast::{Item, Visibility};

/// A qualified path: `root` is `["root"]`, `root::utils::foo` is `["root", "utils", "foo"]`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedPath(Vec<String>);

impl QualifiedPath {
    pub fn new(segments: Vec<String>) -> Self {
        assert!(!segments.is_empty(), "QualifiedPath cannot be empty");
        QualifiedPath(segments)
    }

    pub fn root() -> Self {
        QualifiedPath(vec!["root".to_string()])
    }

    /// Create a single-segment path for local references
    pub fn local(name: String) -> Self {
        QualifiedPath(vec![name])
    }

    pub fn child(&self, name: &str) -> Self {
        let mut segments = self.0.clone();
        segments.push(name.to_string());
        QualifiedPath(segments)
    }

    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            None
        } else {
            let mut segments = self.0.clone();
            segments.pop();
            Some(QualifiedPath(segments))
        }
    }

    pub(crate) fn is_root(&self) -> bool {
        self.0.len() == 1 && self.0[0] == "root"
    }

    /// Get the depth of this path (number of segments)
    pub fn depth(&self) -> usize {
        self.0.len()
    }

    /// Get the number of segments
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the path is empty (always false — QualifiedPath is never empty)
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Get the segments of this path
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Get the first segment
    pub fn head(&self) -> &str {
        &self.0[0]
    }

    /// Get all segments after the first
    pub fn tail(&self) -> &[String] {
        &self.0[1..]
    }

    /// Get the last segment
    pub fn last(&self) -> &str {
        self.0.last().expect("QualifiedPath cannot be empty")
    }

    /// Iterate over segments
    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.0.iter()
    }

    /// Replace the root segment ("root") with a new name.
    /// e.g., root::option::Option → std::option::Option
    pub fn with_root(&self, name: &str) -> Self {
        let mut segments = self.0.clone();
        if segments.first().is_some_and(|s| s == "root") {
            segments[0] = name.to_string();
        }
        QualifiedPath(segments)
    }
}

impl std::fmt::Display for QualifiedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("::"))
    }
}

impl Serialize for QualifiedPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for QualifiedPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let segments: Vec<String> = s.split("::").map(String::from).collect();
        if segments.is_empty() {
            return Err(serde::de::Error::custom("QualifiedPath cannot be empty"));
        }
        Ok(QualifiedPath(segments))
    }
}

/// A loaded module
#[derive(Debug, Clone)]
pub struct Module {
    pub items: Vec<Item>,
    pub path: QualifiedPath,
    pub children: HashMap<String, (QualifiedPath, Visibility)>,
}

/// The complete package of loaded modules
#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub modules: HashMap<QualifiedPath, Module>,
}

impl Package {
    pub fn root(&self) -> Option<&Module> {
        self.modules.get(&QualifiedPath::root())
    }

    pub fn get(&self, path: &QualifiedPath) -> Option<&Module> {
        self.modules.get(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qualified_path_root() {
        let root = QualifiedPath::root();
        assert!(root.is_root());
        assert_eq!(root.segments(), &["root"]);
        assert_eq!(root.to_string(), "root");
        assert_eq!(root.head(), "root");
        assert_eq!(root.last(), "root");
        assert_eq!(root.len(), 1);
    }

    #[test]
    fn test_qualified_path_child() {
        let root = QualifiedPath::root();
        let utils = root.child("utils");
        assert_eq!(utils.segments(), &["root", "utils"]);
        assert_eq!(utils.to_string(), "root::utils");
        assert_eq!(utils.head(), "root");
        assert_eq!(utils.last(), "utils");
        assert_eq!(utils.tail(), &["utils"]);
    }

    #[test]
    fn test_qualified_path_local() {
        let local = QualifiedPath::local("x".to_string());
        assert_eq!(local.segments(), &["x"]);
        assert_eq!(local.head(), "x");
        assert_eq!(local.last(), "x");
        assert_eq!(local.len(), 1);
    }

    #[test]
    fn test_qualified_path_parent() {
        let root = QualifiedPath::root();
        assert!(root.parent().is_none());

        let utils = root.child("utils");
        assert_eq!(utils.parent(), Some(QualifiedPath::root()));

        let helpers = utils.child("helpers");
        assert_eq!(
            helpers.parent(),
            Some(QualifiedPath::new(vec![
                "root".to_string(),
                "utils".to_string()
            ]))
        );
    }

    #[test]
    fn test_qualified_path_deeply_nested() {
        let path = QualifiedPath::root().child("a").child("b").child("c");
        assert_eq!(path.segments(), &["root", "a", "b", "c"]);
        assert_eq!(path.to_string(), "root::a::b::c");
    }

    #[test]
    fn test_qualified_path_with_root() {
        let path = QualifiedPath::root().child("option").child("Option");
        let remapped = path.with_root("std");
        assert_eq!(remapped.segments(), &["std", "option", "Option"]);
        assert_eq!(remapped.to_string(), "std::option::Option");
    }

    #[test]
    fn test_qualified_path_with_root_no_change() {
        let path = QualifiedPath::local("x".to_string());
        let remapped = path.with_root("std");
        assert_eq!(remapped.segments(), &["x"]);
    }

    #[test]
    fn test_qualified_path_serde_roundtrip() {
        let path = QualifiedPath::root().child("utils").child("foo");
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, "\"root::utils::foo\"");
        let deserialized: QualifiedPath = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, path);
    }

    #[test]
    fn test_qualified_path_serde_as_map_key() {
        let mut map = HashMap::new();
        map.insert(QualifiedPath::root().child("a"), 1);
        map.insert(QualifiedPath::root().child("b"), 2);
        let json = serde_json::to_string(&map).unwrap();
        let deserialized: HashMap<QualifiedPath, i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, map);
    }
}
