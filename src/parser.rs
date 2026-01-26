use chumsky::prelude::*;

use crate::ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, MatchArm, Param, Pattern, Statement,
    TypeAnnotation, UnaryOp,
};
use crate::lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
}

/// Parse multiple top-level items (for files)
pub fn parse_file(tokens: Vec<Token>) -> Result<Vec<Item>, ParseError> {
    item_parser()
        .repeated()
        .collect()
        .parse(&tokens)
        .into_result()
        .map_err(|errs| ParseError {
            message: errs
                .into_iter()
                .map(|e| format!("{:?}", e))
                .collect::<Vec<_>>()
                .join(", "),
        })
}

/// Parse REPL input: zero or more statements (items or expressions)
pub fn parse_repl(tokens: Vec<Token>) -> Result<Vec<Statement>, ParseError> {
    statement_parser()
        .repeated()
        .collect()
        .parse(&tokens)
        .into_result()
        .map_err(|errs| ParseError {
            message: errs
                .into_iter()
                .map(|e| format!("{:?}", e))
                .collect::<Vec<_>>()
                .join(", "),
        })
}

fn ident<'a>() -> impl Parser<'a, &'a [Token], String, extra::Err<Rich<'a, Token>>> + Clone {
    select! { Token::Ident(name) => name }
}

fn type_annotation<'a>() -> impl Parser<'a, &'a [Token], TypeAnnotation, extra::Err<Rich<'a, Token>>>
{
    ident().map(TypeAnnotation::Named)
}

fn let_binding_parser<'a>(
) -> impl Parser<'a, &'a [Token], LetBinding, extra::Err<Rich<'a, Token>>> {
    just(Token::Let)
        .ignore_then(ident())
        .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr_parser())
        .map(|((name, type_annotation), value)| LetBinding {
            name,
            type_annotation,
            value: Box::new(value),
        })
}

fn item_parser<'a>() -> impl Parser<'a, &'a [Token], Item, extra::Err<Rich<'a, Token>>> {
    // Type parameters: <T, U>
    let type_params = ident()
        .separated_by(just(Token::Comma))
        .collect::<Vec<_>>()
        .delimited_by(just(Token::Lt), just(Token::Gt))
        .or_not()
        .map(|opt| opt.unwrap_or_default());

    // Parameter: name: Type
    let param = ident()
        .then_ignore(just(Token::Colon))
        .then(type_annotation())
        .map(|(name, typ)| Param { name, typ });

    // Parameters: (x: Int, y: Int)
    let params = param
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LParen), just(Token::RParen));

    // Return type: -> Int
    let return_type = just(Token::Arrow).ignore_then(type_annotation()).or_not();

    // Body: { [let x = e [;]]* expr }
    let body = just(Token::LBrace)
        .ignore_then(
            let_binding_parser()
                .then_ignore(just(Token::Semicolon).or_not())
                .repeated()
                .collect::<Vec<_>>(),
        )
        .then(expr_parser())
        .then_ignore(just(Token::RBrace))
        .map(|(bindings, result)| {
            if bindings.is_empty() {
                result
            } else {
                Expr::Block {
                    bindings,
                    result: Box::new(result),
                }
            }
        });

    // fn name<T>(params) -> ReturnType { body }
    just(Token::Fn)
        .ignore_then(ident())
        .then(type_params)
        .then(params)
        .then(return_type)
        .then(body)
        .map(|((((name, type_params), params), return_type), body)| {
            Item::Function(FunctionDef {
                name,
                type_params,
                params,
                return_type,
                body,
            })
        })
}

