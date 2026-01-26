use chumsky::prelude::*;

use crate::ast::{
    BinOp, Expr, FunctionDef, Item, LetBinding, ListPattern, MatchArm, Param, Pattern, Statement,
    TuplePattern, TypeAnnotation, UnaryOp,
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
       + Clone {
    recursive(|type_ann| {
        // Type parameters: <T, U>
        let type_params = type_ann
            .clone()
            .separated_by(just(Token::Comma))
            .collect::<Vec<_>>()
            .delimited_by(just(Token::Lt), just(Token::Gt));

        // Named or parameterized type: Int32, List<Int32>
        let named_type = ident()
            .then(type_params.or_not())
            .map(|(name, params)| match params {
                Some(params) => TypeAnnotation::Parameterized(name, params),
                None => TypeAnnotation::Named(name),
            });

        // Empty tuple type: ()
        let empty_tuple_type = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .to(TypeAnnotation::Tuple(vec![]));

        // Tuple type: (T1, T2) or (T1,) - requires at least one comma
        let tuple_type = just(Token::LParen)
            .ignore_then(type_ann.clone())
            .then(
                just(Token::Comma)
                    .ignore_then(
                        type_ann
                            .separated_by(just(Token::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .or_not(),
            )
            .then_ignore(just(Token::RParen))
            .try_map(|(first, rest), span| match rest {
                None => {
                    // (T) - not a valid type annotation (use T directly)
                    Err(Rich::custom(
                        span,
                        "single type in parentheses is not valid; use the type directly or add a trailing comma for a single-element tuple",
                    ))
                }
                Some(mut more) => {
                    // (T,) or (T1, T2, ...) - tuple type
                    let mut elements = vec![first];
                    elements.append(&mut more);
                    Ok(TypeAnnotation::Tuple(elements))
                }
            });

        choice((empty_tuple_type, tuple_type, named_type))
    })
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

        // List literal: [expr, expr, ...]
        let list_literal = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(Expr::List);

        // Pattern for match arms (recursive for nested list patterns)
        let pattern = recursive(|pattern| {
            // Simple patterns (non-list)
            let simple_pattern = choice((
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
                // Variable (must be last among simple patterns)
                ident().map(Pattern::Var),
            ));

            // List pattern element: pattern or .. (rest marker)
            #[derive(Clone)]
            enum ListPatternElement {
                Pattern(Pattern),
                Rest, // ..
            }

            let list_element = choice((
                just(Token::DotDot).to(ListPatternElement::Rest),
                pattern.clone().map(ListPatternElement::Pattern),
            ));

            // List pattern: [], [a, b], [a, ..]
            let list_pattern = list_element
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .try_map(|elements, span| {
                    // Check for .. and convert to appropriate ListPattern
                    let rest_pos = elements.iter().position(|e| matches!(e, ListPatternElement::Rest));

                    match rest_pos {
                        None => {
                            // No .., this is an exact pattern
                            let patterns: Vec<Pattern> = elements
                                .into_iter()
                                .map(|e| match e {
                                    ListPatternElement::Pattern(p) => p,
                                    ListPatternElement::Rest => unreachable!(),
                                })
                                .collect();
                            if patterns.is_empty() {
                                Ok(Pattern::List(ListPattern::Empty))
                            } else {
                                Ok(Pattern::List(ListPattern::Exact(patterns)))
                            }
                        }
                        Some(pos) => {
                            // Multiple .. not allowed
                            if elements.iter().filter(|e| matches!(e, ListPatternElement::Rest)).count() > 1 {
                                return Err(Rich::custom(span, "only one .. allowed in list pattern"));
                            }

                            // Split into before and after ..
                            let before: Vec<Pattern> = elements[..pos]
                                .iter()
                                .filter_map(|e| match e {
                                    ListPatternElement::Pattern(p) => Some(p.clone()),
                                    ListPatternElement::Rest => None,
                                })
                                .collect();

                            let after: Vec<Pattern> = elements[pos + 1..]
                                .iter()
                                .filter_map(|e| match e {
                                    ListPatternElement::Pattern(p) => Some(p.clone()),
                                    ListPatternElement::Rest => None,
                                })
                                .collect();

                            if after.is_empty() {
                                // [a, b, ..] - prefix only
                                Ok(Pattern::List(ListPattern::Prefix(before)))
                            } else if before.is_empty() {
                                // [.., x, y] - suffix only
                                Ok(Pattern::List(ListPattern::Suffix(after)))
                            } else {
                                // [a, .., z] - prefix and suffix
                                Ok(Pattern::List(ListPattern::PrefixSuffix(before, after)))
                            }
                        }
                    }
                });

            // Tuple pattern element: pattern or .. (rest marker)
            #[derive(Clone)]
            enum TuplePatternElement {
                Pattern(Pattern),
                Rest, // ..
            }

            let tuple_element = choice((
                just(Token::DotDot).to(TuplePatternElement::Rest),
                pattern.clone().map(TuplePatternElement::Pattern),
            ));

            // Tuple pattern: (), (a,), (a, b), (a, ..)
            let tuple_pattern = tuple_element
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .try_map(|elements, span| {
                    // Check for .. and convert to appropriate TuplePattern
                    let rest_pos = elements
                        .iter()
                        .position(|e| matches!(e, TuplePatternElement::Rest));

                    // Check for multiple .. markers
                    if elements
                        .iter()
                        .filter(|e| matches!(e, TuplePatternElement::Rest))
                        .count()
                        > 1
                    {
                        return Err(Rich::custom(span, "only one .. allowed in tuple pattern"));
                    }

                    match rest_pos {
                        None => {
                            // No .., this is an exact pattern
                            let patterns: Vec<Pattern> = elements
                                .into_iter()
                                .map(|e| match e {
                                    TuplePatternElement::Pattern(p) => p,
                                    TuplePatternElement::Rest => unreachable!(),
                                })
                                .collect();
                            if patterns.is_empty() {
                                Ok(Pattern::Tuple(TuplePattern::Empty))
                            } else {
                                Ok(Pattern::Tuple(TuplePattern::Exact(patterns)))
                            }
                        }
                        Some(pos) => {
                            // Split into before and after ..
                            let before: Vec<Pattern> = elements[..pos]
                                .iter()
                                .filter_map(|e| match e {
                                    TuplePatternElement::Pattern(p) => Some(p.clone()),
                                    TuplePatternElement::Rest => None,
                                })
                                .collect();

                            let after: Vec<Pattern> = elements[pos + 1..]
                                .iter()
                                .filter_map(|e| match e {
                                    TuplePatternElement::Pattern(p) => Some(p.clone()),
                                    TuplePatternElement::Rest => None,
                                })
                                .collect();

                            if after.is_empty() {
                                // (a, b, ..) - prefix only
                                Ok(Pattern::Tuple(TuplePattern::Prefix(before)))
                            } else if before.is_empty() {
                                // (.., x, y) - suffix only
                                Ok(Pattern::Tuple(TuplePattern::Suffix(after)))
                            } else {
                                // (a, .., z) - prefix and suffix
                                Ok(Pattern::Tuple(TuplePattern::PrefixSuffix(before, after)))
                            }
                        }
                    }
                });

            choice((list_pattern, tuple_pattern, simple_pattern))
        });

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

        // Empty tuple: ()
        let empty_tuple = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .to(Expr::Tuple(vec![]));

        // Tuple or parenthesized expression: (expr) or (expr,) or (expr, expr, ...)
        // - (expr) with no comma is a parenthesized expression
        // - (expr,) with trailing comma is a single-element tuple
        // - (expr, expr, ...) is a multi-element tuple
        let paren_or_tuple = just(Token::LParen)
            .ignore_then(expr.clone())
            .then(
                just(Token::Comma)
                    .ignore_then(
                        expr.clone()
                            .separated_by(just(Token::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .or_not(),
            )
            .then_ignore(just(Token::RParen))
            .map(|(first, rest)| match rest {
                None => first, // (expr) - parenthesized expression
                Some(mut more) => {
                    // (expr,) or (expr, expr, ...) - tuple
                    let mut elements = vec![first];
                    elements.append(&mut more);
                    Expr::Tuple(elements)
                }
            });

        let atom = choice((
            literal,
            list_literal,
            match_expr,
            ident_expr,
            empty_tuple,
            paren_or_tuple,
        ));

        // Method calls: expr.method(args) - highest precedence postfix operator
        let method_args = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let postfix = atom.foldl(
            just(Token::Dot)
                .ignore_then(ident())
                .then(method_args)
                .repeated(),
            |receiver, (method, args)| Expr::MethodCall {
                receiver: Box::new(receiver),
                method,
                args,
            },
        );

        let unary = just(Token::Minus)
            .repeated()
            .foldr(postfix, |_, e| Expr::UnaryOp {
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
        if let Expr::MethodCall {
            receiver,
            method,
            args,
        } = expr
        {
            assert_eq!(method, "len");
            assert!(args.is_empty());
            assert!(matches!(
                *receiver,
                Expr::MethodCall {
                    method: ref m,
                    ..
                } if m == "to_uppercase"
            ));
        } else {
            panic!("expected method call");
        }
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
            } if matches!(*receiver, Expr::Var(ref name) if name == "s")
                && method == "trim"
                && args.is_empty()
        ));
    }

    #[test]
    fn test_parse_method_call_in_expression() {
        let expr = parse_str(r#""hello".len() + 1"#).unwrap();
        assert!(matches!(
            expr,
            Expr::BinOp {
                op: BinOp::Add,
                ..
            }
        ));
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
        assert_eq!(expr, Expr::List(vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)]));
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

    // List pattern tests
    #[test]
    fn test_parse_match_empty_list_pattern() {
        let expr = parse_str("match xs { [] => 0 }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(&arms[0].pattern, Pattern::List(ListPattern::Empty)));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_exact_list_pattern() {
        let expr = parse_str("match xs { [a, b] => a }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Exact(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "a"));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "b"));
            } else {
                panic!("expected exact list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_prefix_list_pattern() {
        let expr = parse_str("match xs { [head, ..] => head }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Prefix(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "head"));
            } else {
                panic!("expected prefix list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_list_pattern_with_literals() {
        let expr = parse_str("match xs { [1, x, ..] => x }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Prefix(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Literal(lit) if **lit == Expr::Int(1)));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "x"));
            } else {
                panic!("expected prefix list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_list_pattern_with_wildcard() {
        let expr = parse_str("match xs { [_, x] => x }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Exact(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Wildcard));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "x"));
            } else {
                panic!("expected exact list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_suffix_list_pattern() {
        let expr = parse_str("match xs { [.., last] => last }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Suffix(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "last"));
            } else {
                panic!("expected suffix list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_suffix_list_pattern_multiple() {
        let expr = parse_str("match xs { [.., x, y] => x }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Suffix(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "x"));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "y"));
            } else {
                panic!("expected suffix list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_prefix_suffix_list_pattern() {
        let expr = parse_str("match xs { [first, .., last] => first }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::PrefixSuffix(prefix, suffix)) = &arms[0].pattern {
                assert_eq!(prefix.len(), 1);
                assert_eq!(suffix.len(), 1);
                assert!(matches!(&prefix[0], Pattern::Var(s) if s == "first"));
                assert!(matches!(&suffix[0], Pattern::Var(s) if s == "last"));
            } else {
                panic!("expected prefix+suffix list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_prefix_suffix_multiple() {
        let expr = parse_str("match xs { [a, b, .., y, z] => a }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::PrefixSuffix(prefix, suffix)) = &arms[0].pattern {
                assert_eq!(prefix.len(), 2);
                assert_eq!(suffix.len(), 2);
                assert!(matches!(&prefix[0], Pattern::Var(s) if s == "a"));
                assert!(matches!(&prefix[1], Pattern::Var(s) if s == "b"));
                assert!(matches!(&suffix[0], Pattern::Var(s) if s == "y"));
                assert!(matches!(&suffix[1], Pattern::Var(s) if s == "z"));
            } else {
                panic!("expected prefix+suffix list pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    // Parameterized type annotation tests
    #[test]
    fn test_parse_function_with_list_param() {
        let item = parse_item_str("fn len(xs: List<Int32>) -> Int32 { 0 }").unwrap();
        let Item::Function(FunctionDef { params, .. }) = item;
        assert!(matches!(
            &params[0].typ,
            TypeAnnotation::Parameterized(name, args)
                if name == "List" && args.len() == 1
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
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::Tuple(TuplePattern::Exact(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "a"));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "b"));
            } else {
                panic!("expected exact tuple pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_tuple_pattern_prefix() {
        let expr = parse_str("match t { (a, ..) => a }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::Tuple(TuplePattern::Prefix(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "a"));
            } else {
                panic!("expected prefix tuple pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_tuple_pattern_suffix() {
        let expr = parse_str("match t { (.., z) => z }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::Tuple(TuplePattern::Suffix(patterns)) = &arms[0].pattern {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "z"));
            } else {
                panic!("expected suffix tuple pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_tuple_pattern_prefix_suffix() {
        let expr = parse_str("match t { (a, .., z) => a + z }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::Tuple(TuplePattern::PrefixSuffix(prefix, suffix)) = &arms[0].pattern {
                assert_eq!(prefix.len(), 1);
                assert_eq!(suffix.len(), 1);
                assert!(matches!(&prefix[0], Pattern::Var(s) if s == "a"));
                assert!(matches!(&suffix[0], Pattern::Var(s) if s == "z"));
            } else {
                panic!("expected prefix+suffix tuple pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_tuple_pattern_empty() {
        let expr = parse_str("match t { () => 0 }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(&arms[0].pattern, Pattern::Tuple(TuplePattern::Empty)));
        } else {
            panic!("expected match expression");
        }
    }
}
