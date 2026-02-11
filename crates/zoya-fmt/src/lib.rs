mod format;

use zoya_ast::{Item, ModDecl, Visibility};

use format::{fmt_item, fmt_mod_decl};

const WIDTH: usize = 120;

/// Format a parsed Zoya module into canonical pretty-printed source code.
///
/// Ordering rules:
/// 1. Mod declarations first (pub before private, stable sort)
/// 2. Use declarations next (pub before private, stable sort)
/// 3. Other items last (pub before private, stable sort)
///
/// Blank lines: none between consecutive mods, none between consecutive uses,
/// blank line between each other item. Blank line separating groups. Trailing newline.
pub fn fmt(mods: Vec<ModDecl>, items: Vec<Item>) -> String {
    let (mut pub_mods, mut priv_mods): (Vec<_>, Vec<_>) = mods
        .into_iter()
        .partition(|m| m.visibility == Visibility::Public);
    // Stable sort preserves original order within each group - no re-sorting needed
    let _ = (&mut pub_mods, &mut priv_mods);

    let ordered_mods: Vec<&ModDecl> = pub_mods.iter().chain(priv_mods.iter()).collect();

    // Separate use declarations from other items
    let (uses, others): (Vec<_>, Vec<_>) =
        items.into_iter().partition(|i| matches!(i, Item::Use(_)));

    let (mut pub_uses, mut priv_uses): (Vec<_>, Vec<_>) = uses
        .into_iter()
        .partition(|i| matches!(i, Item::Use(u) if u.visibility == Visibility::Public));
    let _ = (&mut pub_uses, &mut priv_uses);

    let ordered_uses: Vec<&Item> = pub_uses.iter().chain(priv_uses.iter()).collect();

    let (mut pub_others, mut priv_others): (Vec<_>, Vec<_>) = others
        .into_iter()
        .partition(|i| item_visibility(i) == Visibility::Public);
    let _ = (&mut pub_others, &mut priv_others);

    let ordered_others: Vec<&Item> = pub_others.iter().chain(priv_others.iter()).collect();

    let mut output = String::new();
    let mut has_content = false;

    // Mods group (newline separated)
    for m in &ordered_mods {
        if has_content {
            output.push('\n');
        }
        let mut rendered = String::new();
        fmt_mod_decl(m).render_fmt(WIDTH, &mut rendered).unwrap();
        output.push_str(&rendered);
        has_content = true;
    }

    // Uses group (newline separated, continues from mods without blank line)
    for u in &ordered_uses {
        if has_content {
            output.push('\n');
        }
        let mut rendered = String::new();
        fmt_item(u).render_fmt(WIDTH, &mut rendered).unwrap();
        output.push_str(&rendered);
        has_content = true;
    }

    // Other items group (blank line between each, and blank line after mods/uses)
    for item in &ordered_others {
        if has_content {
            output.push_str("\n\n");
        }
        let mut rendered = String::new();
        fmt_item(item).render_fmt(WIDTH, &mut rendered).unwrap();
        output.push_str(&rendered);
        has_content = true;
    }

    if has_content {
        output.push('\n');
    }

    output
}

