use chumsky::prelude::*;

use zoya_ast::{LetBinding, Stmt};
use zoya_lexer::Token;

use crate::expressions::expr_parser;
use crate::helpers::validate_typed_pattern;
use crate::patterns::pattern_parser;
use crate::types::type_annotation;

pub(crate) fn let_binding_parser<'a>(
) -> impl Parser<'a, &'a [Token], LetBinding, extra::Err<Rich<'a, Token>>> + Clone {
    just(Token::Let)
        .ignore_then(pattern_parser())
        .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr_parser())
        .try_map(|((pattern, type_annotation), value), span| {
            validate_typed_pattern(&pattern, &type_annotation, span)?;
            Ok(LetBinding {
                pattern,
                type_annotation,
                value: Box::new(value),
            })
        })
}

pub(crate) fn stmt_parser<'a>(
) -> impl Parser<'a, &'a [Token], Stmt, extra::Err<Rich<'a, Token>>> + Clone {
    // Parse let binding or expression
    choice((
        let_binding_parser().map(Stmt::Let),
        expr_parser().map(Stmt::Expr),
    ))
}