fn expr_parser<'a>() -> impl Parser<'a, &'a [Token], Expr, extra::Err<Rich<'a, Token>>> {
    recursive(|expr| {
        let literal = select! {
            Token::Int(n) => Expr::Int(n),
            Token::Float(n) => Expr::Float(n),
            Token::True => Expr::Bool(true),
            Token::False => Expr::Bool(false),
            Token::String(s) => Expr::String(s),
        };

        // Pattern for match arms
        let pattern = choice((
            // Wildcard: _ (must check before ident)
            select! { Token::Ident(s) if s == "_" => Pattern::Wildcard },
            // Literals
            select! {
                Token::Int(n) => Pattern::Literal(Box::new(Expr::Int(n))),
                Token::Float(n) => Pattern::Literal(Box::new(Expr::Float(n))),
                Token::True => Pattern::Literal(Box::new(Expr::Bool(true))),
                Token::False => Pattern::Literal(Box::new(Expr::Bool(false))),
                Token::String(s) => Pattern::Literal(Box::new(Expr::String(s))),
            },
            // Variable (must be last)
            ident().map(Pattern::Var),
        ));

        // Match arm: pattern => expr
        let match_arm = pattern
            .then_ignore(just(Token::FatArrow))
            .then(expr.clone())
            .map(|(pattern, result)| MatchArm { pattern, result });

        // Match expression: match scrutinee { arms }
        let match_expr = just(Token::Match)
            .ignore_then(expr.clone())
            .then(
                match_arm
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map(|(scrutinee, arms)| Expr::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            });

        // Arguments: (expr, expr, ...)
        let args = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Identifier: variable or function call
        let ident_expr = ident().then(args.or_not()).map(|(name, args)| match args {
            Some(args) => Expr::Call { func: name, args },
            None => Expr::Var(name),
        });

        let atom = choice((
            literal,
            match_expr,
            ident_expr,
            expr.delimited_by(just(Token::LParen), just(Token::RParen)),
        ));

        let unary = just(Token::Minus)
            .repeated()
            .foldr(atom, |_, e| Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(e),
            });

        let op = |t: Token, op: BinOp| just(t).to(op);

        let product = unary.clone().foldl(
            choice((op(Token::Star, BinOp::Mul), op(Token::Slash, BinOp::Div)))
                .then(unary)
                .repeated(),
            |left, (op, right)| Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
        );

        let sum = product.clone().foldl(
            choice((op(Token::Plus, BinOp::Add), op(Token::Minus, BinOp::Sub)))
                .then(product)
                .repeated(),
            |left, (op, right)| Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
        );

        // Comparison operators (lowest precedence)
        sum.clone().foldl(
            choice((
                op(Token::EqEq, BinOp::Eq),
                op(Token::Ne, BinOp::Ne),
                op(Token::Le, BinOp::Le),
                op(Token::Ge, BinOp::Ge),
                op(Token::Lt, BinOp::Lt),
                op(Token::Gt, BinOp::Gt),
            ))
            .then(sum)
            .repeated(),
            |left, (op, right)| Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
        )
    })
}

