use zoya_loader::{MemorySource, load_memory_package};

use crate::check::check;

/// Helper: parse source into a package, type-check with std as dep, return error message if any.
fn check_source(source: &str) -> Result<(), String> {
    let mem = MemorySource::new().with_module("root", source);
    let pkg = load_memory_package(&mem, zoya_loader::Mode::Dev).map_err(|e| format!("{}", e))?;
    let std = zoya_std::std();
    check(&pkg, &[std]).map(|_| ()).map_err(|e| e.message)
}

// String method tests

#[test]
fn test_check_method_call_len() {
    let result = check_source(r#"pub fn main() -> Int { "hello".len() }"#);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_method_call_is_empty() {
    let result = check_source(r#"pub fn main() -> Bool { "".is_empty() }"#);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_method_call_contains() {
    let result = check_source(r#"pub fn main() -> Bool { "hello".contains("ell") }"#);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_method_call_to_uppercase() {
    let result = check_source(r#"pub fn main() -> String { "hello".to_uppercase() }"#);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_method_call_trim() {
    let result = check_source(r#"pub fn main() -> String { "  hello  ".trim() }"#);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_method_call_unknown_method() {
    let result = check_source(r#"pub fn main() -> Int { "hello".foo() }"#);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("no method 'foo'"));
}

#[test]
fn test_check_method_call_on_int_error() {
    let result = check_source("pub fn main() -> Int { 42.len() }");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("no method 'len' on type Int"),);
}

#[test]
fn test_check_method_call_wrong_arg_count() {
    let result = check_source(r#"pub fn main() -> Bool { "hello".contains() }"#);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("expects 1 argument"));
}

#[test]
fn test_check_method_call_wrong_arg_type() {
    let result = check_source(r#"pub fn main() -> Bool { "hello".contains(42) }"#);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("type mismatch"));
}

#[test]
fn test_check_chained_method_calls() {
    let result = check_source(r#"pub fn main() -> Int { "hello".to_uppercase().len() }"#);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

// List method tests

#[test]
fn test_check_list_len() {
    let result = check_source("pub fn main() -> Int { [1, 2].len() }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_is_empty() {
    let result = check_source("pub fn main() -> Bool { [1, 2].is_empty() }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_reverse() {
    let result = check_source("pub fn main() -> List<Int> { [1, 2].reverse() }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_push() {
    let result = check_source("pub fn main() -> List<Int> { [1, 2].push(3) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_push_type_mismatch() {
    let result = check_source(r#"pub fn main() -> List<Int> { [1, 2].push("hello") }"#);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("type mismatch"));
}

#[test]
fn test_check_list_map() {
    let result = check_source("pub fn main() -> List<Int> { [1, 2].map(|x| x + 1) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_map_type_change() {
    let result = check_source("pub fn main() -> List<Bool> { [1, 2].map(|x| x > 0) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_filter() {
    let result = check_source("pub fn main() -> List<Int> { [1, 2, 3].filter(|x| x > 1) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_fold() {
    let result = check_source("pub fn main() -> Int { [1, 2, 3].fold(0, |acc, x| acc + x) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_filter_map() {
    let result = check_source(
        r#"
        pub fn main() -> List<Int> {
            [1, 2, 3].filter_map(|x| match x > 1 {
                true => Some(x * 2),
                false => None,
            })
        }
        "#,
    );
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_first() {
    let result = check_source("pub fn main() -> Option<Int> { [1, 2].first() }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_last() {
    let result = check_source("pub fn main() -> Option<Int> { [1, 2].last() }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_truncate() {
    let result = check_source("pub fn main() -> List<Int> { [1, 2, 3].truncate(2) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_insert() {
    let result = check_source("pub fn main() -> List<Int> { [1, 3].insert(1, 2) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_remove() {
    let result = check_source("pub fn main() -> List<Int> { [1, 2, 3].remove(1) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_chained_methods() {
    // [1, 2].push(3).reverse()
    let result = check_source("pub fn main() -> List<Int> { [1, 2].push(3).reverse() }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}

#[test]
fn test_check_list_filter_map_chain() {
    let result =
        check_source("pub fn main() -> List<Int> { [1, 2, 3].filter(|x| x > 1).map(|x| x * 10) }");
    assert!(result.is_ok(), "Expected OK, got: {:?}", result);
}
