//! Naming conventions and validation for the Zoya language.
//!
//! Provides a single source of truth for name validation, case conversion,
//! and sanitization used across the compiler.

/// Names reserved for path prefixes, the standard library, or the language itself.
/// These cannot be used as package or module names.
pub const RESERVED_NAMES: &[&str] = &["root", "self", "super", "std", "zoya"];

/// Check if a name is reserved.
pub fn is_reserved_name(name: &str) -> bool {
    RESERVED_NAMES.contains(&name)
}

/// Check if name is snake_case: starts with a lowercase letter,
/// followed by lowercase letters, digits, or underscores.
pub fn is_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
        _ => false,
    }
}

/// Check if name is PascalCase: starts with an uppercase letter,
/// followed by alphanumeric characters (no underscores).
pub fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => name.chars().all(|c| c.is_ascii_alphanumeric()),
        _ => false,
    }
}

/// Check if a name is a valid package name.
///
/// Valid names are lowercase alphanumeric with underscores or hyphens,
/// must start with a lowercase letter, and must not be a reserved name.
pub fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    if is_reserved_name(name) {
        return false;
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap();

    // Must start with a lowercase letter
    if !first.is_ascii_lowercase() {
        return false;
    }

    // Rest must be lowercase alphanumeric, underscore, or hyphen
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Check if a name is a valid module name.
///
/// Valid module names are snake_case and must not be a reserved name.
pub fn is_valid_module_name(name: &str) -> bool {
    if is_reserved_name(name) {
        return false;
    }
    is_snake_case(name)
}

/// Check if a name is a valid identifier (alias for `is_snake_case`).
pub fn is_valid_identifier(name: &str) -> bool {
    is_snake_case(name)
}

/// Check if a name is a valid type name (alias for `is_pascal_case`).
pub fn is_valid_type_name(name: &str) -> bool {
    is_pascal_case(name)
}

/// Check if a name is a valid field name (alias for `is_snake_case`).
pub fn is_valid_field_name(name: &str) -> bool {
    is_snake_case(name)
}

/// Convert a name to snake_case for error message suggestions.
pub fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert a name to PascalCase for error message suggestions.
pub fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in name.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

/// Sanitize an input string into a valid package name.
///
/// - Converts to lowercase
/// - Preserves hyphens
/// - Replaces other non-alphanumeric characters with underscores
/// - Collapses all consecutive separators (`__`, `--`, `_-`, `-_`)
/// - Unifies separator style based on the first separator encountered
/// - Prepends `pkg_` if starts with digit
/// - Prepends `pkg_` if result is a reserved name
pub fn sanitize_package_name(input: &str) -> String {
    if input.is_empty() {
        return "pkg".to_string();
    }

    // Convert to lowercase, preserve hyphens, replace other invalid chars with underscores
    let mut result: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == '-' {
                '-'
            } else {
                '_'
            }
        })
        .collect();

    // Trim leading/trailing underscores and hyphens
    result = result.trim_matches(|c| c == '_' || c == '-').to_string();

    if result.is_empty() {
        return "pkg".to_string();
    }

    // Find the first separator to determine the canonical separator style
    let canonical_sep = result
        .chars()
        .find(|&c| c == '_' || c == '-')
        .unwrap_or('_');

    // Unify all separators to the canonical style and collapse consecutive separators
    let mut unified = String::new();
    let mut prev_was_sep = false;
    for c in result.chars() {
        if c == '_' || c == '-' {
            if !prev_was_sep {
                unified.push(canonical_sep);
            }
            prev_was_sep = true;
        } else {
            unified.push(c);
            prev_was_sep = false;
        }
    }
    result = unified;

    // Trim trailing separator (could happen if input ended with separator before non-sep trimming)
    result = result.trim_end_matches(['_', '-']).to_string();

    if result.is_empty() {
        return "pkg".to_string();
    }

    // Prepend pkg_ if starts with digit
    if result.chars().next().unwrap().is_ascii_digit() {
        result = format!("pkg_{}", result);
    }

    // Prepend pkg_ if result is a reserved name
    if is_reserved_name(&result) {
        result = format!("pkg_{}", result);
    }

    result
}