fn statement_parser<'a>() -> impl Parser<'a, &'a [Token], Statement, extra::Err<Rich<'a, Token>>> {
    // Try to parse as an item (starts with fn), let binding, or expression
    choice((
        item_parser().map(Statement::Item),
        let_binding_parser().map(Statement::Let),
        expr_parser().map(Statement::Expr),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse(tokens: Vec<Token>) -> Result<Expr, ParseError> {
        expr_parser()
            .parse(&tokens)
            .into_result()
            .map_err(|errs| ParseError {
                message: errs
                    .into_iter()
                    .map(|e| format!("{:?}", e))
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }

    fn parse_item(tokens: Vec<Token>) -> Result<Item, ParseError> {
        item_parser()
            .parse(&tokens)
            .into_result()
            .map_err(|errs| ParseError {
                message: errs
                    .into_iter()
                    .map(|e| format!("{:?}", e))
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }

    fn parse_str(input: &str) -> Result<Expr, ParseError> {
        let tokens = lex(input).expect("lexing failed");
        parse(tokens)
    }

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
    fn test_parse_float() {
        let expr = parse_str("3.14").unwrap();
        assert_eq!(expr, Expr::Float(3.14));
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
        let expr = parse_str("-3.14").unwrap();
        assert_eq!(
            expr,
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(Expr::Float(3.14)),
            }
        );
    }

    #[test]
    fn test_parse_variable() {
        let expr = parse_str("x").unwrap();
        assert_eq!(expr, Expr::Var("x".to_string()));
    }

    #[test]
    fn test_parse_variable_in_expression() {
        let expr = parse_str("x + y").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Var("y".to_string())),
            }
        );
    }

    #[test]
    fn test_parse_function_call_no_args() {
        let expr = parse_str("foo()").unwrap();
        assert_eq!(
            expr,
            Expr::Call {
                func: "foo".to_string(),
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
                func: "square".to_string(),
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
                func: "add".to_string(),
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
                func: "add".to_string(),
                args: vec![
                    Expr::BinOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Int(1)),
                        right: Box::new(Expr::Int(2)),
                    },
                    Expr::BinOp {
                        op: BinOp::Mul,
                        left: Box::new(Expr::Var("x".to_string())),
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
                func: "foo".to_string(),
                args: vec![Expr::Call {
                    func: "bar".to_string(),
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
                    func: "square".to_string(),
                    args: vec![Expr::Int(2)],
                }),
            }
        );
    }

    use crate::ast::{FunctionDef, Item, Param, TypeAnnotation};

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
                name: "foo".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: Some(TypeAnnotation::Named("Int".to_string())),
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
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".to_string(),
                        typ: TypeAnnotation::Named("Int".to_string()),
                    },
                    Param {
                        name: "y".to_string(),
                        typ: TypeAnnotation::Named("Int".to_string()),
                    },
                ],
                return_type: Some(TypeAnnotation::Named("Int".to_string())),
                body: Expr::BinOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Var("x".to_string())),
                    right: Box::new(Expr::Var("y".to_string())),
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
                name: "identity".to_string(),
                type_params: vec!["T".to_string()],
                params: vec![Param {
                    name: "x".to_string(),
                    typ: TypeAnnotation::Named("T".to_string()),
                }],
                return_type: Some(TypeAnnotation::Named("T".to_string())),
                body: Expr::Var("x".to_string()),
            })
        );
    }

    #[test]
    fn test_parse_function_multiple_type_params() {
        let item = parse_item_str("fn pair<A, B>(a: A, b: B) { a }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                name: "pair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                params: vec![
                    Param {
                        name: "a".to_string(),
                        typ: TypeAnnotation::Named("A".to_string()),
                    },
                    Param {
                        name: "b".to_string(),
                        typ: TypeAnnotation::Named("B".to_string()),
                    },
                ],
                return_type: None,
                body: Expr::Var("a".to_string()),
            })
        );
    }

    #[test]
    fn test_parse_function_with_call_body() {
        let item = parse_item_str("fn double(x: Int) -> Int { add(x, x) }").unwrap();
        assert_eq!(
            item,
            Item::Function(FunctionDef {
                name: "double".to_string(),
                type_params: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    typ: TypeAnnotation::Named("Int".to_string()),
                }],
                return_type: Some(TypeAnnotation::Named("Int".to_string())),
                body: Expr::Call {
                    func: "add".to_string(),
                    args: vec![Expr::Var("x".to_string()), Expr::Var("x".to_string()),],
                },
            })
        );
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

    fn parse_repl_str(input: &str) -> Result<Vec<Statement>, ParseError> {
        let tokens = lex(input).expect("lexing failed");
        parse_repl(tokens)
    }

    #[test]
    fn test_parse_repl_single_expr() {
        let stmts = parse_repl_str("1 + 2").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Statement::Expr(_)));
    }

    #[test]
    fn test_parse_repl_single_function() {
        let stmts = parse_repl_str("fn foo() -> Int32 { 42 }").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Statement::Item(_)));
    }

    #[test]
    fn test_parse_repl_function_then_expr() {
        let stmts = parse_repl_str("fn foo() -> Int32 { 42 } foo()").unwrap();
        assert_eq!(stmts.len(), 2);
        assert!(matches!(stmts[0], Statement::Item(_)));
        assert!(matches!(stmts[1], Statement::Expr(_)));
    }

    #[test]
    fn test_parse_repl_multiple_exprs() {
        let stmts = parse_repl_str("1 2 3").unwrap();
        assert_eq!(stmts.len(), 3);
        assert!(matches!(stmts[0], Statement::Expr(Expr::Int(1))));
        assert!(matches!(stmts[1], Statement::Expr(Expr::Int(2))));
        assert!(matches!(stmts[2], Statement::Expr(Expr::Int(3))));
    }

    use crate::ast::LetBinding;

    #[test]
    fn test_parse_let_simple() {
        let stmts = parse_repl_str("let x = 42").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            &stmts[0],
            Statement::Let(LetBinding {
                name,
                type_annotation: None,
                value,
            }) if name == "x" && **value == Expr::Int(42)
        ));
    }

    #[test]
    fn test_parse_let_with_type() {
        let stmts = parse_repl_str("let x: Int32 = 42").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            &stmts[0],
            Statement::Let(LetBinding {
                name,
                type_annotation: Some(TypeAnnotation::Named(ty)),
                value,
            }) if name == "x" && ty == "Int32" && **value == Expr::Int(42)
        ));
    }

    #[test]
    fn test_parse_let_with_expression() {
        let stmts = parse_repl_str("let x = 1 + 2").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(&stmts[0], Statement::Let(_)));
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
        if let Item::Function(FunctionDef {
            body: Expr::Block { bindings, result },
            ..
        }) = item
        {
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[0].name, "x");
            assert_eq!(bindings[1].name, "y");
            assert!(matches!(*result, Expr::BinOp { .. }));
        } else {
            panic!("expected function with block body");
        }
    }

    #[test]
    fn test_parse_function_without_let_no_block() {
        // Without let statements, body should be a plain expression, not a block
        let item = parse_item_str("fn foo() { 42 }").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item;
        assert!(matches!(body, Expr::Int(42)));
    }

    #[test]
    fn test_parse_function_with_lets_no_semicolons() {
        // Semicolons are optional after let bindings
        let item = parse_item_str("fn foo() { let x = 1 let y = 2 x + y }").unwrap();
        if let Item::Function(FunctionDef {
            body: Expr::Block { bindings, result },
            ..
        }) = item
        {
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[0].name, "x");
            assert_eq!(bindings[1].name, "y");
            assert!(matches!(*result, Expr::BinOp { .. }));
        } else {
            panic!("expected function with block body");
        }
    }

    use crate::ast::{MatchArm, Pattern};

    #[test]
    fn test_parse_match_with_literals() {
        let expr = parse_str("match x { 0 => 1 1 => 2 }").unwrap();
        if let Expr::Match { scrutinee, arms } = expr {
            assert!(matches!(*scrutinee, Expr::Var(ref s) if s == "x"));
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
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_with_wildcard() {
        let expr = parse_str("match x { 0 => 1 _ => 2 }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
            assert!(matches!(arms[1].pattern, Pattern::Wildcard));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_with_variable() {
        let expr = parse_str("match x { n => n }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 1);
            assert!(matches!(&arms[0].pattern, Pattern::Var(s) if s == "n"));
            assert!(matches!(&arms[0].result, Expr::Var(s) if s == "n"));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_with_strings() {
        let expr = parse_str(r#"match s { "a" => 1 "b" => 2 }"#).unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Literal(lit) if **lit == Expr::String("a".to_string())
            ));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_in_function() {
        let item = parse_item_str("fn f(x: Int32) -> Int32 { match x { 0 => 0 n => n } }").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item;
        assert!(matches!(body, Expr::Match { .. }));
    }
}
