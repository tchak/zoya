use chumsky::prelude::*;

use zoya_ast::{Item, ModDecl, Stmt};
use zoya_lexer::Token;

mod expressions;
mod helpers;
mod items;
mod patterns;
mod statements;
mod types;

use helpers::{mod_decl_parser, use_decl_parser};
use items::{attribute_parser, item_parser};
use statements::stmt_parser;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("{}", format_errors(.0))]
    SyntaxErrors(Vec<SyntaxError>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxError {
    pub span: zoya_lexer::Span,
    pub found: Option<String>,
    pub expected: Vec<String>,
    pub label: Option<String>,
}

fn display_token(t: &Token) -> String {
    match t {
        Token::Comma => "','".to_string(),
        Token::RBrace => "'}'".to_string(),
        Token::LBrace => "'{'".to_string(),
        Token::RParen => "')'".to_string(),
        Token::LParen => "'('".to_string(),
        Token::RBracket => "']'".to_string(),
        Token::LBracket => "'['".to_string(),
        Token::FatArrow => "'=>'".to_string(),
        Token::Arrow => "'->'".to_string(),
        Token::Semicolon => "';'".to_string(),
        Token::Colon => "':'".to_string(),
        Token::ColonColon => "'::'".to_string(),
        Token::Dot => "'.'".to_string(),
        Token::DotDot => "'..'".to_string(),
        Token::Eq => "'='".to_string(),
        Token::EqEq => "'=='".to_string(),
        Token::Ne => "'!='".to_string(),
        Token::Lt => "'<'".to_string(),
        Token::Gt => "'>'".to_string(),
        Token::Le => "'<='".to_string(),
        Token::Ge => "'>='".to_string(),
        Token::Plus => "'+'".to_string(),
        Token::Minus => "'-'".to_string(),
        Token::Star => "'*'".to_string(),
        Token::Slash => "'/'".to_string(),
        Token::Pipe => "'|'".to_string(),
        Token::At => "'@'".to_string(),
        Token::Hash => "'#'".to_string(),
        Token::Ident(s) => format!("'{}'", s),
        Token::String(s) => format!("\"{}\"", s),
        Token::Int(n) => format!("{}", n),
        Token::BigInt(n) => format!("{}n", n),
        Token::Float(f) => format!("{}", f),
        Token::Fn => "'fn'".to_string(),
        Token::Let => "'let'".to_string(),
        Token::Match => "'match'".to_string(),
        Token::True => "'true'".to_string(),
        Token::False => "'false'".to_string(),
        Token::Struct => "'struct'".to_string(),
        Token::Enum => "'enum'".to_string(),
        Token::Type => "'type'".to_string(),
        Token::Mod => "'mod'".to_string(),
        Token::Use => "'use'".to_string(),
        Token::Pub => "'pub'".to_string(),
        Token::Root => "'root'".to_string(),
        Token::Self_ => "'self'".to_string(),
        Token::Super => "'super'".to_string(),
    }
}

/// Convert chumsky Rich errors (with token-index spans) to ParseError (with byte-offset spans).
///
/// `byte_spans` maps token indices to byte-offset spans from the lexer.
fn convert_errors(errs: Vec<Rich<'_, Token>>, byte_spans: &[zoya_lexer::Span]) -> ParseError {
    let map_span = |token_span: &SimpleSpan| -> zoya_lexer::Span {
        let start_idx = token_span.start;
        let end_idx = token_span.end;
        let byte_start = byte_spans
            .get(start_idx)
            .map(|s| s.start)
            .unwrap_or(byte_spans.last().map(|s| s.end).unwrap_or(0));
        let byte_end = if end_idx > 0 {
            byte_spans
                .get(end_idx - 1)
                .map(|s| s.end)
                .unwrap_or(byte_spans.last().map(|s| s.end).unwrap_or(0))
        } else {
            byte_start
        };
        byte_start..byte_end
    };

    ParseError::SyntaxErrors(
        errs.into_iter()
            .map(|e| SyntaxError {
                span: map_span(e.span()),
                found: e.found().map(display_token),
                expected: e
                    .expected()
                    .map(|pat| match pat {
                        chumsky::error::RichPattern::Token(t) => display_token(t),
                        chumsky::error::RichPattern::Label(l) => l.to_string(),
                        chumsky::error::RichPattern::EndOfInput => "end of input".to_string(),
                        other => format!("{:?}", other),
                    })
                    .collect(),
                label: match e.reason() {
                    chumsky::error::RichReason::Custom(msg) => Some(msg.to_string()),
                    _ => None,
                },
            })
            .collect(),
    )
}

fn join_expected(expected: &[String]) -> String {
    match expected.len() {
        0 => "something else".to_string(),
        1 => expected[0].clone(),
        _ => {
            let last = &expected[expected.len() - 1];
            let rest = &expected[..expected.len() - 1];
            format!("{} or {}", rest.join(", "), last)
        }
    }
}

fn format_errors(errors: &[SyntaxError]) -> String {
    errors
        .iter()
        .map(|e| {
            let location = format!("at {}..{}", e.span.start, e.span.end);
            match (&e.label, e.found.as_ref()) {
                (Some(label), _) => format!("{} ({})", label, location),
                (None, Some(found)) => {
                    let expected = join_expected(&e.expected);
                    format!("found {} but expected {} ({})", found, expected, location)
                }
                (None, None) => {
                    let expected = join_expected(&e.expected);
                    format!(
                        "unexpected end of input, expected {} ({})",
                        expected, location
                    )
                }
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn split_tokens(spanned_tokens: Vec<zoya_lexer::Spanned>) -> (Vec<Token>, Vec<zoya_lexer::Span>) {
    spanned_tokens.into_iter().unzip()
}

/// Element type for REPL input parsing
enum InputElement {
    Item(Box<Item>),
    Stmt(Box<Stmt>),
}

/// Element type for module parsing
enum ModuleElement {
    Mod(ModDecl),
    Item(Box<Item>),
}

/// Parse REPL input: items and statements in any order.
///
/// This parser handles interactive input where definitions (type, function, etc.)
/// and statements (expressions, let bindings) can be interleaved in any order.
///
/// # Arguments
/// * `tokens` - Token stream from the lexer
///
/// # Returns
/// Tuple of (items, stmts) on success, or `ParseError` with diagnostics
pub fn parse_input(tokens: Vec<zoya_lexer::Spanned>) -> Result<(Vec<Item>, Vec<Stmt>), ParseError> {
    let (toks, byte_spans) = split_tokens(tokens);

    let element = choice((
        item_parser().map(|i| InputElement::Item(Box::new(i))),
        stmt_parser().map(|s| InputElement::Stmt(Box::new(s))),
    ));

    let parser = element.repeated().collect::<Vec<_>>().map(|elements| {
        let mut items = vec![];
        let mut stmts = vec![];
        for elem in elements {
            match elem {
                InputElement::Item(i) => items.push(*i),
                InputElement::Stmt(s) => stmts.push(*s),
            }
        }
        (items, stmts)
    });

    parser
        .parse(&toks)
        .into_result()
        .map_err(|errs| convert_errors(errs, &byte_spans))
}

/// Parse a module file: mod declarations, use declarations, and items in any order.
///
/// Module files can declare submodules, import names, and define items (types, functions, etc.)
/// in any order.
///
/// # Arguments
/// * `tokens` - Token stream from the lexer
///
/// # Returns
/// Tuple of (mod_decls, items) on success, or `ParseError`
pub fn parse_module(
    tokens: Vec<zoya_lexer::Spanned>,
) -> Result<(Vec<ModDecl>, Vec<Item>), ParseError> {
    let (toks, byte_spans) = split_tokens(tokens);

    let attributes = attribute_parser().repeated().collect::<Vec<_>>();

    let use_with_attrs = attributes.then(use_decl_parser()).map(|(attrs, mut u)| {
        u.attributes = attrs;
        ModuleElement::Item(Box::new(Item::Use(u)))
    });

    let element = choice((
        mod_decl_parser().map(ModuleElement::Mod),
        use_with_attrs,
        item_parser().map(|i| ModuleElement::Item(Box::new(i))),
    ));

    let parser = element.repeated().collect::<Vec<_>>().map(|elements| {
        let mut mods = vec![];
        let mut items = vec![];
        for elem in elements {
            match elem {
                ModuleElement::Mod(m) => mods.push(m),
                ModuleElement::Item(i) => items.push(*i),
            }
        }
        (mods, items)
    });

    parser
        .parse(&toks)
        .into_result()
        .map_err(|errs| convert_errors(errs, &byte_spans))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_lexer::lex;

    use crate::expressions::expr_parser;

    fn parse(tokens: Vec<zoya_lexer::Spanned>) -> Result<zoya_ast::Expr, ParseError> {
        let (toks, byte_spans) = split_tokens(tokens);
        expr_parser()
            .parse(&toks)
            .into_result()
            .map_err(|errs| convert_errors(errs, &byte_spans))
    }

    fn parse_item(tokens: Vec<zoya_lexer::Spanned>) -> Result<Item, ParseError> {
        let (toks, byte_spans) = split_tokens(tokens);
        item_parser()
            .parse(&toks)
            .into_result()
            .map_err(|errs| convert_errors(errs, &byte_spans))
    }

    fn parse_str(input: &str) -> Result<zoya_ast::Expr, ParseError> {
        let tokens = lex(input).expect("lexing failed");
        parse(tokens)
    }

    use zoya_ast::{BinOp, Expr, Path, UnaryOp};

    #[test]
    fn test_parse_integer() {
        let expr = parse_str("42").unwrap();
        assert_eq!(expr, Expr::Int(42));
    }

    #[test]
    fn test_parse_addition() {
        let expr = parse_str("2 + 3").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Int(2)),
                right: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn test_parse_precedence_mul_over_add() {
        let expr = parse_str("2 + 3 * 4").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Int(2)),
                right: Box::new(Expr::BinOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Int(3)),
                    right: Box::new(Expr::Int(4)),
                }),
            }
        );
    }

    #[test]
    fn test_parse_parentheses_override() {
        let expr = parse_str("(2 + 3) * 4").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Mul,
                left: Box::new(Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Int(2)),
                    right: Box::new(Expr::Int(3)),
                }),
                right: Box::new(Expr::Int(4)),
            }
        );
    }

    #[test]
    fn test_parse_left_associativity() {
        let expr = parse_str("1 - 2 - 3").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Sub,
                left: Box::new(Expr::BinOp {
                    op: BinOp::Sub,
                    left: Box::new(Expr::Int(1)),
                    right: Box::new(Expr::Int(2)),
                }),
                right: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn test_parse_all_operators() {
        let expr = parse_str("1 + 2 - 3 * 4 / 5").unwrap();
        // Should parse as: (1 + 2) - ((3 * 4) / 5)
        // But with left-associativity: ((1 + 2) - ((3 * 4) / 5))
        assert!(matches!(expr, Expr::BinOp { op: BinOp::Sub, .. }));
    }

    #[test]
    fn test_parse_nested_parentheses() {
        let expr = parse_str("((1 + 2))").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_complex_expression() {
        let expr = parse_str("2 + 3 * (4 - 1)").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Int(2)),
                right: Box::new(Expr::BinOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Int(3)),
                    right: Box::new(Expr::BinOp {
                        op: BinOp::Sub,
                        left: Box::new(Expr::Int(4)),
                        right: Box::new(Expr::Int(1)),
                    }),
                }),
            }
        );
    }

    #[test]
    fn test_parse_unary_minus() {
        let expr = parse_str("-42").unwrap();
        assert_eq!(
            expr,
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(Expr::Int(42)),
            }
        );
    }

    #[test]
    fn test_parse_double_negation() {
        let expr = parse_str("--42").unwrap();
        assert_eq!(
            expr,
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(Expr::Int(42)),
                }),
            }
        );
    }

    #[test]
    fn test_parse_subtract_negative() {
        let expr = parse_str("5 - -3").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Sub,
                left: Box::new(Expr::Int(5)),
                right: Box::new(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(Expr::Int(3)),
                }),
            }
        );
    }

    #[test]
    fn test_parse_negate_parentheses() {
        let expr = parse_str("-(2 + 3)").unwrap();
        assert_eq!(
            expr,
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Int(2)),
                    right: Box::new(Expr::Int(3)),
                }),
            }
        );
    }

    #[test]
    fn test_parse_int64() {
        let expr = parse_str("42n").unwrap();
        assert_eq!(expr, Expr::BigInt(42));
    }

    #[test]
    fn test_parse_int64_large() {
        let expr = parse_str("9_000_000_000n").unwrap();
        assert_eq!(expr, Expr::BigInt(9_000_000_000));
    }

    #[test]
    fn test_parse_int64_addition() {
        let expr = parse_str("1n + 2n").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::BigInt(1)),
                right: Box::new(Expr::BigInt(2)),
            }
        );
    }

    #[test]
    fn test_parse_float() {
        let expr = parse_str("3.15").unwrap();
        assert_eq!(expr, Expr::Float(3.15));
    }

    #[test]
    fn test_parse_float_addition() {
        let expr = parse_str("1.5 + 2.5").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Float(1.5)),
                right: Box::new(Expr::Float(2.5)),
            }
        );
    }

    #[test]
    fn test_parse_negate_float() {
        let expr = parse_str("-3.15").unwrap();
        assert_eq!(
            expr,
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(Expr::Float(3.15)),
            }
        );
    }

    #[test]
    fn test_parse_variable() {
        let expr = parse_str("x").unwrap();
        assert_eq!(expr, Expr::Path(Path::simple("x".to_string())));
    }

    #[test]
    fn test_parse_variable_in_expression() {
        let expr = parse_str("x + y").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                right: Box::new(Expr::Path(Path::simple("y".to_string()))),
            }
        );
    }

    #[test]
    fn test_parse_function_call_no_args() {
        let expr = parse_str("foo()").unwrap();
        assert_eq!(
            expr,
            Expr::Call {
                path: Path::simple("foo".to_string()),
                args: vec![],
            }
        );
    }

    #[test]
    fn test_parse_function_call_one_arg() {
        let expr = parse_str("square(5)").unwrap();
        assert_eq!(
            expr,
            Expr::Call {
                path: Path::simple("square".to_string()),
                args: vec![Expr::Int(5)],
            }
        );
    }

    #[test]
    fn test_parse_function_call_multiple_args() {
        let expr = parse_str("add(1, 2)").unwrap();
        assert_eq!(
            expr,
            Expr::Call {
                path: Path::simple("add".to_string()),
                args: vec![Expr::Int(1), Expr::Int(2)],
            }
        );
    }

    #[test]
    fn test_parse_function_call_with_expression_args() {
        let expr = parse_str("add(1 + 2, x * 3)").unwrap();
        assert_eq!(
            expr,
            Expr::Call {
                path: Path::simple("add".to_string()),
                args: vec![
                    Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Int(1)),
                        right: Box::new(Expr::Int(2)),
                    },
                    Expr::BinOp {
                        op: BinOp::Mul,
                        left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                        right: Box::new(Expr::Int(3)),
                    },
                ],
            }
        );
    }

    #[test]
    fn test_parse_nested_call() {
        let expr = parse_str("foo(bar(1))").unwrap();
        assert_eq!(
            expr,
            Expr::Call {
                path: Path::simple("foo".to_string()),
                args: vec![Expr::Call {
                    path: Path::simple("bar".to_string()),
                    args: vec![Expr::Int(1)],
                }],
            }
        );
    }

    #[test]
    fn test_parse_call_in_expression() {
        let expr = parse_str("1 + square(2)").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Call {
                    path: Path::simple("square".to_string()),
                    args: vec![Expr::Int(2)],
                }),
            }
        );
    }

    use zoya_ast::{FunctionDef, Item, Param, Pattern, TypeAnnotation, Visibility};

    fn parse_item_str(input: &str) -> Result<Item, ParseError> {
        let tokens = lex(input).expect("lexing failed");
        parse_item(tokens)
    }

    #[test]
    fn test_parse_simple_function() {
        let item = parse_item_str("fn foo() { 42 }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "foo".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: None,
                body: Expr::Int(42),
            })
        );
    }

    #[test]
    fn test_parse_function_with_return_type() {
        let item = parse_item_str("fn foo() -> Int { 42 }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "foo".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Int(42),
            })
        );
    }

    #[test]
    fn test_parse_function_with_params() {
        let item = parse_item_str("fn add(x: Int, y: Int) -> Int { x + y }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        pattern: Pattern::Path(Path::simple("x".to_string())),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                    Param {
                        pattern: Pattern::Path(Path::simple("y".to_string())),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                ],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                    right: Box::new(Expr::Path(Path::simple("y".to_string()))),
                },
            })
        );
    }

    #[test]
    fn test_parse_generic_function() {
        let item = parse_item_str("fn identity<T>(x: T) -> T { x }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "identity".to_string(),
                type_params: vec!["T".to_string()],
                params: vec![Param {
                    pattern: Pattern::Path(Path::simple("x".to_string())),
                    typ: TypeAnnotation::Named(Path::simple("T".to_string())),
                }],
                return_type: Some(TypeAnnotation::Named(Path::simple("T".to_string()))),
                body: Expr::Path(Path::simple("x".to_string())),
            })
        );
    }

    #[test]
    fn test_parse_function_multiple_type_params() {
        let item = parse_item_str("fn pair<A, B>(a: A, b: B) { a }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "pair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                params: vec![
                    Param {
                        pattern: Pattern::Path(Path::simple("a".to_string())),
                        typ: TypeAnnotation::Named(Path::simple("A".to_string())),
                    },
                    Param {
                        pattern: Pattern::Path(Path::simple("b".to_string())),
                        typ: TypeAnnotation::Named(Path::simple("B".to_string())),
                    },
                ],
                return_type: None,
                body: Expr::Path(Path::simple("a".to_string())),
            })
        );
    }

    #[test]
    fn test_parse_function_with_call_body() {
        let item = parse_item_str("fn double(x: Int) -> Int { add(x, x) }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "double".to_string(),
                type_params: vec![],
                params: vec![Param {
                    pattern: Pattern::Path(Path::simple("x".to_string())),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                }],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Call {
                    path: Path::simple("add".to_string()),
                    args: vec![
                        Expr::Path(Path::simple("x".to_string())),
                        Expr::Path(Path::simple("x".to_string())),
                    ],
                },
            })
        );
    }

    use zoya_ast::TuplePattern;

    #[test]
    fn test_parse_function_tuple_param() {
        let item = parse_item_str("fn swap((a, b): (Int, Int)) -> (Int, Int) (b, a)").unwrap();
        let Item::Function(func) = item else {
            panic!("expected function")
        };
        assert_eq!(func.name, "swap");
        assert_eq!(func.params.len(), 1);
        assert!(matches!(
            &func.params[0].pattern,
            Pattern::Tuple(TuplePattern::Exact(patterns))
            if patterns.len() == 2
        ));
    }

    #[test]
    fn test_parse_lambda_tuple_param() {
        let expr = parse_str("|(a, b)| a + b").unwrap();
        let Expr::Lambda { params, .. } = expr else {
            panic!("expected lambda")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(
            &params[0].pattern,
            Pattern::Tuple(TuplePattern::Exact(patterns))
            if patterns.len() == 2
        ));
    }

    #[test]
    fn test_parse_bool_true() {
        let expr = parse_str("true").unwrap();
        assert_eq!(expr, Expr::Bool(true));
    }

    #[test]
    fn test_parse_bool_false() {
        let expr = parse_str("false").unwrap();
        assert_eq!(expr, Expr::Bool(false));
    }

    #[test]
    fn test_parse_string() {
        let expr = parse_str(r#""hello""#).unwrap();
        assert_eq!(expr, Expr::String("hello".to_string()));
    }

    #[test]
    fn test_parse_string_equality() {
        let expr = parse_str(r#""hello" == "world""#).unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Eq,
                left: Box::new(Expr::String("hello".to_string())),
                right: Box::new(Expr::String("world".to_string())),
            }
        );
    }

    #[test]
    fn test_parse_equality() {
        let expr = parse_str("1 == 2").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Eq,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_inequality() {
        let expr = parse_str("1 != 2").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Ne,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_less_than() {
        let expr = parse_str("1 < 2").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Lt,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_greater_than() {
        let expr = parse_str("1 > 2").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Gt,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_less_equal() {
        let expr = parse_str("1 <= 2").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Le,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_greater_equal() {
        let expr = parse_str("1 >= 2").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Ge,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Int(2)),
            }
        );
    }

    #[test]
    fn test_parse_comparison_precedence() {
        // Arithmetic has higher precedence than comparison
        let expr = parse_str("1 + 2 == 3").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Eq,
                left: Box::new(Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Int(1)),
                    right: Box::new(Expr::Int(2)),
                }),
                right: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn test_parse_chained_comparison() {
        // Left associative: 1 < 2 < 3 parses as (1 < 2) < 3
        let expr = parse_str("1 < 2 < 3").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Lt,
                left: Box::new(Expr::BinOp {
                    op: BinOp::Lt,
                    left: Box::new(Expr::Int(1)),
                    right: Box::new(Expr::Int(2)),
                }),
                right: Box::new(Expr::Int(3)),
            }
        );
    }

    use zoya_ast::Stmt;

    fn parse_input_str(input: &str) -> Result<(Vec<Item>, Vec<Stmt>), ParseError> {
        let tokens = lex(input).expect("lexing failed");
        parse_input(tokens)
    }

    #[test]
    fn test_parse_input_single_expr() {
        let (items, stmts) = parse_input_str("1 + 2").unwrap();
        assert!(items.is_empty());
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Stmt::Expr(_)));
    }

    #[test]
    fn test_parse_input_single_function() {
        let (items, stmts) = parse_input_str("fn foo() -> Int { 42 }").unwrap();
        assert_eq!(items.len(), 1);
        assert!(stmts.is_empty());
        assert!(matches!(items[0], Item::Function(_)));
    }

    #[test]
    fn test_parse_input_function_then_expr() {
        let (items, stmts) = parse_input_str("fn foo() -> Int { 42 } foo()").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(stmts.len(), 1);
        assert!(matches!(items[0], Item::Function(_)));
        assert!(matches!(stmts[0], Stmt::Expr(_)));
    }

    #[test]
    fn test_parse_input_multiple_exprs() {
        let (items, stmts) = parse_input_str("1 2 3").unwrap();
        assert!(items.is_empty());
        assert_eq!(stmts.len(), 3);
        assert!(matches!(stmts[0], Stmt::Expr(Expr::Int(1))));
        assert!(matches!(stmts[1], Stmt::Expr(Expr::Int(2))));
        assert!(matches!(stmts[2], Stmt::Expr(Expr::Int(3))));
    }

    use zoya_ast::LetBinding;

    #[test]
    fn test_parse_let_simple() {
        let (_, stmts) = parse_input_str("let x = 42").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            &stmts[0],
            Stmt::Let(LetBinding {
                pattern: Pattern::Path(p),
                type_annotation: None,
                value,
            }) if p.segments == ["x"] && **value == Expr::Int(42)
        ));
    }

    #[test]
    fn test_parse_let_with_type() {
        let (_, stmts) = parse_input_str("let x: Int = 42").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            &stmts[0],
            Stmt::Let(LetBinding {
                pattern: Pattern::Path(p),
                type_annotation: Some(TypeAnnotation::Named(ty)),
                value,
            }) if p.segments == ["x"] && ty.as_simple() == Some("Int") && **value == Expr::Int(42)
        ));
    }

    #[test]
    fn test_parse_let_with_expression() {
        let (_, stmts) = parse_input_str("let x = 1 + 2").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(&stmts[0], Stmt::Let(_)));
    }

    #[test]
    fn test_parse_function_with_let() {
        let item = parse_item_str("fn foo() { let x = 1; x + 1 }").unwrap();
        assert!(matches!(
            item,
            Item::Function(FunctionDef {
                body: Expr::Block { .. },
                ..
            })
        ));
    }

    #[test]
    fn test_parse_function_with_multiple_lets() {
        let item = parse_item_str("fn foo() { let x = 1; let y = 2; x + y }").unwrap();
        let Item::Function(FunctionDef {
            body: Expr::Block { bindings, result },
            ..
        }) = item
        else {
            panic!("expected function with block body")
        };
        assert_eq!(bindings.len(), 2);
        assert!(matches!(&bindings[0].pattern, Pattern::Path(p) if p.segments == ["x"]));
        assert!(matches!(&bindings[1].pattern, Pattern::Path(p) if p.segments == ["y"]));
        assert!(matches!(*result, Expr::BinOp { .. }));
    }

    #[test]
    fn test_parse_function_without_let_no_block() {
        // Without let statements, body should be a plain expression, not a block
        let item = parse_item_str("fn foo() { 42 }").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item else {
            panic!("expected function")
        };
        assert!(matches!(body, Expr::Int(42)));
    }

    #[test]
    fn test_parse_function_requires_semicolons_after_let() {
        // Semicolons are required after let bindings in function bodies
        let result = parse_item_str("fn foo() { let x = 1 let y = 2 x + y }");
        assert!(result.is_err(), "should fail without semicolons");
    }

    #[test]
    fn test_parse_function_simple_body_no_braces() {
        // Simple expression body without braces
        let item = parse_item_str("fn foo() -> Int 42").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "foo".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Int(42),
            })
        );
    }

    #[test]
    fn test_parse_function_expression_body_no_braces() {
        // Expression body without braces
        let item = parse_item_str("fn add(x: Int, y: Int) -> Int x + y").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Private,
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        pattern: Pattern::Path(Path::simple("x".to_string())),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                    Param {
                        pattern: Pattern::Path(Path::simple("y".to_string())),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                ],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Path(Path::simple("x".to_string()))),
                    right: Box::new(Expr::Path(Path::simple("y".to_string()))),
                },
            })
        );
    }

    #[test]
    fn test_parse_function_no_braces_with_method_call() {
        // Method call expression body without braces
        let item = parse_item_str("fn double(x: Int) -> Int x * 2").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item else {
            panic!("expected function")
        };
        assert!(matches!(body, Expr::BinOp { op: BinOp::Mul, .. }));
    }

    #[test]
    fn test_parse_pub_function() {
        let item = parse_item_str("pub fn foo() -> Int 42").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                attributes: vec![],
                visibility: Visibility::Public,
                name: "foo".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Int(42),
            })
        );
    }

    #[test]
    fn test_parse_pub_function_with_params() {
        let item = parse_item_str("pub fn add(x: Int, y: Int) -> Int x + y").unwrap();
        let Item::Function(func) = item else {
            panic!("expected function")
        };
        assert_eq!(func.visibility, Visibility::Public);
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
    }

    use zoya_ast::MatchArm;

    #[test]
    fn test_parse_match_with_literals() {
        let expr = parse_str("match x { 0 => 1, 1 => 2 }").unwrap();
        let Expr::Match { scrutinee, arms } = expr else {
            panic!("expected match expression")
        };
        assert!(matches!(*scrutinee, Expr::Path(ref p) if p.as_simple() == Some("x")));
        assert_eq!(arms.len(), 2);
        assert!(matches!(
            &arms[0],
            MatchArm {
                pattern: Pattern::Literal(lit),
                result: Expr::Int(1),
            } if **lit == Expr::Int(0)
        ));
        assert!(matches!(
            &arms[1],
            MatchArm {
                pattern: Pattern::Literal(lit),
                result: Expr::Int(2),
            } if **lit == Expr::Int(1)
        ));
    }

    #[test]
    fn test_parse_match_with_wildcard() {
        let expr = parse_str("match x { 0 => 1, _ => 2 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[1].pattern, Pattern::Wildcard));
    }

    #[test]
    fn test_parse_match_with_variable() {
        let expr = parse_str("match x { n => n }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 1);
        assert!(matches!(&arms[0].pattern, Pattern::Path(p) if p.segments == ["n"]));
        assert!(matches!(&arms[0].result, Expr::Path(p) if p.as_simple() == Some("n")));
    }

    #[test]
    fn test_parse_match_with_strings() {
        let expr = parse_str(r#"match s { "a" => 1, "b" => 2 }"#).unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Literal(lit) if **lit == Expr::String("a".to_string())
        ));
    }

    #[test]
    fn test_parse_match_in_function() {
        let item = parse_item_str("fn f(x: Int) -> Int { match x { 0 => 0, n => n } }").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item else {
            panic!("expected function")
        };
        assert!(matches!(body, Expr::Match { .. }));
    }

    #[test]
    fn test_parse_method_call_no_args() {
        let expr = parse_str(r#""hello".len()"#).unwrap();
        assert!(matches!(
            expr,
            Expr::MethodCall {
                receiver,
                method,
                args,
            } if matches!(*receiver, Expr::String(ref s) if s == "hello")
                && method == "len"
                && args.is_empty()
        ));
    }

    #[test]
    fn test_parse_method_call_with_arg() {
        let expr = parse_str(r#""hello".contains("ell")"#).unwrap();
        assert!(matches!(
            expr,
            Expr::MethodCall {
                receiver,
                method,
                args,
            } if matches!(*receiver, Expr::String(ref s) if s == "hello")
                && method == "contains"
                && args.len() == 1
        ));
    }

    #[test]
    fn test_parse_chained_method_calls() {
        let expr = parse_str(r#""hello".to_uppercase().len()"#).unwrap();
        // Should parse as ("hello".to_uppercase()).len()
        let Expr::MethodCall {
            receiver,
            method,
            args,
        } = expr
        else {
            panic!("expected method call")
        };
        assert_eq!(method, "len");
        assert!(args.is_empty());
        assert!(matches!(
            *receiver,
            Expr::MethodCall {
                method: ref m,
                ..
            } if m == "to_uppercase"
        ));
    }

    #[test]
    fn test_parse_method_call_on_variable() {
        let expr = parse_str("s.trim()").unwrap();
        assert!(matches!(
            expr,
            Expr::MethodCall {
                receiver,
                method,
                args,
            } if matches!(*receiver, Expr::Path(ref p) if p.as_simple() == Some("s"))
                && method == "trim"
                && args.is_empty()
        ));
    }

    #[test]
    fn test_parse_method_call_in_expression() {
        let expr = parse_str(r#""hello".len() + 1"#).unwrap();
        assert!(matches!(expr, Expr::BinOp { op: BinOp::Add, .. }));
    }

    // List literal tests
    #[test]
    fn test_parse_empty_list() {
        let expr = parse_str("[]").unwrap();
        assert_eq!(expr, Expr::List(vec![]));
    }

    #[test]
    fn test_parse_list_single_element() {
        let expr = parse_str("[1]").unwrap();
        assert_eq!(expr, Expr::List(vec![Expr::Int(1)]));
    }

    #[test]
    fn test_parse_list_multiple_elements() {
        let expr = parse_str("[1, 2, 3]").unwrap();
        assert_eq!(
            expr,
            Expr::List(vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)])
        );
    }

    #[test]
    fn test_parse_list_with_expressions() {
        let expr = parse_str("[1 + 2, x]").unwrap();
        assert!(matches!(expr, Expr::List(elems) if elems.len() == 2));
    }

    #[test]
    fn test_parse_nested_list() {
        let expr = parse_str("[[1, 2], [3]]").unwrap();
        assert!(matches!(expr, Expr::List(elems) if elems.len() == 2));
    }

    #[test]
    fn test_parse_list_trailing_comma() {
        let expr = parse_str("[1, 2,]").unwrap();
        assert_eq!(expr, Expr::List(vec![Expr::Int(1), Expr::Int(2)]));
    }

    use zoya_ast::ListPattern;

    // List pattern tests
    #[test]
    fn test_parse_match_empty_list_pattern() {
        let expr = parse_str("match xs { [] => 0 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::List(ListPattern::Empty)
        ));
    }

    #[test]
    fn test_parse_match_exact_list_pattern() {
        let expr = parse_str("match xs { [a, b] => a }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Exact(patterns)) = &arms[0].pattern else {
            panic!("expected exact list pattern")
        };
        assert_eq!(patterns.len(), 2);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["a"]));
        assert!(matches!(&patterns[1], Pattern::Path(p) if p.segments == ["b"]));
    }

    #[test]
    fn test_parse_match_prefix_list_pattern() {
        let expr = parse_str("match xs { [head, ..] => head }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Prefix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix list pattern")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["head"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_match_list_pattern_with_literals() {
        let expr = parse_str("match xs { [1, x, ..] => x }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Prefix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix list pattern")
        };
        assert_eq!(patterns.len(), 2);
        assert!(matches!(&patterns[0], Pattern::Literal(lit) if **lit == Expr::Int(1)));
        assert!(matches!(&patterns[1], Pattern::Path(p) if p.segments == ["x"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_match_list_pattern_with_wildcard() {
        let expr = parse_str("match xs { [_, x] => x }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Exact(patterns)) = &arms[0].pattern else {
            panic!("expected exact list pattern")
        };
        assert_eq!(patterns.len(), 2);
        assert!(matches!(&patterns[0], Pattern::Wildcard));
        assert!(matches!(&patterns[1], Pattern::Path(p) if p.segments == ["x"]));
    }

    #[test]
    fn test_parse_match_suffix_list_pattern() {
        let expr = parse_str("match xs { [.., last] => last }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Suffix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected suffix list pattern")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["last"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_match_suffix_list_pattern_multiple() {
        let expr = parse_str("match xs { [.., x, y] => x }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Suffix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected suffix list pattern")
        };
        assert_eq!(patterns.len(), 2);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["x"]));
        assert!(matches!(&patterns[1], Pattern::Path(p) if p.segments == ["y"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_match_prefix_suffix_list_pattern() {
        let expr = parse_str("match xs { [first, .., last] => first }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::PrefixSuffix {
            prefix,
            suffix,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix+suffix list pattern")
        };
        assert_eq!(prefix.len(), 1);
        assert_eq!(suffix.len(), 1);
        assert!(matches!(&prefix[0], Pattern::Path(p) if p.segments == ["first"]));
        assert!(matches!(&suffix[0], Pattern::Path(p) if p.segments == ["last"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_match_prefix_suffix_multiple() {
        let expr = parse_str("match xs { [a, b, .., y, z] => a }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::PrefixSuffix {
            prefix,
            suffix,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix+suffix list pattern")
        };
        assert_eq!(prefix.len(), 2);
        assert_eq!(suffix.len(), 2);
        assert!(matches!(&prefix[0], Pattern::Path(p) if p.segments == ["a"]));
        assert!(matches!(&prefix[1], Pattern::Path(p) if p.segments == ["b"]));
        assert!(matches!(&suffix[0], Pattern::Path(p) if p.segments == ["y"]));
        assert!(matches!(&suffix[1], Pattern::Path(p) if p.segments == ["z"]));
        assert!(rest_binding.is_none());
    }

    // Parameterized type annotation tests
    #[test]
    fn test_parse_function_with_list_param() {
        let item = parse_item_str("fn len(xs: List<Int>) -> Int { 0 }").unwrap();
        let Item::Function(FunctionDef { params, .. }) = item else {
            panic!("expected function")
        };
        assert!(matches!(
            &params[0].typ,
            TypeAnnotation::Parameterized(name, args)
                if name.as_simple() == Some("List") && args.len() == 1
        ));
    }

    // Tuple tests
    #[test]
    fn test_parse_empty_tuple() {
        let expr = parse_str("()").unwrap();
        assert_eq!(expr, Expr::Tuple(vec![]));
    }

    #[test]
    fn test_parse_single_element_tuple() {
        let expr = parse_str("(42,)").unwrap();
        assert_eq!(expr, Expr::Tuple(vec![Expr::Int(42)]));
    }

    #[test]
    fn test_parse_tuple_literal() {
        let expr = parse_str("(1, \"hello\", true)").unwrap();
        assert_eq!(
            expr,
            Expr::Tuple(vec![
                Expr::Int(1),
                Expr::String("hello".to_string()),
                Expr::Bool(true)
            ])
        );
    }

    #[test]
    fn test_parse_parenthesized_expr_not_tuple() {
        let expr = parse_str("(1 + 2)").unwrap();
        // Should be a BinOp, not a tuple
        assert!(matches!(expr, Expr::BinOp { .. }));
    }

    #[test]
    fn test_parse_tuple_pattern_exact() {
        let expr = parse_str("match t { (a, b) => a }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Tuple(TuplePattern::Exact(patterns)) = &arms[0].pattern else {
            panic!("expected exact tuple pattern")
        };
        assert_eq!(patterns.len(), 2);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["a"]));
        assert!(matches!(&patterns[1], Pattern::Path(p) if p.segments == ["b"]));
    }

    #[test]
    fn test_parse_tuple_pattern_prefix() {
        let expr = parse_str("match t { (a, ..) => a }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Tuple(TuplePattern::Prefix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix tuple pattern")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["a"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_tuple_pattern_suffix() {
        let expr = parse_str("match t { (.., z) => z }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Tuple(TuplePattern::Suffix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected suffix tuple pattern")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["z"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_tuple_pattern_prefix_suffix() {
        let expr = parse_str("match t { (a, .., z) => a + z }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Tuple(TuplePattern::PrefixSuffix {
            prefix,
            suffix,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix+suffix tuple pattern")
        };
        assert_eq!(prefix.len(), 1);
        assert_eq!(suffix.len(), 1);
        assert!(matches!(&prefix[0], Pattern::Path(p) if p.segments == ["a"]));
        assert!(matches!(&suffix[0], Pattern::Path(p) if p.segments == ["z"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_tuple_pattern_empty() {
        let expr = parse_str("match t { () => 0 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Tuple(TuplePattern::Empty)
        ));
    }

    // As pattern (@) tests
    #[test]
    fn test_parse_as_pattern() {
        let expr = parse_str("match x { n @ 42 => n }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::As { name, pattern } = &arms[0].pattern else {
            panic!("expected as pattern")
        };
        assert_eq!(name, "n");
        assert!(matches!(pattern.as_ref(), Pattern::Literal(lit) if **lit == Expr::Int(42)));
    }

    #[test]
    fn test_parse_list_rest_binding() {
        let expr = parse_str("match xs { [first, rest @ ..] => rest }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Prefix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix list pattern with rest binding")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["first"]));
        assert_eq!(rest_binding.as_deref(), Some("rest"));
    }

    #[test]
    fn test_parse_list_rest_binding_suffix() {
        let expr = parse_str("match xs { [rest @ .., last] => rest }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::List(ListPattern::Suffix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected suffix list pattern with rest binding")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["last"]));
        assert_eq!(rest_binding.as_deref(), Some("rest"));
    }

    #[test]
    fn test_parse_tuple_rest_binding() {
        let expr = parse_str("match t { (a, rest @ ..) => rest }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Tuple(TuplePattern::Prefix {
            patterns,
            rest_binding,
        }) = &arms[0].pattern
        else {
            panic!("expected prefix tuple pattern with rest binding")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["a"]));
        assert_eq!(rest_binding.as_deref(), Some("rest"));
    }

    // Match arm block expression tests
    #[test]
    fn test_parse_match_with_commas() {
        let expr = parse_str("match x { 0 => 1, 1 => 2 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(&arms[0].result, Expr::Int(1)));
        assert!(matches!(&arms[1].result, Expr::Int(2)));
    }

    #[test]
    fn test_parse_match_with_trailing_comma() {
        let expr = parse_str("match x { 0 => 1, _ => 2, }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn test_parse_match_braced_simple() {
        let expr = parse_str("match x { 0 => { 1 }, _ => { 2 } }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 2);
        // Braced simple expressions should unwrap to just the expression
        assert!(matches!(&arms[0].result, Expr::Int(1)));
        assert!(matches!(&arms[1].result, Expr::Int(2)));
    }

    #[test]
    fn test_parse_match_braced_block() {
        let expr = parse_str("match x { 0 => { let y = 1; y + 1 }, _ => 0 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 2);
        let Expr::Block { bindings, result } = &arms[0].result else {
            panic!("expected block expression in first arm")
        };
        assert_eq!(bindings.len(), 1);
        assert!(matches!(&bindings[0].pattern, Pattern::Path(p) if p.segments == ["y"]));
        assert!(matches!(**result, Expr::BinOp { .. }));
        assert!(matches!(&arms[1].result, Expr::Int(0)));
    }

    #[test]
    fn test_parse_match_mixed() {
        // Mix of braced and non-braced arms with commas
        let expr = parse_str("match x { 0 => 1, 1 => { 2 }, _ => { let z = 3; z } }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        assert_eq!(arms.len(), 3);
        assert!(matches!(&arms[0].result, Expr::Int(1)));
        assert!(matches!(&arms[1].result, Expr::Int(2)));
        assert!(matches!(&arms[2].result, Expr::Block { .. }));
    }

    #[test]
    fn test_parse_match_braced_block_with_semicolons() {
        let expr = parse_str("match x { n => { let a = n; let b = a * 2; a + b } }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Expr::Block { bindings, .. } = &arms[0].result else {
            panic!("expected block expression")
        };
        assert_eq!(bindings.len(), 2);
        assert!(matches!(&bindings[0].pattern, Pattern::Path(p) if p.segments == ["a"]));
        assert!(matches!(&bindings[1].pattern, Pattern::Path(p) if p.segments == ["b"]));
    }

    #[test]
    fn test_parse_match_braced_with_pattern_binding() {
        // Pattern binding should be usable in the block
        let expr = parse_str("match x { n => { let doubled = n * 2; doubled + 1 } }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Expr::Block { bindings, result } = &arms[0].result else {
            panic!("expected block expression")
        };
        assert_eq!(bindings.len(), 1);
        assert!(matches!(&bindings[0].pattern, Pattern::Path(p) if p.segments == ["doubled"]));
        // The binding value should reference 'n' from the pattern
        let Expr::BinOp { left, .. } = &*bindings[0].value else {
            panic!("expected binop in binding value")
        };
        assert!(matches!(**left, Expr::Path(ref p) if p.as_simple() == Some("n")));
        // The result should reference 'doubled'
        let Expr::BinOp { left, .. } = &**result else {
            panic!("expected binop in result")
        };
        assert!(matches!(**left, Expr::Path(ref p) if p.as_simple() == Some("doubled")));
    }

    // Lambda tests
    #[test]
    fn test_parse_simple_lambda() {
        let expr = parse_str("|x| x + 1").unwrap();
        let Expr::Lambda {
            params,
            return_type,
            body,
        } = expr
        else {
            panic!("expected lambda")
        };
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0].pattern,
            Pattern::Path(Path::simple("x".to_string()))
        );
        assert!(params[0].typ.is_none());
        assert!(return_type.is_none());
        assert!(matches!(*body, Expr::BinOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn test_parse_lambda_multi_param() {
        let expr = parse_str("|x, y| x + y").unwrap();
        let Expr::Lambda { params, .. } = expr else {
            panic!("expected lambda")
        };
        assert_eq!(params.len(), 2);
        assert_eq!(
            params[0].pattern,
            Pattern::Path(Path::simple("x".to_string()))
        );
        assert_eq!(
            params[1].pattern,
            Pattern::Path(Path::simple("y".to_string()))
        );
    }

    #[test]
    fn test_parse_lambda_with_type_annotation() {
        let expr = parse_str("|x: Int| x * 2").unwrap();
        let Expr::Lambda { params, .. } = expr else {
            panic!("expected lambda")
        };
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0].pattern,
            Pattern::Path(Path::simple("x".to_string()))
        );
        assert!(matches!(
            &params[0].typ,
            Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
        ));
    }

    #[test]
    fn test_parse_lambda_with_return_type() {
        let expr = parse_str("|x| -> Int x + 1").unwrap();
        let Expr::Lambda {
            params,
            return_type,
            ..
        } = expr
        else {
            panic!("expected lambda")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(
            return_type,
            Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
        ));
    }

    #[test]
    fn test_parse_lambda_fully_annotated() {
        let expr = parse_str("|x: Int| -> Int x * 2").unwrap();
        let Expr::Lambda {
            params,
            return_type,
            ..
        } = expr
        else {
            panic!("expected lambda")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(
            &params[0].typ,
            Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
        ));
        assert!(matches!(
            return_type,
            Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
        ));
    }

    #[test]
    fn test_parse_lambda_block_body() {
        let expr = parse_str("|x| { let y = x * 2; y + 1 }").unwrap();
        let Expr::Lambda { body, .. } = expr else {
            panic!("expected lambda")
        };
        assert!(matches!(*body, Expr::Block { .. }));
    }

    #[test]
    fn test_parse_lambda_no_params() {
        let expr = parse_str("|| 42").unwrap();
        let Expr::Lambda { params, body, .. } = expr else {
            panic!("expected lambda")
        };
        assert!(params.is_empty());
        assert!(matches!(*body, Expr::Int(42)));
    }

    #[test]
    fn test_parse_lambda_in_expression() {
        // Lambda as function argument (conceptually - requires let binding to use)
        let (_, stmts) = parse_input_str("let f = |x| x + 1").unwrap();
        let Stmt::Let(binding) = &stmts[0] else {
            panic!("expected let statement")
        };
        assert!(matches!(&binding.pattern, Pattern::Path(p) if p.segments == ["f"]));
        assert!(matches!(*binding.value, Expr::Lambda { .. }));
    }

    #[test]
    fn test_parse_lambda_nested() {
        let expr = parse_str("|x| |y| x + y").unwrap();
        let Expr::Lambda { body, .. } = expr else {
            panic!("expected lambda")
        };
        assert!(matches!(*body, Expr::Lambda { .. }));
    }

    #[test]
    fn test_parse_function_type_simple() {
        // let f: Int -> Int = ...
        let (_, stmts) = parse_input_str("let f: Int -> Int = |x| x + 1").unwrap();
        let Stmt::Let(binding) = &stmts[0] else {
            panic!("expected let statement")
        };
        assert!(matches!(
            &binding.type_annotation,
            Some(TypeAnnotation::Function(params, ret))
            if params.len() == 1
                && matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
                && matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
        ));
    }

    #[test]
    fn test_parse_function_type_multi_param() {
        // let f: (Int, String) -> Bool = ...
        let (_, stmts) = parse_input_str("let f: (Int, String) -> Bool = |x, y| true").unwrap();
        let Stmt::Let(binding) = &stmts[0] else {
            panic!("expected let statement")
        };
        let Some(TypeAnnotation::Function(params, ret)) = &binding.type_annotation else {
            panic!("expected function type annotation")
        };
        assert_eq!(params.len(), 2);
        assert!(matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
        assert!(matches!(&params[1], TypeAnnotation::Named(n) if n.as_simple() == Some("String")));
        assert!(matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Bool")));
    }

    #[test]
    fn test_parse_function_type_no_params() {
        // let f: () -> Int = ...
        let (_, stmts) = parse_input_str("let f: () -> Int = || 42").unwrap();
        let Stmt::Let(binding) = &stmts[0] else {
            panic!("expected let statement")
        };
        let Some(TypeAnnotation::Function(params, ret)) = &binding.type_annotation else {
            panic!("expected function type annotation")
        };
        assert!(params.is_empty());
        assert!(matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
    }

    #[test]
    fn test_parse_function_type_nested() {
        // let f: Int -> Int -> Int = |x| |y| x + y
        // Should be: Int -> (Int -> Int) (right associative)
        let (_, stmts) = parse_input_str("let f: Int -> Int -> Int = |x| |y| x + y").unwrap();
        let Stmt::Let(binding) = &stmts[0] else {
            panic!("expected let statement")
        };
        let Some(TypeAnnotation::Function(params, ret)) = &binding.type_annotation else {
            panic!("expected function type annotation")
        };
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
        // ret should be Int -> Int
        let TypeAnnotation::Function(inner_params, inner_ret) = ret.as_ref() else {
            panic!("expected nested function type")
        };
        assert_eq!(inner_params.len(), 1);
        assert!(
            matches!(&inner_params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
        );
        assert!(
            matches!(inner_ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
        );
    }

    #[test]
    fn test_parse_function_param_with_function_type() {
        // fn apply(f: Int -> Int, x: Int) -> Int f(x)
        let item = parse_item_str("fn apply(f: Int -> Int, x: Int) -> Int f(x)").unwrap();
        let Item::Function(func) = &item else {
            panic!("expected function")
        };
        assert_eq!(func.name, "apply");
        assert_eq!(func.params.len(), 2);
        assert!(matches!(
            &func.params[0].typ,
            TypeAnnotation::Function(params, ret)
            if params.len() == 1
                && matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
                && matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
        ));
    }

    // Struct tests
    #[test]
    fn test_parse_struct_simple() {
        let item = parse_item_str("struct Point { x: Int, y: Int }").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Point");
        assert_eq!(s.type_params, Vec::<String>::new());
        let zoya_ast::StructKind::Named(fields) = &s.kind else {
            panic!("expected named struct")
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[1].name, "y");
    }

    #[test]
    fn test_parse_struct_empty() {
        let item = parse_item_str("struct Empty {}").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Empty");
        let zoya_ast::StructKind::Named(fields) = &s.kind else {
            panic!("expected named struct")
        };
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn test_parse_struct_unit_no_braces() {
        let item = parse_item_str("struct Empty").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Empty");
        assert_eq!(s.type_params, Vec::<String>::new());
        assert!(matches!(s.kind, zoya_ast::StructKind::Unit));
    }

    #[test]
    fn test_parse_struct_generic() {
        let item = parse_item_str("struct Pair<T, U> { first: T, second: U }").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Pair");
        assert_eq!(s.type_params, vec!["T", "U"]);
        let zoya_ast::StructKind::Named(fields) = &s.kind else {
            panic!("expected named struct")
        };
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_parse_tuple_struct() {
        let item = parse_item_str("struct Wrapper(Int)").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Wrapper");
        let zoya_ast::StructKind::Tuple(types) = &s.kind else {
            panic!("expected tuple struct")
        };
        assert_eq!(types.len(), 1);
    }

    #[test]
    fn test_parse_tuple_struct_multiple_fields() {
        let item = parse_item_str("struct Pair(String, Int)").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Pair");
        let zoya_ast::StructKind::Tuple(types) = &s.kind else {
            panic!("expected tuple struct")
        };
        assert_eq!(types.len(), 2);
    }

    #[test]
    fn test_parse_tuple_struct_generic() {
        let item = parse_item_str("struct Box<T>(T)").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Box");
        assert_eq!(s.type_params, vec!["T"]);
        let zoya_ast::StructKind::Tuple(types) = &s.kind else {
            panic!("expected tuple struct")
        };
        assert_eq!(types.len(), 1);
    }

    #[test]
    fn test_parse_tuple_struct_trailing_comma() {
        let item = parse_item_str("struct Pair(Int, String,)").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.name, "Pair");
        let zoya_ast::StructKind::Tuple(types) = &s.kind else {
            panic!("expected tuple struct")
        };
        assert_eq!(types.len(), 2);
    }

    #[test]
    fn test_parse_struct_construct() {
        let expr = parse_str("Point { x: 1, y: 2 }").unwrap();
        assert!(matches!(
            expr,
            Expr::Struct { path, fields }
            if path.as_simple() == Some("Point") && fields.len() == 2
        ));
    }

    #[test]
    fn test_parse_struct_construct_shorthand() {
        let expr = parse_str("Point { x, y }").unwrap();
        let Expr::Struct { path, fields } = expr else {
            panic!("expected struct construct")
        };
        assert_eq!(path.as_simple(), Some("Point"));
        assert_eq!(fields.len(), 2);
        // Shorthand: x means x: x
        assert_eq!(fields[0].0, "x");
        assert!(matches!(&fields[0].1, Expr::Path(p) if p.as_simple() == Some("x")));
    }

    #[test]
    fn test_parse_struct_construct_empty() {
        let expr = parse_str("Empty {}").unwrap();
        assert!(matches!(
            expr,
            Expr::Struct { path, fields }
            if path.as_simple() == Some("Empty") && fields.is_empty()
        ));
    }

    #[test]
    fn test_parse_field_access() {
        let expr = parse_str("p.x").unwrap();
        assert!(matches!(
            expr,
            Expr::FieldAccess { field, .. }
            if field == "x"
        ));
    }

    #[test]
    fn test_parse_chained_field_access() {
        let expr = parse_str("a.b.c").unwrap();
        let Expr::FieldAccess { expr: inner, field } = expr else {
            panic!("expected field access")
        };
        assert_eq!(field, "c");
        assert!(matches!(
            *inner,
            Expr::FieldAccess { field: f, .. }
            if f == "b"
        ));
    }

    #[test]
    fn test_parse_struct_pattern_exact() {
        let expr = parse_str("match p { Point { x, y } => x + y }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match")
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Struct { path, fields, is_partial: false }
            if path.as_simple() == Some("Point") && fields.len() == 2
        ));
    }

    #[test]
    fn test_parse_struct_pattern_partial() {
        let expr = parse_str("match p { Point { x, .. } => x }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match")
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Struct { path, fields, is_partial: true }
            if path.as_simple() == Some("Point") && fields.len() == 1
        ));
    }

    #[test]
    fn test_parse_struct_pattern_with_binding() {
        let expr = parse_str("match p { Point { x: a, y: b } => a }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match")
        };
        let Pattern::Struct {
            path,
            fields,
            is_partial: false,
        } = &arms[0].pattern
        else {
            panic!("expected exact struct pattern")
        };
        assert_eq!(path.as_simple(), Some("Point"));
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_name, "x");
        assert!(matches!(&*fields[0].pattern, Pattern::Path(p) if p.segments == ["a"]));
    }

    // ===== Let Pattern Destructuring Parser Tests =====

    #[test]
    fn test_parse_let_tuple_pattern() {
        let (_, stmts) = parse_input_str("let (a, b) = x").unwrap();
        assert!(matches!(
            &stmts[0],
            Stmt::Let(LetBinding {
                pattern: Pattern::Tuple(TuplePattern::Exact(patterns)),
                ..
            }) if patterns.len() == 2
        ));
    }

    #[test]
    fn test_parse_let_struct_pattern() {
        let (_, stmts) = parse_input_str("let Point { x, y } = p").unwrap();
        let Stmt::Let(LetBinding {
            pattern:
                Pattern::Struct {
                    fields,
                    is_partial: false,
                    ..
                },
            ..
        }) = &stmts[0]
        else {
            panic!("expected struct pattern")
        };
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_parse_let_wildcard_pattern() {
        let (_, stmts) = parse_input_str("let _ = expr").unwrap();
        assert!(matches!(
            &stmts[0],
            Stmt::Let(LetBinding {
                pattern: Pattern::Wildcard,
                ..
            })
        ));
    }

    #[test]
    fn test_parse_let_nested_pattern() {
        let (_, stmts) = parse_input_str("let (a, (b, c)) = x").unwrap();
        let Stmt::Let(LetBinding {
            pattern: Pattern::Tuple(TuplePattern::Exact(outer)),
            ..
        }) = &stmts[0]
        else {
            panic!("expected nested tuple pattern")
        };
        assert_eq!(outer.len(), 2);
        assert!(matches!(&outer[0], Pattern::Path(_)));
        assert!(matches!(&outer[1], Pattern::Tuple(_)));
    }

    #[test]
    fn test_parse_let_tuple_rest_pattern() {
        let (_, stmts) = parse_input_str("let (first, ..) = tuple").unwrap();
        assert!(matches!(
            &stmts[0],
            Stmt::Let(LetBinding {
                pattern: Pattern::Tuple(TuplePattern::Prefix { .. }),
                ..
            })
        ));
    }

    #[test]
    fn test_parse_let_type_annotation_on_var_only() {
        // Type annotation on simple var pattern - should succeed
        let result = parse_input_str("let x: Int = 42");
        assert!(result.is_ok());

        // Type annotation on tuple pattern - should fail
        let result = parse_input_str("let (a, b): (Int, Int) = x");
        assert!(result.is_err());
    }

    use zoya_ast::TypeAliasDef;

    #[test]
    fn test_parse_type_alias_simple() {
        let tokens = lex("type UserId = Int").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::TypeAlias(TypeAliasDef {
                visibility: Visibility::Private,
                name,
                type_params,
                typ: TypeAnnotation::Named(_),
                ..
            }) if name == "UserId" && type_params.is_empty()
        ));
    }

    #[test]
    fn test_parse_type_alias_generic() {
        let tokens = lex("type Pair<A, B> = (A, B)").unwrap();
        let item = parse_item(tokens).unwrap();
        let Item::TypeAlias(TypeAliasDef {
            visibility: _,
            name,
            type_params,
            typ: TypeAnnotation::Tuple(elems),
            ..
        }) = item
        else {
            panic!("expected generic type alias with tuple")
        };
        assert_eq!(name, "Pair");
        assert_eq!(type_params, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(elems.len(), 2);
    }

    #[test]
    fn test_parse_type_alias_parameterized() {
        let tokens = lex("type StringList = List<String>").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::TypeAlias(TypeAliasDef {
                visibility: Visibility::Private,
                name,
                type_params,
                typ: TypeAnnotation::Parameterized(_, _),
                ..
            }) if name == "StringList" && type_params.is_empty()
        ));
    }

    #[test]
    fn test_parse_type_alias_function() {
        let tokens = lex("type Callback = (Int) -> Bool").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::TypeAlias(TypeAliasDef {
                visibility: Visibility::Private,
                name,
                type_params,
                typ: TypeAnnotation::Function(_, _),
                ..
            }) if name == "Callback" && type_params.is_empty()
        ));
    }

    // ===== pub struct/enum/type visibility parsing =====

    use zoya_ast::{EnumDef, StructDef};

    #[test]
    fn test_parse_pub_struct() {
        let tokens = lex("pub struct Point { x: Int, y: Int }").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::Struct(StructDef {
                visibility: Visibility::Public,
                name,
                ..
            }) if name == "Point"
        ));
    }

    #[test]
    fn test_parse_private_struct() {
        let tokens = lex("struct Point { x: Int, y: Int }").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::Struct(StructDef {
                visibility: Visibility::Private,
                name,
                ..
            }) if name == "Point"
        ));
    }

    #[test]
    fn test_parse_pub_enum() {
        let tokens = lex("pub enum Color { Red, Blue }").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::Enum(EnumDef {
                visibility: Visibility::Public,
                name,
                ..
            }) if name == "Color"
        ));
    }

    #[test]
    fn test_parse_private_enum() {
        let tokens = lex("enum Color { Red, Blue }").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::Enum(EnumDef {
                visibility: Visibility::Private,
                name,
                ..
            }) if name == "Color"
        ));
    }

    #[test]
    fn test_parse_pub_type_alias() {
        let tokens = lex("pub type UserId = Int").unwrap();
        let item = parse_item(tokens).unwrap();
        assert!(matches!(
            item,
            Item::TypeAlias(TypeAliasDef {
                visibility: Visibility::Public,
                name,
                ..
            }) if name == "UserId"
        ));
    }

    // ===== annotation tests =====

    #[test]
    fn test_parse_annotation_on_function() {
        let item = parse_item_str("#[test] fn foo() 42").unwrap();
        let Item::Function(f) = item else {
            panic!("expected function")
        };
        assert_eq!(f.attributes.len(), 1);
        assert_eq!(f.attributes[0].name, "test");
        assert_eq!(f.name, "foo");
    }

    #[test]
    fn test_parse_multiple_annotations() {
        let item = parse_item_str("#[test] #[inline] pub fn foo() 42").unwrap();
        let Item::Function(f) = item else {
            panic!("expected function")
        };
        assert_eq!(f.attributes.len(), 2);
        assert_eq!(f.attributes[0].name, "test");
        assert_eq!(f.attributes[1].name, "inline");
        assert_eq!(f.visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_annotation_on_struct() {
        let item = parse_item_str("#[derive] struct Point { x: Int }").unwrap();
        let Item::Struct(s) = item else {
            panic!("expected struct")
        };
        assert_eq!(s.attributes.len(), 1);
        assert_eq!(s.attributes[0].name, "derive");
    }

    #[test]
    fn test_parse_annotation_on_enum() {
        let item = parse_item_str("#[derive] enum Color { Red, Blue }").unwrap();
        let Item::Enum(e) = item else {
            panic!("expected enum")
        };
        assert_eq!(e.attributes.len(), 1);
        assert_eq!(e.attributes[0].name, "derive");
    }

    #[test]
    fn test_parse_annotation_on_type_alias() {
        let item = parse_item_str("#[deprecated] type UserId = Int").unwrap();
        let Item::TypeAlias(t) = item else {
            panic!("expected type alias")
        };
        assert_eq!(t.attributes.len(), 1);
        assert_eq!(t.attributes[0].name, "deprecated");
    }

    #[test]
    fn test_parse_no_annotations() {
        let item = parse_item_str("fn foo() 42").unwrap();
        let Item::Function(f) = item else {
            panic!("expected function")
        };
        assert!(f.attributes.is_empty());
    }

    #[test]
    fn test_parse_annotation_on_use() {
        let (_, items) = parse_module_str("#[allow] use root::foo").unwrap();
        let Item::Use(u) = &items[0] else {
            panic!("expected use")
        };
        assert_eq!(u.attributes.len(), 1);
        assert_eq!(u.attributes[0].name, "allow");
    }

    // ===== parse_file() tests =====

    fn parse_file_str(input: &str) -> Result<Vec<Item>, ParseError> {
        let tokens = lex(input).expect("lexing failed");
        parse_module(tokens).map(|(_, items)| items)
    }

    #[test]
    fn test_parse_file_single_function() {
        let items = parse_file_str("fn foo() -> Int 42").unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Item::Function(f) if f.name == "foo"));
    }

    #[test]
    fn test_parse_file_multiple_functions() {
        let items = parse_file_str("fn foo() -> Int 1 fn bar() -> Int 2").unwrap();
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Item::Function(f) if f.name == "foo"));
        assert!(matches!(&items[1], Item::Function(f) if f.name == "bar"));
    }

    #[test]
    fn test_parse_file_mixed_items() {
        let items = parse_file_str(
            "struct Point { x: Int, y: Int } \
             enum Option<T> { None, Some(T) } \
             type IntPair = (Int, Int) \
             fn make_point(x: Int) -> Point Point { x, y: x }",
        )
        .unwrap();
        assert_eq!(items.len(), 4);
        assert!(matches!(&items[0], Item::Struct(s) if s.name == "Point"));
        assert!(matches!(&items[1], Item::Enum(e) if e.name == "Option"));
        assert!(matches!(&items[2], Item::TypeAlias(t) if t.name == "IntPair"));
        assert!(matches!(&items[3], Item::Function(f) if f.name == "make_point"));
    }

    #[test]
    fn test_parse_file_empty() {
        let items = parse_file_str("").unwrap();
        assert!(items.is_empty());
    }

    // ===== Enum definition tests =====

    use zoya_ast::{EnumVariant, EnumVariantKind};

    #[test]
    fn test_parse_enum_unit_variants() {
        let item = parse_item_str("enum Color { Red, Green, Blue }").unwrap();
        let Item::Enum(e) = item else {
            panic!("expected enum");
        };
        assert_eq!(e.name, "Color");
        assert_eq!(e.variants.len(), 3);
        assert!(
            matches!(&e.variants[0], EnumVariant { name, kind: EnumVariantKind::Unit } if name == "Red")
        );
        assert!(
            matches!(&e.variants[1], EnumVariant { name, kind: EnumVariantKind::Unit } if name == "Green")
        );
        assert!(
            matches!(&e.variants[2], EnumVariant { name, kind: EnumVariantKind::Unit } if name == "Blue")
        );
    }

    #[test]
    fn test_parse_enum_tuple_variant() {
        let item = parse_item_str("enum Option<T> { None, Some(T) }").unwrap();
        let Item::Enum(e) = item else {
            panic!("expected enum")
        };
        assert_eq!(e.name, "Option");
        assert_eq!(e.type_params, vec!["T"]);
        assert_eq!(e.variants.len(), 2);
        assert!(
            matches!(&e.variants[0], EnumVariant { name, kind: EnumVariantKind::Unit } if name == "None")
        );
        let EnumVariant {
            name,
            kind: EnumVariantKind::Tuple(types),
        } = &e.variants[1]
        else {
            panic!("expected tuple variant")
        };
        assert_eq!(name, "Some");
        assert_eq!(types.len(), 1);
    }

    #[test]
    fn test_parse_enum_struct_variant() {
        let item = parse_item_str("enum Message { Move { x: Int, y: Int } }").unwrap();
        let Item::Enum(e) = item else {
            panic!("expected enum")
        };
        assert_eq!(e.name, "Message");
        let EnumVariant {
            name,
            kind: EnumVariantKind::Struct(fields),
        } = &e.variants[0]
        else {
            panic!("expected struct variant")
        };
        assert_eq!(name, "Move");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[1].name, "y");
    }

    #[test]
    fn test_parse_enum_generic() {
        let item = parse_item_str("enum Result<T, E> { Ok(T), Err(E) }").unwrap();
        let Item::Enum(e) = item else {
            panic!("expected enum");
        };
        assert_eq!(e.name, "Result");
        assert_eq!(e.type_params, vec!["T", "E"]);
        assert_eq!(e.variants.len(), 2);
    }

    #[test]
    fn test_parse_enum_mixed_variants() {
        let item =
            parse_item_str("enum Event { Click, Move { x: Int, y: Int }, KeyPress(String) }")
                .unwrap();
        let Item::Enum(e) = item else {
            panic!("expected enum");
        };
        assert_eq!(e.name, "Event");
        assert_eq!(e.variants.len(), 3);
        assert!(matches!(
            &e.variants[0],
            EnumVariant {
                kind: EnumVariantKind::Unit,
                ..
            }
        ));
        assert!(matches!(
            &e.variants[1],
            EnumVariant {
                kind: EnumVariantKind::Struct(_),
                ..
            }
        ));
        assert!(matches!(
            &e.variants[2],
            EnumVariant {
                kind: EnumVariantKind::Tuple(_),
                ..
            }
        ));
    }

    // ===== Enum pattern tests =====

    #[test]
    fn test_parse_enum_pattern_unit() {
        let expr = parse_str("match x { Option::None => 0 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Path(path) = &arms[0].pattern else {
            panic!("expected path pattern for unit enum variant")
        };
        assert_eq!(path.segments, vec!["Option", "None"]);
    }

    #[test]
    fn test_parse_enum_pattern_tuple() {
        let expr = parse_str("match x { Option::Some(v) => v }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Call { path, args } = &arms[0].pattern else {
            panic!("expected call pattern for tuple enum variant")
        };
        assert_eq!(path.segments, vec!["Option", "Some"]);
        let TuplePattern::Exact(patterns) = args else {
            panic!("expected exact tuple pattern args")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["v"]));
    }

    #[test]
    fn test_parse_enum_pattern_struct() {
        let expr = parse_str("match m { Message::Move { x, y } => x + y }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Struct {
            path,
            fields,
            is_partial,
        } = &arms[0].pattern
        else {
            panic!("expected struct pattern for enum struct variant")
        };
        assert_eq!(path.segments, vec!["Message", "Move"]);
        assert!(!is_partial);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_name, "x");
        assert_eq!(fields[1].field_name, "y");
    }

    #[test]
    fn test_parse_enum_pattern_turbofish() {
        let expr = parse_str("match x { Option::Some::<Int>(v) => v }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Call { path, args } = &arms[0].pattern else {
            panic!("expected call pattern for turbofish")
        };
        assert_eq!(path.segments, vec!["Option", "Some"]);
        assert!(path.type_args.is_some());
        let type_args = path.type_args.as_ref().unwrap();
        assert_eq!(type_args.len(), 1);
        assert!(matches!(&type_args[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
        assert!(matches!(args, TuplePattern::Exact(_)));
    }

    #[test]
    fn test_parse_enum_pattern_tuple_with_rest() {
        let expr = parse_str("match x { Triple::V(first, ..) => first }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Call { path, args } = &arms[0].pattern else {
            panic!("expected call pattern")
        };
        assert_eq!(path.segments, vec!["Triple", "V"]);
        let TuplePattern::Prefix {
            patterns,
            rest_binding,
        } = args
        else {
            panic!("expected tuple prefix pattern args")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["first"]));
        assert!(rest_binding.is_none());
    }

    #[test]
    fn test_parse_enum_pattern_struct_with_rest() {
        let expr = parse_str("match m { Message::Move { x, .. } => x }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Struct {
            path,
            fields,
            is_partial,
        } = &arms[0].pattern
        else {
            panic!("expected struct pattern")
        };
        assert_eq!(path.segments, vec!["Message", "Move"]);
        assert!(is_partial);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "x");
    }

    #[test]
    fn test_parse_enum_pattern_empty_tuple() {
        let expr = parse_str("match x { Unit::Empty() => 0 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Call { args, .. } = &arms[0].pattern else {
            panic!("expected call pattern")
        };
        assert!(matches!(args, TuplePattern::Empty));
    }

    #[test]
    fn test_parse_enum_pattern_tuple_suffix() {
        let expr = parse_str("match x { Triple::V(.., last) => last }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Call { args, .. } = &arms[0].pattern else {
            panic!("expected call pattern")
        };
        let TuplePattern::Suffix { patterns, .. } = args else {
            panic!("expected tuple suffix pattern")
        };
        assert_eq!(patterns.len(), 1);
        assert!(matches!(&patterns[0], Pattern::Path(p) if p.segments == ["last"]));
    }

    #[test]
    fn test_parse_enum_pattern_tuple_prefix_suffix() {
        let expr = parse_str("match x { Triple::V(a, .., z) => a + z }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        let Pattern::Call { args, .. } = &arms[0].pattern else {
            panic!("expected call pattern")
        };
        let TuplePattern::PrefixSuffix { prefix, suffix, .. } = args else {
            panic!("expected tuple prefix+suffix pattern")
        };
        assert_eq!(prefix.len(), 1);
        assert_eq!(suffix.len(), 1);
    }

    // ===== Error case tests =====

    // Note: Some error cases produce generic parser errors rather than our custom
    // messages because chumsky fails earlier during parsing before reaching our
    // try_map validation. We still verify that invalid syntax produces errors.

    #[test]
    fn test_parse_list_pattern_multiple_rest_error() {
        // Multiple .. in list pattern - parser fails with a syntax error
        let result = parse_str("match xs { [a, .., .., c] => a }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tuple_pattern_multiple_rest_error() {
        // Multiple .. in tuple pattern - parser fails with a syntax error
        let result = parse_str("match t { (a, .., .., z) => a }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_struct_pattern_rest_binding_error() {
        // @ binding on struct rest pattern - not allowed
        let result = parse_str("match p { Point { x, y @ .. } => x }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_enum_tuple_pattern_multiple_rest_error() {
        // Multiple .. in enum tuple pattern - parser fails with a syntax error
        let result = parse_str("match x { Triple::V(a, .., .., z) => a }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_enum_struct_pattern_multiple_rest_error() {
        // Multiple .. in struct pattern (enum variant or struct) - produces our custom error
        let result = parse_str("match m { Message::Move { x, .., .. } => x }");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("only one .. allowed in struct pattern")
        );
    }

    #[test]
    fn test_parse_enum_struct_pattern_rest_binding_error() {
        // @ binding on enum struct rest pattern - not allowed
        let result = parse_str("match m { Message::Move { x, y @ .. } => x }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_file_error() {
        // Invalid syntax in file - should produce an error
        let result = parse_file_str("fn foo( 42");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_struct_pattern_multiple_rest_error() {
        // Multiple .. in struct pattern - parser fails with a syntax error
        let result = parse_str("match p { Point { x, .., .. } => x }");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_let_type_annotation_on_tuple_error() {
        // Type annotation on tuple pattern in let - produces custom error in match arm
        let result = parse_str("match x { n => { let (a, b): (Int, Int) = n; a } }");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("type annotations are only allowed on simple variable patterns")
        );
    }

    #[test]
    fn test_parse_lambda_let_type_annotation_error() {
        // Type annotation on tuple pattern in lambda body - produces custom error
        let result = parse_str("|x| { let (a, b): (Int, Int) = x; a }");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("type annotations are only allowed on simple variable patterns")
        );
    }

    #[test]
    fn test_parse_lambda_braced_body_no_bindings() {
        // Lambda with braces but no let bindings - should unwrap to just the expression
        let expr = parse_str("|x| { x + 1 }").unwrap();
        let Expr::Lambda { body, .. } = expr else {
            panic!("expected lambda")
        };
        // Should not be a Block, just a BinOp expression
        assert!(matches!(*body, Expr::BinOp { op: BinOp::Add, .. }));
    }

    // === Module parsing tests ===

    fn parse_module_str(input: &str) -> Result<(Vec<ModDecl>, Vec<Item>), ParseError> {
        let tokens = lex(input).map_err(|e| {
            ParseError::SyntaxErrors(vec![SyntaxError {
                span: 0..0,
                found: Some(e.to_string()),
                expected: vec![],
                label: None,
            }])
        })?;
        parse_module(tokens)
    }

    #[test]
    fn test_parse_module_empty() {
        let (mods, items) = parse_module_str("").unwrap();
        assert!(mods.is_empty());
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_module_items_only() {
        let (mods, items) = parse_module_str("fn foo() -> Int 42").unwrap();
        assert!(mods.is_empty());
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_parse_module_mods_only() {
        let (mods, items) = parse_module_str("mod foo mod bar").unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, "foo");
        assert_eq!(mods[0].visibility, Visibility::Private);
        assert_eq!(mods[1].name, "bar");
        assert_eq!(mods[1].visibility, Visibility::Private);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_module_pub_mod() {
        let (mods, _) = parse_module_str("pub mod foo mod bar").unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, "foo");
        assert_eq!(mods[0].visibility, Visibility::Public);
        assert_eq!(mods[1].name, "bar");
        assert_eq!(mods[1].visibility, Visibility::Private);
    }

    #[test]
    fn test_parse_module_mods_and_items() {
        let (mods, items) = parse_module_str("mod utils mod helpers fn main() -> Int 42").unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(items.len(), 1);
    }

    // Use declaration tests

    #[test]
    fn test_parse_module_use_root() {
        let (mods, items) = parse_module_str("use root::foo::bar").unwrap();
        assert!(mods.is_empty());
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].visibility, Visibility::Private);
        assert_eq!(uses[0].path.prefix, PathPrefix::Root);
        assert_eq!(uses[0].path.segments, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_module_pub_use() {
        let (_, items) = parse_module_str("pub use root::foo::bar").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].visibility, Visibility::Public);
        assert_eq!(uses[0].path.prefix, PathPrefix::Root);
        assert_eq!(uses[0].path.segments, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_module_use_self() {
        let (_, items) = parse_module_str("use self::helper").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].path.prefix, PathPrefix::Self_);
        assert_eq!(uses[0].path.segments, vec!["helper"]);
    }

    #[test]
    fn test_parse_module_use_super() {
        let (_, items) = parse_module_str("use super::parent_fn").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].path.prefix, PathPrefix::Super);
        assert_eq!(uses[0].path.segments, vec!["parent_fn"]);
    }

    #[test]
    fn test_parse_module_use_without_prefix_is_package() {
        let (_, items) = parse_module_str("use serde::Deserialize").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(
            uses[0].path.prefix,
            PathPrefix::Package("serde".to_string())
        );
        assert_eq!(uses[0].path.segments, vec!["Deserialize"]);
    }

    #[test]
    fn test_parse_module_use_package_glob() {
        let (_, items) = parse_module_str("use serde::*").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(
            uses[0].path.prefix,
            PathPrefix::Package("serde".to_string())
        );
        assert!(matches!(uses[0].path.target, UseTarget::Glob));
    }

    #[test]
    fn test_parse_module_use_package_group() {
        let (_, items) = parse_module_str("use serde::{A, B}").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(
            uses[0].path.prefix,
            PathPrefix::Package("serde".to_string())
        );
        if let UseTarget::Group(items) = &uses[0].path.target {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "A");
            assert_eq!(items[1].name, "B");
        } else {
            panic!("expected Group target");
        }
    }

    #[test]
    fn test_parse_module_use_multiple() {
        let (_, items) = parse_module_str("use root::a::b use root::c::d").unwrap();
        let uses_count = items.iter().filter(|i| matches!(i, Item::Use(_))).count();
        assert_eq!(uses_count, 2);
    }

    #[test]
    fn test_parse_module_mods_uses_items() {
        let (mods, items) =
            parse_module_str("mod utils use root::types::Option fn main() -> Int 42").unwrap();
        assert_eq!(mods.len(), 1);
        let uses_count = items.iter().filter(|i| matches!(i, Item::Use(_))).count();
        let non_use_count = items.iter().filter(|i| !matches!(i, Item::Use(_))).count();
        assert_eq!(uses_count, 1);
        assert_eq!(non_use_count, 1);
    }

    // Path prefix tests

    use zoya_ast::{PathPrefix, UseTarget};

    #[test]
    fn test_parse_path_no_prefix() {
        let expr = parse_str("foo").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.prefix, PathPrefix::None);
                assert_eq!(path.segments, vec!["foo"]);
            }
            _ => panic!("expected path"),
        }
    }

    #[test]
    fn test_parse_path_root_prefix() {
        let expr = parse_str("root::foo").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.prefix, PathPrefix::Root);
                assert_eq!(path.segments, vec!["foo"]);
            }
            _ => panic!("expected path"),
        }
    }

    #[test]
    fn test_parse_path_root_prefix_multi_segment() {
        let expr = parse_str("root::utils::helper").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.prefix, PathPrefix::Root);
                assert_eq!(path.segments, vec!["utils", "helper"]);
            }
            _ => panic!("expected path"),
        }
    }

    #[test]
    fn test_parse_path_self_prefix() {
        let expr = parse_str("self::bar").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.prefix, PathPrefix::Self_);
                assert_eq!(path.segments, vec!["bar"]);
            }
            _ => panic!("expected path"),
        }
    }

    #[test]
    fn test_parse_path_super_prefix() {
        let expr = parse_str("super::baz").unwrap();
        match expr {
            Expr::Path(path) => {
                assert_eq!(path.prefix, PathPrefix::Super);
                assert_eq!(path.segments, vec!["baz"]);
            }
            _ => panic!("expected path"),
        }
    }

    #[test]
    fn test_parse_path_prefix_call() {
        let expr = parse_str("root::utils::add(1, 2)").unwrap();
        match expr {
            Expr::Call { path, args } => {
                assert_eq!(path.prefix, PathPrefix::Root);
                assert_eq!(path.segments, vec!["utils", "add"]);
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn test_parse_path_prefix_enum_variant() {
        // Note: enum patterns in expressions still use Enum::Variant syntax
        // The path prefix is for module resolution
        let expr = parse_str("root::types::Option::Some(42)").unwrap();
        match expr {
            Expr::Call { path, args } => {
                assert_eq!(path.prefix, PathPrefix::Root);
                assert_eq!(path.segments, vec!["types", "Option", "Some"]);
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected call"),
        }
    }

    // Type annotation path prefix tests

    #[test]
    fn test_parse_type_annotation_qualified() {
        let (items, _) = parse_input_str("fn foo(x: utils::MyType) -> Int { 0 }").unwrap();
        match &items[0] {
            Item::Function(f) => {
                let param_type = &f.params[0].typ;
                match param_type {
                    TypeAnnotation::Named(path) => {
                        assert_eq!(path.prefix, PathPrefix::None);
                        assert_eq!(path.segments, vec!["utils", "MyType"]);
                    }
                    _ => panic!("expected named type"),
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_type_annotation_root_prefix() {
        let (items, _) = parse_input_str("fn foo(x: root::types::MyType) -> Int { 0 }").unwrap();
        match &items[0] {
            Item::Function(f) => {
                let param_type = &f.params[0].typ;
                match param_type {
                    TypeAnnotation::Named(path) => {
                        assert_eq!(path.prefix, PathPrefix::Root);
                        assert_eq!(path.segments, vec!["types", "MyType"]);
                    }
                    _ => panic!("expected named type"),
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_type_annotation_self_prefix() {
        let (items, _) = parse_input_str("fn foo(x: self::MyType) -> Int { 0 }").unwrap();
        match &items[0] {
            Item::Function(f) => {
                let param_type = &f.params[0].typ;
                match param_type {
                    TypeAnnotation::Named(path) => {
                        assert_eq!(path.prefix, PathPrefix::Self_);
                        assert_eq!(path.segments, vec!["MyType"]);
                    }
                    _ => panic!("expected named type"),
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_type_annotation_super_prefix() {
        let (items, _) = parse_input_str("fn foo(x: super::parent::Type) -> Int { 0 }").unwrap();
        match &items[0] {
            Item::Function(f) => {
                let param_type = &f.params[0].typ;
                match param_type {
                    TypeAnnotation::Named(path) => {
                        assert_eq!(path.prefix, PathPrefix::Super);
                        assert_eq!(path.segments, vec!["parent", "Type"]);
                    }
                    _ => panic!("expected named type"),
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_type_annotation_parameterized_qualified() {
        let (items, _) =
            parse_input_str("fn foo(x: root::types::Option<Int>) -> Int { 0 }").unwrap();
        match &items[0] {
            Item::Function(f) => {
                let param_type = &f.params[0].typ;
                match param_type {
                    TypeAnnotation::Parameterized(path, params) => {
                        assert_eq!(path.prefix, PathPrefix::Root);
                        assert_eq!(path.segments, vec!["types", "Option"]);
                        assert_eq!(params.len(), 1);
                    }
                    _ => panic!("expected parameterized type"),
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_type_annotation_deep_path() {
        let (items, _) = parse_input_str("fn foo(x: root::a::b::c::MyType) -> Int { 0 }").unwrap();
        match &items[0] {
            Item::Function(f) => {
                let param_type = &f.params[0].typ;
                match param_type {
                    TypeAnnotation::Named(path) => {
                        assert_eq!(path.prefix, PathPrefix::Root);
                        assert_eq!(path.segments, vec!["a", "b", "c", "MyType"]);
                    }
                    _ => panic!("expected named type"),
                }
            }
            _ => panic!("expected function"),
        }
    }

    // Struct pattern path prefix tests

    #[test]
    fn test_parse_struct_pattern_qualified() {
        // Paths with 2+ segments are now parsed as Pattern::Struct (unified)
        // The type checker will determine if it's a struct or enum variant
        let expr = parse_str("match x { types::Point { x, y } => x }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Struct {
                    path,
                    fields,
                    is_partial,
                } => {
                    assert_eq!(path.prefix, PathPrefix::None);
                    assert_eq!(path.segments, vec!["types", "Point"]);
                    assert!(!is_partial);
                    assert_eq!(fields.len(), 2);
                }
                _ => panic!("expected struct pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_struct_pattern_root_prefix() {
        let expr = parse_str("match x { root::Point { x, y } => x }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Struct {
                    path,
                    fields,
                    is_partial: false,
                } => {
                    assert_eq!(path.prefix, PathPrefix::Root);
                    assert_eq!(path.segments, vec!["Point"]);
                    assert_eq!(fields.len(), 2);
                }
                _ => panic!("expected struct pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_struct_pattern_self_prefix() {
        let expr = parse_str("match x { self::Point { x, .. } => x }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Struct {
                    path,
                    fields,
                    is_partial: true,
                } => {
                    assert_eq!(path.prefix, PathPrefix::Self_);
                    assert_eq!(path.segments, vec!["Point"]);
                    assert_eq!(fields.len(), 1);
                }
                _ => panic!("expected partial struct pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_struct_pattern_super_prefix() {
        let expr = parse_str("match x { super::Point { x } => x }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Struct {
                    path,
                    fields,
                    is_partial: false,
                } => {
                    assert_eq!(path.prefix, PathPrefix::Super);
                    assert_eq!(path.segments, vec!["Point"]);
                    assert_eq!(fields.len(), 1);
                }
                _ => panic!("expected struct pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    // Enum pattern path prefix tests (now using Pattern::Call and Pattern::Path)

    #[test]
    fn test_parse_enum_pattern_qualified() {
        let expr =
            parse_str("match x { types::Option::Some(v) => v, types::Option::None => 0 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => {
                match &arms[0].pattern {
                    Pattern::Call { path, args } => {
                        assert_eq!(path.prefix, PathPrefix::None);
                        assert_eq!(path.segments, vec!["types", "Option", "Some"]);
                        assert!(matches!(args, TuplePattern::Exact(_)));
                    }
                    _ => panic!("expected call pattern"),
                }
                match &arms[1].pattern {
                    Pattern::Path(path) => {
                        assert_eq!(path.prefix, PathPrefix::None);
                        assert_eq!(path.segments, vec!["types", "Option", "None"]);
                    }
                    _ => panic!("expected path pattern"),
                }
            }
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_root_prefix() {
        let expr = parse_str("match x { root::types::Result::Ok(v) => v, _ => 0 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Call { path, args } => {
                    assert_eq!(path.prefix, PathPrefix::Root);
                    assert_eq!(path.segments, vec!["types", "Result", "Ok"]);
                    assert!(matches!(args, TuplePattern::Exact(_)));
                }
                _ => panic!("expected call pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_self_prefix() {
        let expr = parse_str("match x { self::Option::None => 0, _ => 1 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Path(path) => {
                    assert_eq!(path.prefix, PathPrefix::Self_);
                    assert_eq!(path.segments, vec!["Option", "None"]);
                }
                _ => panic!("expected path pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_super_prefix() {
        let expr = parse_str("match x { super::parent::Color::Red => 1, _ => 0 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Path(path) => {
                    assert_eq!(path.prefix, PathPrefix::Super);
                    assert_eq!(path.segments, vec!["parent", "Color", "Red"]);
                }
                _ => panic!("expected path pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_struct_variant_qualified() {
        let expr = parse_str("match x { root::Message::Move { x, y } => x, _ => 0 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Struct {
                    path,
                    fields,
                    is_partial,
                } => {
                    assert_eq!(path.prefix, PathPrefix::Root);
                    assert_eq!(path.segments, vec!["Message", "Move"]);
                    assert!(!is_partial);
                    assert_eq!(fields.len(), 2);
                }
                _ => panic!("expected struct pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_turbofish_qualified() {
        let expr = parse_str("match x { root::Option::None::<Int> => 0, _ => 1 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Path(path) => {
                    assert_eq!(path.prefix, PathPrefix::Root);
                    assert_eq!(path.segments, vec!["Option", "None"]);
                    assert!(path.type_args.is_some());
                    assert_eq!(path.type_args.as_ref().unwrap().len(), 1);
                }
                _ => panic!("expected path pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_deep_path() {
        let expr = parse_str("match x { root::a::b::Option::Some(v) => v, _ => 0 }").unwrap();
        match expr {
            Expr::Match { arms, .. } => match &arms[0].pattern {
                Pattern::Call { path, .. } => {
                    assert_eq!(path.prefix, PathPrefix::Root);
                    assert_eq!(path.segments, vec!["a", "b", "Option", "Some"]);
                }
                _ => panic!("expected call pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn test_parse_enum_pattern_single_segment_call() {
        let expr = parse_str("match x { Some(v) => v, None => 0 }").unwrap();
        let Expr::Match { arms, .. } = expr else {
            panic!("expected match expression")
        };
        // First arm: Some(v) should be a call pattern
        let Pattern::Call { path, args } = &arms[0].pattern else {
            panic!("expected call pattern for single-segment")
        };
        assert_eq!(path.prefix, PathPrefix::None);
        assert_eq!(path.segments, vec!["Some"]);
        assert!(matches!(args, TuplePattern::Exact(_)));

        // Second arm: None should be a Path pattern (no parens = not a call)
        assert!(matches!(&arms[1].pattern, Pattern::Path(p) if p.segments == ["None"]));
    }

    // Expression struct constructor path prefix tests

    #[test]
    fn test_parse_struct_expr_root_prefix() {
        let expr = parse_str("root::Point { x: 1, y: 2 }").unwrap();
        match expr {
            Expr::Struct { path, fields } => {
                assert_eq!(path.prefix, PathPrefix::Root);
                assert_eq!(path.segments, vec!["Point"]);
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_struct_expr_self_prefix() {
        let expr = parse_str("self::Point { x: 1 }").unwrap();
        match expr {
            Expr::Struct { path, fields } => {
                assert_eq!(path.prefix, PathPrefix::Self_);
                assert_eq!(path.segments, vec!["Point"]);
                assert_eq!(fields.len(), 1);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_struct_expr_super_prefix() {
        let expr = parse_str("super::Point { x: 1 }").unwrap();
        match expr {
            Expr::Struct { path, fields } => {
                assert_eq!(path.prefix, PathPrefix::Super);
                assert_eq!(path.segments, vec!["Point"]);
                assert_eq!(fields.len(), 1);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_struct_expr_qualified() {
        let expr = parse_str("types::shapes::Point { x: 1, y: 2 }").unwrap();
        match expr {
            Expr::Struct { path, fields } => {
                assert_eq!(path.prefix, PathPrefix::None);
                assert_eq!(path.segments, vec!["types", "shapes", "Point"]);
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    // ========================================================================
    // Use declaration parsing tests
    // ========================================================================

    use zoya_ast::UseDecl;

    fn get_use_decl(input: &str) -> UseDecl {
        let (_, items) = parse_module_str(input).unwrap();
        match items.into_iter().next().unwrap() {
            Item::Use(u) => u,
            _ => panic!("expected use declaration"),
        }
    }

    #[test]
    fn test_parse_use_single() {
        let u = get_use_decl("use root::foo::bar");
        assert_eq!(u.visibility, Visibility::Private);
        assert_eq!(u.path.prefix, PathPrefix::Root);
        assert_eq!(u.path.segments, vec!["foo", "bar"]);
        assert_eq!(u.path.target, UseTarget::Single { alias: None });
    }

    #[test]
    fn test_parse_use_pub_single() {
        let u = get_use_decl("pub use root::foo::bar");
        assert_eq!(u.visibility, Visibility::Public);
        assert_eq!(u.path.prefix, PathPrefix::Root);
        assert_eq!(u.path.segments, vec!["foo", "bar"]);
        assert_eq!(u.path.target, UseTarget::Single { alias: None });
    }

    #[test]
    fn test_parse_use_glob() {
        let u = get_use_decl("use root::foo::bar::*");
        assert_eq!(u.visibility, Visibility::Private);
        assert_eq!(u.path.prefix, PathPrefix::Root);
        assert_eq!(u.path.segments, vec!["foo", "bar"]);
        assert_eq!(u.path.target, UseTarget::Glob);
    }

    #[test]
    fn test_parse_use_pub_glob() {
        let u = get_use_decl("pub use root::foo::bar::*");
        assert_eq!(u.visibility, Visibility::Public);
        assert_eq!(u.path.target, UseTarget::Glob);
    }

    #[test]
    fn test_parse_use_group() {
        let u = get_use_decl("use root::foo::bar::{add, divide}");
        assert_eq!(u.path.prefix, PathPrefix::Root);
        assert_eq!(u.path.segments, vec!["foo", "bar"]);
        match u.path.target {
            UseTarget::Group(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].name, "add");
                assert_eq!(items[0].alias, None);
                assert_eq!(items[1].name, "divide");
                assert_eq!(items[1].alias, None);
            }
            _ => panic!("expected Group target"),
        }
    }

    #[test]
    fn test_parse_use_group_trailing_comma() {
        let u = get_use_decl("use root::foo::{add, divide,}");
        match u.path.target {
            UseTarget::Group(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].name, "add");
                assert_eq!(items[1].name, "divide");
            }
            _ => panic!("expected Group target"),
        }
    }

    #[test]
    fn test_parse_use_group_single_item() {
        let u = get_use_decl("use root::foo::{bar}");
        match u.path.target {
            UseTarget::Group(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "bar");
            }
            _ => panic!("expected Group target"),
        }
    }

    #[test]
    fn test_parse_use_pub_group() {
        let u = get_use_decl("pub use root::foo::{add, sub}");
        assert_eq!(u.visibility, Visibility::Public);
        match u.path.target {
            UseTarget::Group(items) => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("expected Group target"),
        }
    }

    #[test]
    fn test_parse_use_self_glob() {
        let u = get_use_decl("use self::utils::*");
        assert_eq!(u.path.prefix, PathPrefix::Self_);
        assert_eq!(u.path.segments, vec!["utils"]);
        assert_eq!(u.path.target, UseTarget::Glob);
    }

    #[test]
    fn test_parse_use_super_group() {
        let u = get_use_decl("use super::helpers::{add, mul}");
        assert_eq!(u.path.prefix, PathPrefix::Super);
        assert_eq!(u.path.segments, vec!["helpers"]);
        match u.path.target {
            UseTarget::Group(items) => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("expected Group target"),
        }
    }

    #[test]
    fn test_parse_use_no_prefix_is_package() {
        // Prefix-free use paths are now parsed as package paths
        let (_, items) = parse_module_str("use foo::bar::*").unwrap();
        let uses: Vec<_> = items
            .iter()
            .filter_map(|i| if let Item::Use(u) = i { Some(u) } else { None })
            .collect();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].path.prefix, PathPrefix::Package("foo".to_string()));
        assert_eq!(uses[0].path.segments, vec!["bar"]);
        assert_eq!(uses[0].path.target, UseTarget::Glob);
    }

    #[test]
    fn test_parse_use_empty_group_error() {
        let result = parse_module_str("use root::foo::{}");
        assert!(result.is_err());
    }

    // List index tests

    #[test]
    fn test_parse_list_index_basic() {
        let expr = parse_str("list[0]").unwrap();
        assert_eq!(
            expr,
            Expr::ListIndex {
                expr: Box::new(Expr::Path(Path::simple("list".to_string()))),
                index: Box::new(Expr::Int(0)),
            }
        );
    }

    #[test]
    fn test_parse_list_index_complex_expr() {
        let expr = parse_str("list[a + 1]").unwrap();
        assert_eq!(
            expr,
            Expr::ListIndex {
                expr: Box::new(Expr::Path(Path::simple("list".to_string()))),
                index: Box::new(Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Path(Path::simple("a".to_string()))),
                    right: Box::new(Expr::Int(1)),
                }),
            }
        );
    }

    #[test]
    fn test_parse_list_index_chained() {
        let expr = parse_str("matrix[0][1]").unwrap();
        assert_eq!(
            expr,
            Expr::ListIndex {
                expr: Box::new(Expr::ListIndex {
                    expr: Box::new(Expr::Path(Path::simple("matrix".to_string()))),
                    index: Box::new(Expr::Int(0)),
                }),
                index: Box::new(Expr::Int(1)),
            }
        );
    }

    #[test]
    fn test_parse_list_index_after_method() {
        let expr = parse_str("list.reverse()[0]").unwrap();
        assert_eq!(
            expr,
            Expr::ListIndex {
                expr: Box::new(Expr::MethodCall {
                    receiver: Box::new(Expr::Path(Path::simple("list".to_string()))),
                    method: "reverse".to_string(),
                    args: vec![],
                }),
                index: Box::new(Expr::Int(0)),
            }
        );
    }

    #[test]
    fn test_parse_list_literal_index() {
        let expr = parse_str("[1, 2, 3][0]").unwrap();
        assert_eq!(
            expr,
            Expr::ListIndex {
                expr: Box::new(Expr::List(vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)])),
                index: Box::new(Expr::Int(0)),
            }
        );
    }

    #[test]
    fn test_missing_comma_between_match_arms_error() {
        let err = parse_str("match x { A => { 1 } B => 2 }").unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("comma between match arms"),
            "expected error about comma between match arms, got: {}",
            msg
        );
        // Verify human-readable token display
        assert!(
            msg.contains("'B'"),
            "expected human-readable token 'B', got: {}",
            msg
        );
    }

    #[test]
    fn test_parse_tuple_index() {
        let expr = parse_str("t.0").unwrap();
        assert_eq!(
            expr,
            Expr::TupleIndex {
                expr: Box::new(Expr::Path(Path::simple("t".to_string()))),
                index: 0,
            }
        );
    }

    #[test]
    fn test_parse_tuple_index_then_field() {
        let expr = parse_str("t.0.name").unwrap();
        let Expr::FieldAccess { expr: inner, field } = expr else {
            panic!("expected field access")
        };
        assert_eq!(field, "name");
        assert!(matches!(*inner, Expr::TupleIndex { index: 0, .. }));
    }

    #[test]
    fn test_parse_chained_tuple_index() {
        let expr = parse_str("t.0.1").unwrap();
        let Expr::TupleIndex { expr: inner, index } = expr else {
            panic!("expected tuple index")
        };
        assert_eq!(index, 1);
        assert!(matches!(*inner, Expr::TupleIndex { index: 0, .. }));
    }
}