fn item_visibility(item: &Item) -> Visibility {
    match item {
        Item::Function(f) => f.visibility,
        Item::Struct(s) => s.visibility,
        Item::Enum(e) => e.visibility,
        Item::TypeAlias(t) => t.visibility,
        Item::Use(u) => u.visibility,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_source(source: &str) -> String {
        let tokens = zoya_lexer::lex(source).expect("lex failed");
        let (mods, items) = zoya_parser::parse_module(tokens).expect("parse failed");
        fmt(mods, items)
    }

    // --- Simple items ---

    #[test]
    fn test_simple_function() {
        let result = format_source("fn add(x: Int, y: Int) -> Int { x + y }");
        assert_eq!(result, "fn add(x: Int, y: Int) -> Int x + y\n");
    }

    #[test]
    fn test_pub_function() {
        let result = format_source("pub fn main() -> Int { 42 }");
        assert_eq!(result, "pub fn main() -> Int 42\n");
    }

    #[test]
    fn test_function_with_block_body() {
        let result = format_source("fn foo() -> Int { let x = 1; x + 2 }");
        assert_eq!(result, "fn foo() -> Int {\n  let x = 1;\n  x + 2\n}\n");
    }

    #[test]
    fn test_struct() {
        let result = format_source("pub struct Point { x: Int, y: Int }");
        assert_eq!(result, "pub struct Point { x: Int, y: Int }\n");
    }

    #[test]
    fn test_enum() {
        let result = format_source("pub enum Option<T> { None, Some(T) }");
        assert_eq!(result, "pub enum Option<T> { None, Some(T) }\n");
    }

    #[test]
    fn test_type_alias() {
        let result = format_source("pub type Pair<T> = (T, T)");
        assert_eq!(result, "pub type Pair<T> = (T, T)\n");
    }

    #[test]
    fn test_use_decl() {
        let result = format_source("pub use self::Option::*");
        assert_eq!(result, "pub use self::Option::*\n");
    }

    #[test]
    fn test_use_group() {
        let result = format_source("use root::foo::{add, divide}");
        assert_eq!(result, "use root::foo::{add, divide}\n");
    }

    // --- Annotations ---

    #[test]
    fn test_annotation_on_function() {
        let result = format_source("#[test] fn foo() 42");
        assert_eq!(result, "#[test]\nfn foo() 42\n");
    }

    #[test]
    fn test_multiple_annotations() {
        let result = format_source("#[test] #[inline] pub fn foo() 42");
        assert_eq!(result, "#[test]\n#[inline]\npub fn foo() 42\n");
    }

    #[test]
    fn test_annotation_on_struct() {
        let result = format_source("#[derive] pub struct Point { x: Int }");
        assert_eq!(result, "#[derive]\npub struct Point { x: Int }\n");
    }

    #[test]
    fn test_annotation_on_enum() {
        let result = format_source("#[derive] enum Color { Red, Blue }");
        assert_eq!(result, "#[derive]\nenum Color { Red, Blue }\n");
    }

    #[test]
    fn test_annotation_on_type_alias() {
        let result = format_source("#[deprecated] type UserId = Int");
        assert_eq!(result, "#[deprecated]\ntype UserId = Int\n");
    }

    #[test]
    fn test_annotation_on_use() {
        let result = format_source("#[allow] use root::foo::bar");
        assert_eq!(result, "#[allow]\nuse root::foo::bar\n");
    }

    #[test]
    fn test_annotation_idempotent() {
        let source = "#[test]\n#[inline]\npub fn foo() 42";
        let result = format_source(source);
        assert_eq!(result, "#[test]\n#[inline]\npub fn foo() 42\n");
        // Idempotency: formatting again yields the same output
        let result2 = format_source(&result);
        assert_eq!(result, result2);
    }

    // --- Ordering ---

    #[test]
    fn test_ordering_pub_before_private() {
        let result = format_source("fn bar() -> Int 1\npub fn foo() -> Int 2");
        assert_eq!(result, "pub fn foo() -> Int 2\n\nfn bar() -> Int 1\n");
    }

    #[test]
    fn test_ordering_mods_before_uses_before_items() {
        let result = format_source("pub fn main() -> Int 42\nuse root::foo::bar\npub mod utils");
        assert_eq!(
            result,
            "pub mod utils\nuse root::foo::bar\n\npub fn main() -> Int 42\n"
        );
    }

    #[test]
    fn test_ordering_mods_pub_before_private() {
        let result = format_source("mod bar\npub mod foo");
        assert_eq!(result, "pub mod foo\nmod bar\n");
    }

    // --- No-braces heuristic ---

    #[test]
    fn test_no_braces_simple_expr() {
        // { x + y } unwraps to x + y, formatter should not add braces
        let result = format_source("fn add(x: Int, y: Int) -> Int { x + y }");
        assert_eq!(result, "fn add(x: Int, y: Int) -> Int x + y\n");
    }

    #[test]
    fn test_braces_for_block() {
        let result = format_source("fn foo() -> Int { let x = 1; x }");
        assert_eq!(result, "fn foo() -> Int {\n  let x = 1;\n  x\n}\n");
    }

    #[test]
    fn test_braces_for_match_body() {
        let result = format_source("fn foo(x: Int) -> Int { match x { 0 => 1, _ => 2 } }");
        assert_eq!(
            result,
            "fn foo(x: Int) -> Int {\n  match x {\n    0 => 1,\n    _ => 2,\n  }\n}\n"
        );
    }

    // --- Expressions ---

    #[test]
    fn test_binop_precedence() {
        // a + b * c should not need parens; (a + b) * c should have parens
        let result = format_source("fn f() -> Int { 1 + 2 * 3 }");
        assert_eq!(result, "fn f() -> Int 1 + 2 * 3\n");

        let result = format_source("fn f() -> Int { (1 + 2) * 3 }");
        assert_eq!(result, "fn f() -> Int (1 + 2) * 3\n");
    }

    #[test]
    fn test_binop_left_associativity() {
        // a - b - c is (a - b) - c, right child needs parens if same prec
        let result = format_source("fn f() -> Int { 1 - 2 - 3 }");
        assert_eq!(result, "fn f() -> Int 1 - 2 - 3\n");
    }

    #[test]
    fn test_unary_neg() {
        let result = format_source("fn f() -> Int { -42 }");
        assert_eq!(result, "fn f() -> Int -42\n");
    }

    #[test]
    fn test_call_expr() {
        let result = format_source("fn f() -> Int { add(1, 2) }");
        assert_eq!(result, "fn f() -> Int add(1, 2)\n");
    }

    #[test]
    fn test_method_call() {
        let result = format_source("fn f(x: Int) -> Int { x.to_string() }");
        assert_eq!(result, "fn f(x: Int) -> Int x.to_string()\n");
    }

    #[test]
    fn test_field_access() {
        let result = format_source("fn f(p: Point) -> Int { p.x }");
        assert_eq!(result, "fn f(p: Point) -> Int p.x\n");
    }

    #[test]
    fn test_lambda() {
        let result = format_source("fn f() -> Int -> Int { |x| x + 1 }");
        assert_eq!(result, "fn f() -> Int -> Int |x| x + 1\n");
    }

    #[test]
    fn test_lambda_with_type() {
        let result = format_source("fn f() -> Int -> Int { |x: Int| -> Int x + 1 }");
        assert_eq!(result, "fn f() -> Int -> Int |x: Int| -> Int x + 1\n");
    }

    #[test]
    fn test_list_expr() {
        let result = format_source("fn f() -> List<Int> { [1, 2, 3] }");
        assert_eq!(result, "fn f() -> List<Int> [1, 2, 3]\n");
    }

    #[test]
    fn test_tuple_expr() {
        let result = format_source("fn f() -> (Int, Int) { (1, 2) }");
        assert_eq!(result, "fn f() -> (Int, Int) (1, 2)\n");
    }

    #[test]
    fn test_single_element_tuple() {
        // Parser treats (Int) as parenthesized Int, not a single-element tuple type
        let result = format_source("fn f() -> (Int) { (42,) }");
        assert_eq!(result, "fn f() -> Int (42,)\n");
    }

    #[test]
    fn test_struct_expr() {
        let result = format_source("fn f() -> Point { Point { x: 1, y: 2 } }");
        assert_eq!(result, "fn f() -> Point Point { x: 1, y: 2 }\n");
    }

    #[test]
    fn test_struct_expr_shorthand() {
        let result = format_source("fn f(x: Int, y: Int) -> Point { Point { x, y } }");
        assert_eq!(result, "fn f(x: Int, y: Int) -> Point Point { x, y }\n");
    }

    #[test]
    fn test_string_expr() {
        let result = format_source(r#"fn f() -> String { "hello" }"#);
        assert_eq!(result, "fn f() -> String \"hello\"\n");
    }

    #[test]
    fn test_bool_expr() {
        let result = format_source("fn f() -> Bool { true }");
        assert_eq!(result, "fn f() -> Bool true\n");
    }

    #[test]
    fn test_float_expr() {
        let result = format_source("fn f() -> Float { 3.14 }");
        assert_eq!(result, "fn f() -> Float 3.14\n");
    }

    #[test]
    fn test_bigint_expr() {
        let result = format_source("fn f() -> BigInt { 42n }");
        assert_eq!(result, "fn f() -> BigInt 42n\n");
    }

    #[test]
    fn test_empty_tuple() {
        let result = format_source("fn f() -> () { () }");
        assert_eq!(result, "fn f() -> () ()\n");
    }

    #[test]
    fn test_empty_list() {
        let result = format_source("fn f() -> List<Int> { [] }");
        assert_eq!(result, "fn f() -> List<Int> []\n");
    }

    // --- Patterns ---

    #[test]
    fn test_pattern_wildcard() {
        let result = format_source("fn f(x: Int) -> Int { match x { _ => 0 } }");
        assert!(result.contains("_ => 0"));
    }

    #[test]
    fn test_pattern_literal() {
        let result = format_source("fn f(x: Int) -> Int { match x { 0 => 1, _ => 2 } }");
        assert!(result.contains("0 => 1"));
    }

    #[test]
    fn test_pattern_list_empty() {
        let result = format_source("fn f(xs: List<Int>) -> Int { match xs { [] => 0, _ => 1 } }");
        assert!(result.contains("[] => 0"));
    }

    #[test]
    fn test_pattern_list_exact() {
        let result =
            format_source("fn f(xs: List<Int>) -> Int { match xs { [a, b] => a + b, _ => 0 } }");
        assert!(result.contains("[a, b] => a + b"));
    }

    #[test]
    fn test_pattern_list_prefix() {
        let result =
            format_source("fn f(xs: List<Int>) -> Int { match xs { [a, ..] => a, _ => 0 } }");
        assert!(result.contains("[a, ..] => a"));
    }

    #[test]
    fn test_pattern_list_suffix() {
        let result =
            format_source("fn f(xs: List<Int>) -> Int { match xs { [.., z] => z, _ => 0 } }");
        assert!(result.contains("[.., z] => z"));
    }

    #[test]
    fn test_pattern_list_prefix_suffix() {
        let result = format_source(
            "fn f(xs: List<Int>) -> Int { match xs { [a, .., z] => a + z, _ => 0 } }",
        );
        assert!(result.contains("[a, .., z] => a + z"));
    }

    #[test]
    fn test_pattern_list_rest_binding() {
        let result = format_source(
            "fn f(xs: List<Int>) -> List<Int> { match xs { [a, rest @ ..] => rest, _ => [] } }",
        );
        assert!(result.contains("[a, rest @ ..] => rest"));
    }

    #[test]
    fn test_pattern_tuple() {
        let result = format_source("fn f(t: (Int, Int)) -> Int { match t { (a, b) => a + b } }");
        assert!(result.contains("(a, b) => a + b"));
    }

    #[test]
    fn test_pattern_struct() {
        let result = format_source("fn f(p: Point) -> Int { match p { Point { x, y } => x + y } }");
        assert!(result.contains("Point { x, y } => x + y"));
    }

    #[test]
    fn test_pattern_struct_partial() {
        let result = format_source("fn f(p: Point) -> Int { match p { Point { x, .. } => x } }");
        assert!(result.contains("Point { x, .. } => x"));
    }

    #[test]
    fn test_pattern_as() {
        let result = format_source("fn f(x: Int) -> Int { match x { n @ 0 => n, _ => 1 } }");
        assert!(result.contains("n @ 0 => n"));
    }

    #[test]
    fn test_pattern_call() {
        let result = format_source(
            "fn f(o: Option<Int>) -> Int { match o { Option::Some(x) => x, Option::None => 0 } }",
        );
        assert!(result.contains("Option::Some(x) => x"));
        assert!(result.contains("Option::None => 0"));
    }

    // --- Match ---

    #[test]
    fn test_match_multi_arm() {
        let result = format_source("fn f(x: Int) -> Int { match x { 0 => 1, 1 => 2, _ => 3 } }");
        assert_eq!(
            result,
            "fn f(x: Int) -> Int {\n  match x {\n    0 => 1,\n    1 => 2,\n    _ => 3,\n  }\n}\n"
        );
    }

    #[test]
    fn test_match_with_block_arm() {
        let result =
            format_source("fn f(x: Int) -> Int { match x { 0 => { let y = 1; y }, _ => 0 } }");
        assert!(result.contains("0 => {\n      let y = 1;\n      y\n    }"));
    }

    // --- Enum with struct variant ---

    #[test]
    fn test_enum_struct_variant() {
        let result = format_source("enum Message { Quit, Move { x: Int, y: Int }, Say(String) }");
        assert_eq!(
            result,
            "enum Message { Quit, Move { x: Int, y: Int }, Say(String) }\n"
        );
    }

    // --- Path with turbofish ---

    #[test]
    fn test_turbofish() {
        let result = format_source("fn f() -> Option<Int> { Option::None::<Int> }");
        assert_eq!(result, "fn f() -> Option<Int> Option::None::<Int>\n");
    }

    // --- Comparison operators ---

    #[test]
    fn test_comparison_operators() {
        let result = format_source("fn f(x: Int) -> Bool { x == 0 }");
        assert_eq!(result, "fn f(x: Int) -> Bool x == 0\n");

        let result = format_source("fn f(x: Int) -> Bool { x != 0 }");
        assert_eq!(result, "fn f(x: Int) -> Bool x != 0\n");

        let result = format_source("fn f(x: Int) -> Bool { x < 0 }");
        assert_eq!(result, "fn f(x: Int) -> Bool x < 0\n");
    }

    // --- Idempotency ---

    #[test]
    fn test_idempotency_simple() {
        let source = "pub fn main() -> Int { 42 }";
        let first = format_source(source);
        let second = format_source(&first);
        assert_eq!(first, second, "Formatter is not idempotent");
    }

    #[test]
    fn test_idempotency_complex() {
        let source = r#"
            fn bar() -> Int { 1 }
            pub fn foo(x: Int, y: Int) -> Int {
                let z = x + y;
                match z {
                    0 => bar(),
                    _ => z * 2,
                }
            }
            pub struct Point { x: Int, y: Int }
            pub enum Option<T> { None, Some(T) }
            pub type Pair<T> = (T, T)
        "#;
        let first = format_source(source);
        let second = format_source(&first);
        assert_eq!(first, second, "Formatter is not idempotent");
    }

    // --- Standard library idempotency ---

    #[test]
    fn test_idempotency_std_main() {
        let source = include_str!("../../zoya-std/src/std/main.zy");
        let first = format_source(source);
        let second = format_source(&first);
        assert_eq!(first, second, "std/main.zy not idempotent");
    }

    #[test]
    fn test_idempotency_std_option() {
        let source = include_str!("../../zoya-std/src/std/option.zy");
        let first = format_source(source);
        let second = format_source(&first);
        assert_eq!(first, second, "std/option.zy not idempotent");
    }

    #[test]
    fn test_idempotency_std_result() {
        let source = include_str!("../../zoya-std/src/std/result.zy");
        let first = format_source(source);
        let second = format_source(&first);
        assert_eq!(first, second, "std/result.zy not idempotent");
    }

    #[test]
    fn test_idempotency_std_prelude() {
        let source = include_str!("../../zoya-std/src/std/prelude.zy");
        let first = format_source(source);
        let second = format_source(&first);
        assert_eq!(first, second, "std/prelude.zy not idempotent");
    }

    // --- Function without return type ---

    #[test]
    fn test_function_no_return_type() {
        let result = format_source("fn f(x: Int) { x }");
        assert_eq!(result, "fn f(x: Int) x\n");
    }

    // --- Nested expressions ---

    #[test]
    fn test_chained_method_calls() {
        let result = format_source("fn f(x: Int) -> String { x.to_string().len() }");
        assert_eq!(result, "fn f(x: Int) -> String x.to_string().len()\n");
    }

    #[test]
    fn test_string_escape() {
        let result = format_source(r#"fn f() -> String { "hello\nworld" }"#);
        assert_eq!(result, "fn f() -> String \"hello\\nworld\"\n");
    }

    // --- Use with path prefixes ---

    #[test]
    fn test_use_root_prefix() {
        let result = format_source("use root::foo::bar");
        assert_eq!(result, "use root::foo::bar\n");
    }

    #[test]
    fn test_use_self_prefix() {
        let result = format_source("use self::Option::*");
        assert_eq!(result, "use self::Option::*\n");
    }

    #[test]
    fn test_use_super_prefix() {
        let result = format_source("use super::foo");
        assert_eq!(result, "use super::foo\n");
    }

    // --- Function type annotations ---

    #[test]
    fn test_function_type_single_param() {
        let result = format_source("fn f(g: Int -> Bool) -> Bool { g(1) }");
        assert_eq!(result, "fn f(g: Int -> Bool) -> Bool g(1)\n");
    }

    #[test]
    fn test_function_type_multi_param() {
        let result = format_source("fn f(g: (Int, Int) -> Bool) -> Bool { g(1, 2) }");
        assert_eq!(result, "fn f(g: (Int, Int) -> Bool) -> Bool g(1, 2)\n");
    }

    // --- Parameterized types ---

    #[test]
    fn test_parameterized_type() {
        let result = format_source("fn f(xs: List<Int>) -> List<Int> { xs }");
        assert_eq!(result, "fn f(xs: List<Int>) -> List<Int> xs\n");
    }

    // --- Empty struct ---

    #[test]
    fn test_empty_struct() {
        let result = format_source("struct Unit {}");
        assert_eq!(result, "struct Unit\n");
    }

    #[test]
    fn test_unit_struct_no_braces() {
        let result = format_source("struct Unit");
        assert_eq!(result, "struct Unit\n");
    }

    // --- Empty enum ---

    #[test]
    fn test_empty_enum() {
        let result = format_source("enum Never {}");
        assert_eq!(result, "enum Never {}\n");
    }

    // --- Unary op on binop ---

    #[test]
    fn test_unary_on_binop() {
        let result = format_source("fn f() -> Int { -(1 + 2) }");
        assert_eq!(result, "fn f() -> Int -(1 + 2)\n");
    }

    // --- Lambda with block body ---

    #[test]
    fn test_lambda_with_block() {
        let result = format_source("fn f() -> Int -> Int { |x| { let y = x + 1; y } }");
        assert_eq!(
            result,
            "fn f() -> Int -> Int |x| {\n  let y = x + 1;\n  y\n}\n"
        );
    }

    // --- Match arm with match body gets braces ---

    #[test]
    fn test_match_arm_match_body() {
        let result = format_source(
            "fn f(x: Int) -> Int { match x { 0 => match x { 0 => 1, _ => 2 }, _ => 3 } }",
        );
        // The inner match should be wrapped in braces
        assert!(result.contains("0 => {\n      match x {"));
    }
}
