use chumsky::prelude::*;

use zoya_ast::{Expr, ListPattern, Path, Pattern, StructFieldPattern, TuplePattern};
use zoya_lexer::Token;

use crate::helpers::{
    RestSplit, ident, path_prefix_parser, process_rest_elements, simple_path_parser,
};
use crate::types::type_annotation;

/// Pattern parser for match arms and let bindings
pub(crate) fn pattern_parser<'a>()
-> impl Parser<'a, &'a [Token], Pattern, extra::Err<Rich<'a, Token>>> + Clone {
    recursive(|pattern| {
        // Simple patterns (non-list, non-tuple)
        let simple_pattern = choice((
            // Wildcard: _ (must check before ident)
            select! { Token::Ident(s) if s == "_" => Pattern::Wildcard },
            // Literals
            select! {
                Token::Int(n) => Pattern::Literal(Box::new(Expr::Int(n))),
                Token::BigInt(n) => Pattern::Literal(Box::new(Expr::BigInt(n))),
                Token::Float(n) => Pattern::Literal(Box::new(Expr::Float(n))),
                Token::True => Pattern::Literal(Box::new(Expr::Bool(true))),
                Token::False => Pattern::Literal(Box::new(Expr::Bool(false))),
                Token::String(s) => Pattern::Literal(Box::new(Expr::String(s))),
            },
            // Single identifier as path (resolved later as variable or enum variant)
            ident().map(|name| Pattern::Path(Path::simple(name))),
        ));

        // List pattern element: pattern or .. (rest marker with optional binding)
        #[derive(Clone)]
        enum ListPatternElement {
            Pattern(Pattern),
            Rest(Option<String>), // .. or name @ ..
        }

        let list_element = choice((
            // name @ .. (rest with binding)
            ident()
                .then_ignore(just(Token::At))
                .then_ignore(just(Token::DotDot))
                .map(|name| ListPatternElement::Rest(Some(name))),
            // bare ..
            just(Token::DotDot).to(ListPatternElement::Rest(None)),
            pattern.clone().map(ListPatternElement::Pattern),
        ));

        // List pattern: [], [a, b], [a, ..], [a, rest @ ..]
        let list_pattern = list_element
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .try_map(|elements, span| {
                let is_rest = |e: &ListPatternElement| match e {
                    ListPatternElement::Rest(name) => Some(name.clone()),
                    ListPatternElement::Pattern(_) => None,
                };
                let extract_pattern = |e: ListPatternElement| match e {
                    ListPatternElement::Pattern(p) => p,
                    // SAFETY: process_rest_elements filters out Rest elements
                    ListPatternElement::Rest(_) => unreachable!(),
                };

                match process_rest_elements(elements, is_rest, span, "list")? {
                    RestSplit::Exact(elements) => {
                        let patterns: Vec<Pattern> =
                            elements.into_iter().map(extract_pattern).collect();
                        if patterns.is_empty() {
                            Ok(Pattern::List(ListPattern::Empty))
                        } else {
                            Ok(Pattern::List(ListPattern::Exact(patterns)))
                        }
                    }
                    RestSplit::WithRest {
                        prefix,
                        rest_binding,
                        suffix,
                    } => {
                        let prefix: Vec<Pattern> =
                            prefix.into_iter().map(extract_pattern).collect();
                        let suffix: Vec<Pattern> =
                            suffix.into_iter().map(extract_pattern).collect();

                        if suffix.is_empty() {
                            Ok(Pattern::List(ListPattern::Prefix {
                                patterns: prefix,
                                rest_binding,
                            }))
                        } else if prefix.is_empty() {
                            Ok(Pattern::List(ListPattern::Suffix {
                                patterns: suffix,
                                rest_binding,
                            }))
                        } else {
                            Ok(Pattern::List(ListPattern::PrefixSuffix {
                                prefix,
                                suffix,
                                rest_binding,
                            }))
                        }
                    }
                }
            });

        // Tuple pattern element: pattern or .. (rest marker with optional binding)
        #[derive(Clone)]
        enum TuplePatternElement {
            Pattern(Pattern),
            Rest(Option<String>), // .. or name @ ..
        }

        let tuple_element = choice((
            // name @ .. (rest with binding)
            ident()
                .then_ignore(just(Token::At))
                .then_ignore(just(Token::DotDot))
                .map(|name| TuplePatternElement::Rest(Some(name))),
            // bare ..
            just(Token::DotDot).to(TuplePatternElement::Rest(None)),
            pattern.clone().map(TuplePatternElement::Pattern),
        ));

        // Tuple pattern: (), (a,), (a, b), (a, ..), (a, rest @ ..)
        let tuple_pattern = tuple_element
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .try_map(|elements, span| {
                let is_rest = |e: &TuplePatternElement| match e {
                    TuplePatternElement::Rest(name) => Some(name.clone()),
                    TuplePatternElement::Pattern(_) => None,
                };
                let extract_pattern = |e: TuplePatternElement| match e {
                    TuplePatternElement::Pattern(p) => p,
                    // SAFETY: process_rest_elements filters out Rest elements
                    TuplePatternElement::Rest(_) => unreachable!(),
                };

                match process_rest_elements(elements, is_rest, span, "tuple")? {
                    RestSplit::Exact(elements) => {
                        let patterns: Vec<Pattern> =
                            elements.into_iter().map(extract_pattern).collect();
                        if patterns.is_empty() {
                            Ok(Pattern::Tuple(TuplePattern::Empty))
                        } else {
                            Ok(Pattern::Tuple(TuplePattern::Exact(patterns)))
                        }
                    }
                    RestSplit::WithRest {
                        prefix,
                        rest_binding,
                        suffix,
                    } => {
                        let prefix: Vec<Pattern> =
                            prefix.into_iter().map(extract_pattern).collect();
                        let suffix: Vec<Pattern> =
                            suffix.into_iter().map(extract_pattern).collect();

                        if suffix.is_empty() {
                            Ok(Pattern::Tuple(TuplePattern::Prefix {
                                patterns: prefix,
                                rest_binding,
                            }))
                        } else if prefix.is_empty() {
                            Ok(Pattern::Tuple(TuplePattern::Suffix {
                                patterns: suffix,
                                rest_binding,
                            }))
                        } else {
                            Ok(Pattern::Tuple(TuplePattern::PrefixSuffix {
                                prefix,
                                suffix,
                                rest_binding,
                            }))
                        }
                    }
                }
            });

        // Struct pattern field: `x` (shorthand for x: x) or `x: pattern`
        #[derive(Clone)]
        enum StructPatternField {
            Field(StructFieldPattern),
            Rest, // ..
        }

        let struct_field_pattern = choice((
            // Error on name @ .. in struct patterns (not allowed)
            ident()
                .then_ignore(just(Token::At))
                .then_ignore(just(Token::DotDot))
                .try_map(|_, span| {
                    Err(Rich::custom(
                        span,
                        "@ binding not allowed on struct rest pattern (..)",
                    ))
                }),
            just(Token::DotDot).to(StructPatternField::Rest),
            // Field with binding: x: pattern
            ident()
                .then(just(Token::Colon).ignore_then(pattern.clone()).or_not())
                .map(|(field_name, pat)| {
                    let binding_pattern =
                        pat.unwrap_or_else(|| Pattern::Path(Path::simple(field_name.clone())));
                    StructPatternField::Field(StructFieldPattern {
                        field_name,
                        pattern: Box::new(binding_pattern),
                    })
                }),
        ));

        // Turbofish type arguments in patterns: ::<Int, String>
        let pattern_turbofish = just(Token::ColonColon).ignore_then(
            type_annotation()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::Lt), just(Token::Gt)),
        );

        // Helper to parse struct field patterns with rest support
        let struct_fields_parser = struct_field_pattern
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .try_map(|elements, span| {
                let has_rest = elements
                    .iter()
                    .any(|e| matches!(e, StructPatternField::Rest));

                if elements
                    .iter()
                    .filter(|e| matches!(e, StructPatternField::Rest))
                    .count()
                    > 1
                {
                    return Err(Rich::custom(span, "only one .. allowed in struct pattern"));
                }

                let fields: Vec<StructFieldPattern> = elements
                    .into_iter()
                    .filter_map(|e| match e {
                        StructPatternField::Field(f) => Some(f),
                        StructPatternField::Rest => None,
                    })
                    .collect();

                Ok((fields, has_rest))
            });

        // Helper to parse tuple pattern arguments (for call patterns)
        let call_args_parser = tuple_element
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .try_map(|elements, span| {
                let is_rest = |e: &TuplePatternElement| match e {
                    TuplePatternElement::Rest(name) => Some(name.clone()),
                    TuplePatternElement::Pattern(_) => None,
                };
                let extract_pattern = |e: TuplePatternElement| match e {
                    TuplePatternElement::Pattern(p) => p,
                    // SAFETY: process_rest_elements filters out Rest elements
                    TuplePatternElement::Rest(_) => unreachable!(),
                };

                match process_rest_elements(elements, is_rest, span, "call")? {
                    RestSplit::Exact(elements) => {
                        let patterns: Vec<Pattern> =
                            elements.into_iter().map(extract_pattern).collect();
                        if patterns.is_empty() {
                            Ok(TuplePattern::Empty)
                        } else {
                            Ok(TuplePattern::Exact(patterns))
                        }
                    }
                    RestSplit::WithRest {
                        prefix,
                        rest_binding,
                        suffix,
                    } => {
                        let prefix: Vec<Pattern> =
                            prefix.into_iter().map(extract_pattern).collect();
                        let suffix: Vec<Pattern> =
                            suffix.into_iter().map(extract_pattern).collect();

                        if suffix.is_empty() {
                            Ok(TuplePattern::Prefix {
                                patterns: prefix,
                                rest_binding,
                            })
                        } else if prefix.is_empty() {
                            Ok(TuplePattern::Suffix {
                                patterns: suffix,
                                rest_binding,
                            })
                        } else {
                            Ok(TuplePattern::PrefixSuffix {
                                prefix,
                                suffix,
                                rest_binding,
                            })
                        }
                    }
                }
            });

        // Struct pattern: Point { x }, types::Point { x, .. }, Msg::Move { x }
        // Works for both struct types and enum struct variants
        let struct_pattern = simple_path_parser().then(struct_fields_parser.clone()).map(
            |(path, (fields, is_partial))| Pattern::Struct {
                path,
                fields,
                is_partial,
            },
        );

        // Call pattern: Some(x), Option::Some(x), root::Result::Ok(v, ..)
        // Path (1+ segments) followed by parenthesized args - parens disambiguate from variables
        let call_pattern = path_prefix_parser()
            .then(
                ident()
                    .separated_by(just(Token::ColonColon))
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then(pattern_turbofish.clone().or_not())
            .then(call_args_parser)
            .map(|(((prefix, segments), type_args), args)| {
                let path = Path {
                    prefix,
                    segments,
                    type_args,
                };
                Pattern::Call { path, args }
            });

        // Path pattern: Option::None, root::Color::Red (qualified path, no suffix)
        // Must have 2+ segments OR have a turbofish to be a path pattern (not a variable)
        let path_pattern = path_prefix_parser()
            .then(
                ident()
                    .separated_by(just(Token::ColonColon))
                    .at_least(2)
                    .collect::<Vec<_>>(),
            )
            .then(pattern_turbofish.or_not())
            .map(|((prefix, segments), type_args)| {
                let path = Path {
                    prefix,
                    segments,
                    type_args,
                };
                Pattern::Path(path)
            });

        // As pattern: name @ pattern (binds entire matched value to name)
        // Note: name @ .. is handled separately in list/tuple element parsing
        let as_pattern = ident()
            .then_ignore(just(Token::At))
            .then(choice((
                list_pattern.clone(),
                tuple_pattern.clone(),
                call_pattern.clone(),
                struct_pattern.clone(),
                path_pattern.clone(),
                simple_pattern.clone(),
            )))
            .map(|(name, inner)| Pattern::As {
                name,
                pattern: Box::new(inner),
            });

        // Order matters:
        // - call_pattern must come before path_pattern (both require 2+ segments, but call has parens)
        // - path_pattern must come before struct_pattern (path has no suffix)
        // - struct_pattern has braces
        // - as_pattern must come before simple_pattern to capture name @ ...
        choice((
            list_pattern,
            tuple_pattern,
            call_pattern,
            struct_pattern,
            path_pattern,
            as_pattern,
            simple_pattern,
        ))
    })
}
