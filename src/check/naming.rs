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
