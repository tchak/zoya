# zoya-naming

Naming conventions and validation for the Zoya programming language.

Provides a single source of truth for name validation, case conversion, and sanitization used across the compiler.

## Features

- **Case validation** - Check if names follow `snake_case` or `PascalCase` conventions
- **Case conversion** - Convert between `snake_case` and `PascalCase`
- **Name validation** - Validate package names, module names, identifiers, and type names
- **Reserved names** - Detect reserved keywords (`root`, `self`, `super`, `std`, `zoya`)
- **Sanitization** - Convert arbitrary strings into valid package names

## Usage

```rust
use zoya_naming::{
    is_snake_case, is_pascal_case, is_valid_module_name, is_valid_package_name,
    to_snake_case, to_pascal_case, sanitize_package_name, RESERVED_NAMES,
};

// Case validation
assert!(is_snake_case("my_function"));
assert!(is_pascal_case("MyType"));

// Name validation
assert!(is_valid_module_name("utils"));
assert!(!is_valid_module_name("MyModule"));  // must be snake_case
assert!(!is_valid_module_name("self"));      // reserved

assert!(is_valid_package_name("my-project"));
assert!(!is_valid_package_name("std"));      // reserved

// Case conversion (for error message suggestions)
assert_eq!(to_snake_case("MyFunction"), "my_function");
assert_eq!(to_pascal_case("my_type"), "MyType");

// Package name sanitization
assert_eq!(sanitize_package_name("My-Project"), "my-project");
assert_eq!(sanitize_package_name("123project"), "pkg_123project");
assert_eq!(sanitize_package_name("std"), "pkg_std");
```

## Public API

| Function | Description |
|----------|-------------|
| `is_snake_case(name)` | Check if name is `snake_case` |
| `is_pascal_case(name)` | Check if name is `PascalCase` |
| `is_valid_identifier(name)` | Valid variable/function name (snake_case) |
| `is_valid_type_name(name)` | Valid type name (PascalCase) |
| `is_valid_module_name(name)` | Valid module name (snake_case, not reserved) |
| `is_valid_package_name(name)` | Valid package name (lowercase, not reserved) |
| `is_reserved_name(name)` | Check if name is reserved |
| `to_snake_case(name)` | Convert to snake_case |
| `to_pascal_case(name)` | Convert to PascalCase |
| `sanitize_package_name(input)` | Sanitize arbitrary string to valid package name |
| `package_name_to_module_name(name)` | Convert package name to module name (hyphens to underscores) |
| `RESERVED_NAMES` | List of reserved names: `root`, `self`, `super`, `std`, `zoya` |

This crate has no dependencies - it contains only pure functions.
