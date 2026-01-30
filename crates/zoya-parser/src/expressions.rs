use chumsky::prelude::*;

use zoya_ast::{BinOp, Expr, LambdaParam, LetBinding, MatchArm, Path, UnaryOp};
use zoya_lexer::Token;

use crate::helpers::{ident, path_prefix_parser, validate_typed_pattern};
use crate::patterns::pattern_parser;
use crate::types::type_annotation;

pub(crate) fn expr_parser<'a>(
) -> impl Parser<'a, &'a [Token], Expr, extra::Err<Rich<'a, Token>>> + Clone {
    recursive(|expr| {
        let literal = select! {
            Token::Int(n) => Expr::Int(n),
            Token::BigInt(n) => Expr::BigInt(n),
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

        // Use the shared pattern parser for match arms
        let pattern = pattern_parser();

        // Let binding for use in match arm blocks (uses expr from recursive context)
        let let_in_arm = just(Token::Let)
            .ignore_then(pattern_parser())
            .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .try_map(|((pattern, type_annotation), value), span| {
                validate_typed_pattern(&pattern, &type_annotation, span)?;
                Ok(LetBinding {
                    pattern,
                    type_annotation,
                    value: Box::new(value),
                })
            });

        // Arm body: { [let x = e;]* expr } OR expr
        let arm_body = choice((
            // Braced body (block or simple expression)
            just(Token::LBrace)
                .ignore_then(
                    let_in_arm
                        .then_ignore(just(Token::Semicolon))
                        .repeated()
                        .collect::<Vec<_>>(),
                )
                .then(expr.clone())
                .then_ignore(just(Token::RBrace))
                .map(|(bindings, result)| {
                    if bindings.is_empty() {
                        result // { expr } -> just the expression
                    } else {
                        Expr::Block {
                            bindings,
                            result: Box::new(result),
                        }
                    }
                }),
            // Non-braced expression (unchanged)
            expr.clone(),
        ));

        // Match arm: pattern => arm_body
        let match_arm = pattern
            .then_ignore(just(Token::FatArrow))
            .then(arm_body)
            .map(|(pattern, result)| MatchArm { pattern, result });

        // Match expression: match scrutinee { arms }
        // Commas required between arms, trailing comma allowed
        let match_expr = just(Token::Match)
            .ignore_then(expr.clone())
            .then(
                match_arm
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
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

        // Struct constructor field: `x: expr` or `x` (shorthand for x: x)
        let struct_field = ident()
            .then(just(Token::Colon).ignore_then(expr.clone()).or_not())
            .map(|(name, value)| {
                let value = value.unwrap_or_else(|| Expr::Path(Path::simple(name.clone())));
                (name, value)
            });

        // Struct constructor fields: { x: expr, y: expr }
        let struct_fields = struct_field
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace));

        // Turbofish type arguments: ::<Int, String>
        let turbofish = just(Token::ColonColon).ignore_then(
            type_annotation()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::Lt), just(Token::Gt)),
        );

        // Path parser: `foo` or `Foo::Bar` or `Option::None::<Int>` or `root::utils::foo`
        let path = path_prefix_parser()
            .then(
                ident()
                    .separated_by(just(Token::ColonColon))
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then(turbofish.or_not())
            .map(|((prefix, segments), type_args)| Path {
                prefix,
                segments,
                type_args,
            });

        // What can follow a path
        #[derive(Clone)]
        enum PathSuffix {
            Call(Vec<Expr>),
            Struct(Vec<(String, Expr)>),
        }

        // Path expression: variable, function call, struct/enum constructor
        let path_expr = path
            .then(
                choice((
                    args.clone().map(PathSuffix::Call),
                    struct_fields.clone().map(PathSuffix::Struct),
                ))
                .or_not(),
            )
            .map(|(path, suffix)| match suffix {
                Some(PathSuffix::Call(args)) => Expr::Call { path, args },
                Some(PathSuffix::Struct(fields)) => Expr::Struct { path, fields },
                None => Expr::Path(path),
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
                Some(more) => {
                    // (expr,) or (expr, expr, ...) - tuple
                    Expr::Tuple(std::iter::once(first).chain(more).collect())
                }
            });

        // Lambda parameter: name or name: Type
        let lambda_param = pattern_parser()
            .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
            .map(|(pattern, typ)| LambdaParam { pattern, typ });

        // Lambda parameters: |x| or |x, y| or |x: Int|
        let lambda_params = lambda_param
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::Pipe), just(Token::Pipe));

        // Lambda return type: -> Type (optional)
        let lambda_return_type = just(Token::Arrow).ignore_then(type_annotation()).or_not();

        // Lambda body: { [let x = e;]* expr } OR expr
        // Note: we need to define let_in_lambda fresh here because let_in_arm
        // was already moved when defining arm_body
        let let_in_lambda = just(Token::Let)
            .ignore_then(pattern_parser())
            .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .try_map(|((pattern, type_annotation), value), span| {
                validate_typed_pattern(&pattern, &type_annotation, span)?;
                Ok(LetBinding {
                    pattern,
                    type_annotation,
                    value: Box::new(value),
                })
            });

        let lambda_body = choice((
            // Braced body (block or simple expression)
            just(Token::LBrace)
                .ignore_then(
                    let_in_lambda
                        .then_ignore(just(Token::Semicolon))
                        .repeated()
                        .collect::<Vec<_>>(),
                )
                .then(expr.clone())
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
                }),
            // Non-braced expression (simple body)
            expr.clone(),
        ));

        // Lambda expression: |params| [-> Type] body
        let lambda = lambda_params
            .then(lambda_return_type)
            .then(lambda_body)
            .map(|((params, return_type), body)| Expr::Lambda {
                params,
                return_type,
                body: Box::new(body),
            });

        let atom = choice((
            lambda,
            literal,
            list_literal,
            match_expr,
            path_expr,
            empty_tuple,
            paren_or_tuple,
        ));

        // Method calls and field access: expr.method(args) or expr.field
        let method_args = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // What follows .ident: either (args) for method call, or nothing for field access
        #[derive(Clone)]
        enum DotSuffix {
            MethodCall(String, Vec<Expr>),
            FieldAccess(String),
        }

        let dot_suffix = just(Token::Dot)
            .ignore_then(ident())
            .then(method_args.or_not())
            .map(|(name, args)| match args {
                Some(args) => DotSuffix::MethodCall(name, args),
                None => DotSuffix::FieldAccess(name),
            });

        let postfix = atom.foldl(dot_suffix.repeated(), |receiver, suffix| match suffix {
            DotSuffix::MethodCall(method, args) => Expr::MethodCall {
                receiver: Box::new(receiver),
                method,
                args,
            },
            DotSuffix::FieldAccess(field) => Expr::FieldAccess {
                expr: Box::new(receiver),
                field,
            },
        });

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
