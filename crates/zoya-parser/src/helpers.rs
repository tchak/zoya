use chumsky::prelude::*;

use zoya_ast::{ModDecl, Path, PathPrefix};
use zoya_lexer::Token;

pub(crate) fn ident<'a>() -> impl Parser<'a, &'a [Token], String, extra::Err<Rich<'a, Token>>> + Clone
{
    select! { Token::Ident(name) => name }
}

/// Parse a path prefix: root::, self::, super::, or none
pub(crate) fn path_prefix_parser<'a>(
) -> impl Parser<'a, &'a [Token], PathPrefix, extra::Err<Rich<'a, Token>>> + Clone {
    choice((
        just(Token::Root)
            .then_ignore(just(Token::ColonColon))
            .to(PathPrefix::Root),
        just(Token::Self_)
            .then_ignore(just(Token::ColonColon))
            .to(PathPrefix::Self_),
        just(Token::Super)
            .then_ignore(just(Token::ColonColon))
            .to(PathPrefix::Super),
    ))
    .or_not()
    .map(|opt| opt.unwrap_or(PathPrefix::None))
}

/// Parse a simple path (no turbofish): prefix + ident segments
/// Returns a Path with type_args = None
pub(crate) fn simple_path_parser<'a>(
) -> impl Parser<'a, &'a [Token], Path, extra::Err<Rich<'a, Token>>> + Clone {
    path_prefix_parser()
        .then(
            ident()
                .separated_by(just(Token::ColonColon))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .map(|(prefix, segments)| Path {
            prefix,
            segments,
            type_args: None,
        })
}

pub(crate) fn mod_decl_parser<'a>(
) -> impl Parser<'a, &'a [Token], ModDecl, extra::Err<Rich<'a, Token>>> + Clone {
    just(Token::Mod)
        .ignore_then(ident())
        .map(|name| ModDecl { name })
}
