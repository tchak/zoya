use chumsky::prelude::*;

use zoya_ast::TypeAnnotation;
use zoya_lexer::Token;

use crate::helpers::simple_path_parser;

pub(crate) fn type_annotation<'a>(
) -> impl Parser<'a, &'a [Token], TypeAnnotation, extra::Err<Rich<'a, Token>>> + Clone {
    recursive(|type_ann| {
        // Type parameters: <T, U>
        let type_params = type_ann
            .clone()
            .separated_by(just(Token::Comma))
            .collect::<Vec<_>>()
            .delimited_by(just(Token::Lt), just(Token::Gt));

        // Named or parameterized type: Int, List<Int>, root::types::MyType<T>
        let named_type = simple_path_parser()
            .then(type_params.or_not())
            .map(|(path, params)| match params {
                Some(params) => TypeAnnotation::Parameterized(path, params),
                None => TypeAnnotation::Named(path),
            });

        // Empty tuple type: ()
        let empty_tuple_type = just(Token::LParen)
            .ignore_then(just(Token::RParen))
            .to(TypeAnnotation::Tuple(vec![]));

        // Parenthesized type: (T) for grouping, (T,) for single-element tuple, (T, U) for multi-element tuple
        let paren_type = just(Token::LParen)
            .ignore_then(type_ann.clone())
            .then(
                just(Token::Comma)
                    .ignore_then(
                        type_ann
                            .clone()
                            .separated_by(just(Token::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .or_not(),
            )
            .then_ignore(just(Token::RParen))
            .map(|(first, rest)| match rest {
                None => {
                    // (T) - parenthesized type for grouping (useful in function types)
                    first
                }
                Some(more) => {
                    // (T,) or (T1, T2, ...) - tuple type
                    TypeAnnotation::Tuple(std::iter::once(first).chain(more).collect())
                }
            });

        // Base type (before considering function arrow)
        let base_type = choice((empty_tuple_type, paren_type, named_type));

        // Function type: T -> U or (T, U) -> V
        // The arrow is right-associative: A -> B -> C = A -> (B -> C)
        // This is achieved by recursing into type_ann on the right side
        base_type
            .clone()
            .then(just(Token::Arrow).ignore_then(type_ann).or_not())
            .map(|(lhs, rhs)| match rhs {
                None => lhs,
                Some(ret) => {
                    // Convert LHS to parameter list
                    let params = match lhs {
                        TypeAnnotation::Tuple(elements) => elements,
                        other => vec![other],
                    };
                    TypeAnnotation::Function(params, Box::new(ret))
                }
            })
    })
}
