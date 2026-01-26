use chumsky::prelude::*;

use crate::ast::{BinOp, Expr, UnaryOp};
use crate::lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
}

pub fn parse(tokens: Vec<Token>) -> Result<Expr, ParseError> {
    parser()
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

fn parser<'a>() -> impl Parser<'a, &'a [Token], Expr, extra::Err<Rich<'a, Token>>> {
    recursive(|expr| {
        let literal = select! {
            Token::Int(n) => Expr::Int(n),
            Token::Float(n) => Expr::Float(n),
        };

        let atom = literal.or(expr
            .delimited_by(just(Token::LParen), just(Token::RParen)));

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
}
