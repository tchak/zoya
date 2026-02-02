/// Check if name is PascalCase (starts with uppercase, no underscores)
pub fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {
            name.chars().all(|c| c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

/// Check if name is snake_case (lowercase with underscores, can start with underscore)
pub fn is_snake_case(name: &str) -> bool {
    let mut chars = name.chars().peekable();
    match chars.peek() {
        Some(c) if c.is_ascii_lowercase() || *c == '_' => {
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }
        _ => false,
    }
}

/// Convert a name to snake_case for error message suggestions
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

/// Convert a name to PascalCase for error message suggestions
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(is_snake_case("_foo"));
        assert!(is_snake_case("_"));
        assert!(is_snake_case("__"));
        assert!(is_snake_case("foo_bar_baz"));
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
        // Each capital gets its own underscore
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
        // Converting snake_case to snake_case should be idempotent
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
        // Converting PascalCase to PascalCase (without underscores) should be mostly idempotent
        let input = "MyTypeName";
        assert_eq!(to_pascal_case(input), input);
    }
}
