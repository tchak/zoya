use chumsky::prelude::*;

use crate::ast::{BinOp, Expr, FunctionDef, Item, Param, TypeAnnotation, UnaryOp};
use crate::lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
}

/// Parse an expression (for REPL and function bodies)
pub fn parse(tokens: Vec<Token>) -> Result<Expr, ParseError> {
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

/// Parse a top-level item (function declaration)
pub fn parse_item(tokens: Vec<Token>) -> Result<Item, ParseError> {
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

/// Parse multiple top-level items (for files)
pub fn parse_items(tokens: Vec<Token>) -> Result<Vec<Item>, ParseError> {
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

fn ident<'a>() -> impl Parser<'a, &'a [Token], String, extra::Err<Rich<'a, Token>>> + Clone {
    select! { Token::Ident(name) => name }
}

fn type_annotation<'a>() -> impl Parser<'a, &'a [Token], TypeAnnotation, extra::Err<Rich<'a, Token>>>
{
    ident().map(TypeAnnotation::Named)
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
    let return_type = just(Token::Arrow)
        .ignore_then(type_annotation())
        .or_not();

    // Body: { expr }
    let body = expr_parser().delimited_by(just(Token::LBrace), just(Token::RBrace));

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
        };

        // Arguments: (expr, expr, ...)
        let args = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Identifier: variable or function call
        let ident_expr = ident().then(args.or_not()).map(|(name, args)| {
            match args {
                Some(args) => Expr::Call { func: name, args },
                None => Expr::Var(name),
            }
        });

        let atom = choice((
            literal,
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
            choice((
                op(Token::Star, BinOp::Mul),
                op(Token::Slash, BinOp::Div),
            ))
            .then(unary)
            .repeated(),
            |left, (op, right)| Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
        );

        product.clone().foldl(
            choice((
                op(Token::Plus, BinOp::Add),
                op(Token::Minus, BinOp::Sub),
            ))
            .then(product)
            .repeated(),
            |left, (op, right)| Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

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
                    args: vec![
                        Expr::Var("x".to_string()),
                        Expr::Var("x".to_string()),
                    ],
                },
            })
        );
    }
}