/// Convert a package name to a module name by replacing hyphens with underscores.
pub fn package_name_to_module_name(name: &str) -> String {
    name.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // is_reserved_name tests
    // ========================================================================

    #[test]
    fn test_is_reserved_name() {
        assert!(is_reserved_name("root"));
        assert!(is_reserved_name("self"));
        assert!(is_reserved_name("super"));
        assert!(is_reserved_name("std"));
        assert!(is_reserved_name("zoya"));
    }

    #[test]
    fn test_is_not_reserved_name() {
        assert!(!is_reserved_name("foo"));
        assert!(!is_reserved_name("rooter"));
        assert!(!is_reserved_name("selfie"));
        assert!(!is_reserved_name("supermarket"));
        assert!(!is_reserved_name("std_lib"));
        assert!(!is_reserved_name(""));
    }

    // ========================================================================
    // is_pascal_case tests
    // ========================================================================

    #[test]
    fn test_is_pascal_case_valid() {
        assert!(is_pascal_case("Foo"));
        assert!(is_pascal_case("FooBar"));
        assert!(is_pascal_case("F"));
        assert!(is_pascal_case("MyType"));
        assert!(is_pascal_case("HTTP")); // All caps is valid PascalCase
        assert!(is_pascal_case("HTMLParser"));
    }

    #[test]
    fn test_is_pascal_case_with_numbers() {
        assert!(is_pascal_case("Foo123"));
        assert!(is_pascal_case("Type2"));
        assert!(is_pascal_case("V1"));
    }

    #[test]
    fn test_is_pascal_case_invalid_lowercase_start() {
        assert!(!is_pascal_case("foo"));
        assert!(!is_pascal_case("fooBar"));
        assert!(!is_pascal_case("myType"));
    }

    #[test]
    fn test_is_pascal_case_invalid_underscore() {
        assert!(!is_pascal_case("Foo_Bar"));
        assert!(!is_pascal_case("_Foo"));
        assert!(!is_pascal_case("Foo_"));
    }

    #[test]
    fn test_is_pascal_case_empty_string() {
        assert!(!is_pascal_case(""));
    }

    #[test]
    fn test_is_pascal_case_number_start() {
        assert!(!is_pascal_case("1Foo"));
        assert!(!is_pascal_case("123"));
    }

    #[test]
    fn test_is_pascal_case_special_chars() {
        assert!(!is_pascal_case("Foo-Bar"));
        assert!(!is_pascal_case("Foo.Bar"));
        assert!(!is_pascal_case("Foo Bar"));
    }

    // ========================================================================
    // is_snake_case tests
    // ========================================================================

    #[test]
    fn test_is_snake_case_valid() {
        assert!(is_snake_case("foo"));
        assert!(is_snake_case("foo_bar"));
        assert!(is_snake_case("foo_bar_baz"));
    }

    #[test]
    fn test_is_snake_case_no_leading_underscore() {
        assert!(!is_snake_case("_foo"));
        assert!(!is_snake_case("_"));
        assert!(!is_snake_case("__"));
    }

    #[test]
    fn test_is_snake_case_with_numbers() {
        assert!(is_snake_case("foo123"));
        assert!(is_snake_case("foo_123"));
        assert!(is_snake_case("v1"));
        assert!(is_snake_case("type2_impl"));
    }

    #[test]
    fn test_is_snake_case_single_char() {
        assert!(is_snake_case("x"));
        assert!(is_snake_case("a"));
    }

    #[test]
    fn test_is_snake_case_invalid_uppercase() {
        assert!(!is_snake_case("Foo"));
        assert!(!is_snake_case("fooBar"));
        assert!(!is_snake_case("foo_Bar"));
        assert!(!is_snake_case("FOO"));
    }

    #[test]
    fn test_is_snake_case_empty_string() {
        assert!(!is_snake_case(""));
    }

    #[test]
    fn test_is_snake_case_number_start() {
        assert!(!is_snake_case("1foo"));
        assert!(!is_snake_case("123"));
    }

    #[test]
    fn test_is_snake_case_special_chars() {
        assert!(!is_snake_case("foo-bar"));
        assert!(!is_snake_case("foo.bar"));
        assert!(!is_snake_case("foo bar"));
    }

    // ========================================================================
    // is_valid_identifier / is_valid_type_name tests
    // ========================================================================

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("foo_bar"));
        assert!(!is_valid_identifier("Foo"));
        assert!(!is_valid_identifier("_foo"));
        assert!(!is_valid_identifier(""));
    }

    #[test]
    fn test_is_valid_type_name() {
        assert!(is_valid_type_name("Foo"));
        assert!(is_valid_type_name("FooBar"));
        assert!(!is_valid_type_name("foo"));
        assert!(!is_valid_type_name("Foo_Bar"));
        assert!(!is_valid_type_name(""));
    }

    #[test]
    fn test_is_valid_field_name() {
        assert!(is_valid_field_name("foo"));
        assert!(is_valid_field_name("foo_bar"));
        assert!(!is_valid_field_name("Foo"));
        assert!(!is_valid_field_name("_foo"));
        assert!(!is_valid_field_name(""));
    }

    // ========================================================================
    // is_valid_package_name tests
    // ========================================================================

    #[test]
    fn test_is_valid_package_name() {
        assert!(is_valid_package_name("myproject"));
        assert!(is_valid_package_name("my_project"));
        assert!(is_valid_package_name("my-project"));
        assert!(is_valid_package_name("foo-bar-baz"));
        assert!(is_valid_package_name("project123"));
        assert!(is_valid_package_name("a"));
    }

    #[test]
    fn test_is_valid_package_name_invalid() {
        assert!(!is_valid_package_name(""));
        assert!(!is_valid_package_name("123project"));
        assert!(!is_valid_package_name("_project"));
        assert!(!is_valid_package_name("-project"));
        assert!(!is_valid_package_name("MyProject"));
        assert!(!is_valid_package_name("my project"));
    }

    #[test]
    fn test_is_valid_package_name_reserved() {
        for name in RESERVED_NAMES {
            assert!(
                !is_valid_package_name(name),
                "reserved name '{}' should be rejected",
                name
            );
        }
    }

    #[test]
    fn test_is_valid_package_name_reserved_prefixed_ok() {
        assert!(is_valid_package_name("std-lib"));
        assert!(is_valid_package_name("zoya-utils"));
        assert!(is_valid_package_name("selfie"));
        assert!(is_valid_package_name("supermarket"));
        assert!(is_valid_package_name("rooter"));
    }

    // ========================================================================
    // is_valid_module_name tests
    // ========================================================================

    #[test]
    fn test_valid_module_names() {
        assert!(is_valid_module_name("utils"));
        assert!(is_valid_module_name("my_helpers"));
        assert!(is_valid_module_name("v2"));
        assert!(is_valid_module_name("a"));
        assert!(is_valid_module_name("foo_bar_baz"));
        assert!(is_valid_module_name("mod123"));
    }

    #[test]
    fn test_invalid_module_name_pascal_case() {
        assert!(!is_valid_module_name("MyModule"));
        assert!(!is_valid_module_name("Utils"));
    }

    #[test]
    fn test_invalid_module_name_leading_underscore() {
        assert!(!is_valid_module_name("_private"));
        assert!(!is_valid_module_name("_"));
    }

    #[test]
    fn test_invalid_module_name_uppercase() {
        assert!(!is_valid_module_name("UPPER"));
        assert!(!is_valid_module_name("FOO_BAR"));
    }

    #[test]
    fn test_invalid_module_name_empty() {
        assert!(!is_valid_module_name(""));
    }

    #[test]
    fn test_invalid_module_name_starts_with_digit() {
        assert!(!is_valid_module_name("1foo"));
        assert!(!is_valid_module_name("123"));
    }

    #[test]
    fn test_invalid_module_name_reserved() {
        assert!(!is_valid_module_name("root"));
        assert!(!is_valid_module_name("self"));
        assert!(!is_valid_module_name("super"));
        assert!(!is_valid_module_name("std"));
        assert!(!is_valid_module_name("zoya"));
    }

    // ========================================================================
    // to_snake_case tests
    // ========================================================================

    #[test]
    fn test_to_snake_case_from_pascal() {
        assert_eq!(to_snake_case("FooBar"), "foo_bar");
        assert_eq!(to_snake_case("MyType"), "my_type");
        assert_eq!(to_snake_case("HTTPServer"), "h_t_t_p_server");
    }

    #[test]
    fn test_to_snake_case_already_snake() {
        assert_eq!(to_snake_case("foo_bar"), "foo_bar");
        assert_eq!(to_snake_case("foo"), "foo");
    }

    #[test]
    fn test_to_snake_case_single_word() {
        assert_eq!(to_snake_case("Foo"), "foo");
        assert_eq!(to_snake_case("F"), "f");
    }

    #[test]
    fn test_to_snake_case_consecutive_capitals() {
        assert_eq!(to_snake_case("XMLParser"), "x_m_l_parser");
        assert_eq!(to_snake_case("AB"), "a_b");
        assert_eq!(to_snake_case("ABC"), "a_b_c");
    }

    #[test]
    fn test_to_snake_case_with_numbers() {
        assert_eq!(to_snake_case("Type2"), "type2");
        assert_eq!(to_snake_case("V1Handler"), "v1_handler");
    }

    #[test]
    fn test_to_snake_case_empty() {
        assert_eq!(to_snake_case(""), "");
    }

    #[test]
    fn test_to_snake_case_idempotent() {
        let input = "my_variable_name";
        assert_eq!(to_snake_case(input), input);
    }

    // ========================================================================
    // to_pascal_case tests
    // ========================================================================

    #[test]
    fn test_to_pascal_case_from_snake() {
        assert_eq!(to_pascal_case("foo_bar"), "FooBar");
        assert_eq!(to_pascal_case("my_type"), "MyType");
        assert_eq!(to_pascal_case("http_server"), "HttpServer");
    }

    #[test]
    fn test_to_pascal_case_already_pascal() {
        assert_eq!(to_pascal_case("FooBar"), "FooBar");
        assert_eq!(to_pascal_case("Foo"), "Foo");
    }

    #[test]
    fn test_to_pascal_case_single_word() {
        assert_eq!(to_pascal_case("foo"), "Foo");
        assert_eq!(to_pascal_case("f"), "F");
    }

    #[test]
    fn test_to_pascal_case_consecutive_underscores() {
        assert_eq!(to_pascal_case("foo__bar"), "FooBar");
        assert_eq!(to_pascal_case("__foo"), "Foo");
    }

    #[test]
    fn test_to_pascal_case_leading_trailing_underscores() {
        assert_eq!(to_pascal_case("_foo"), "Foo");
        assert_eq!(to_pascal_case("foo_"), "Foo");
        assert_eq!(to_pascal_case("_foo_"), "Foo");
    }

    #[test]
    fn test_to_pascal_case_with_numbers() {
        assert_eq!(to_pascal_case("type_2"), "Type2");
        assert_eq!(to_pascal_case("v1_handler"), "V1Handler");
    }

    #[test]
    fn test_to_pascal_case_empty() {
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn test_to_pascal_case_only_underscores() {
        assert_eq!(to_pascal_case("_"), "");
        assert_eq!(to_pascal_case("__"), "");
        assert_eq!(to_pascal_case("___"), "");
    }

    #[test]
    fn test_to_pascal_case_idempotent_on_pascal() {
        let input = "MyTypeName";
        assert_eq!(to_pascal_case(input), input);
    }

    // ========================================================================
    // sanitize_package_name tests
    // ========================================================================

    #[test]
    fn test_sanitize_package_name() {
        assert_eq!(sanitize_package_name("my-project"), "my-project");
        assert_eq!(sanitize_package_name("MyProject"), "myproject");
        assert_eq!(sanitize_package_name("123project"), "pkg_123project");
        assert_eq!(sanitize_package_name("  spaces  "), "spaces");
        assert_eq!(sanitize_package_name(""), "pkg");
        assert_eq!(sanitize_package_name("---"), "pkg");
        assert_eq!(sanitize_package_name("_leading"), "leading");
        assert_eq!(sanitize_package_name("trailing_"), "trailing");
        assert_eq!(sanitize_package_name("-leading"), "leading");
        assert_eq!(sanitize_package_name("trailing-"), "trailing");
    }

    #[test]
    fn test_sanitize_package_name_collapses_separators() {
        assert_eq!(sanitize_package_name("my--project"), "my-project");
        assert_eq!(sanitize_package_name("my__project"), "my_project");
        assert_eq!(sanitize_package_name("foo_-bar"), "foo_bar");
        assert_eq!(sanitize_package_name("foo-_bar"), "foo-bar");
    }

    #[test]
    fn test_sanitize_package_name_unifies_separators() {
        // First separator is '_', so '-' becomes '_'
        assert_eq!(sanitize_package_name("foo_bar-yo"), "foo_bar_yo");
        // First separator is '-', so '_' becomes '-'
        assert_eq!(sanitize_package_name("foo-bar_yo"), "foo-bar-yo");
    }

    #[test]
    fn test_sanitize_reserved_names() {
        assert_eq!(sanitize_package_name("std"), "pkg_std");
        assert_eq!(sanitize_package_name("zoya"), "pkg_zoya");
        assert_eq!(sanitize_package_name("root"), "pkg_root");
        assert_eq!(sanitize_package_name("self"), "pkg_self");
        assert_eq!(sanitize_package_name("super"), "pkg_super");
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
            "std",
            "zoya",
            "root",
            "self",
            "super",
        ];

        for input in inputs {
            let sanitized = sanitize_package_name(input);
            assert!(
                is_valid_package_name(&sanitized),
                "sanitize_package_name({:?}) = {:?} should be valid",
                input,
                sanitized
            );
        }
    }

    // ========================================================================
    // package_name_to_module_name tests
    // ========================================================================

    #[test]
    fn test_package_name_to_module_name() {
        assert_eq!(package_name_to_module_name("my-project"), "my_project");
        assert_eq!(package_name_to_module_name("simple"), "simple");
        assert_eq!(package_name_to_module_name("foo-bar-baz"), "foo_bar_baz");
    }
}
