use chumsky::prelude::*;

use crate::ast::{
    BinOp, EnumDef, EnumPattern, EnumPatternFields, EnumVariant, EnumVariantKind, Expr,
    FunctionDef, Item, LambdaParam, LetBinding, ListPattern, MatchArm, Param, Path, Pattern,
    Statement, StructDef, StructFieldDef, StructFieldPattern, StructPattern, TuplePattern,
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
       + Clone {
    recursive(|type_ann| {
        // Type parameters: <T, U>
        let type_params = type_ann
            .clone()
            .separated_by(just(Token::Comma))
            .collect::<Vec<_>>()
            .delimited_by(just(Token::Lt), just(Token::Gt));

        // Named or parameterized type: Int, List<Int>
        let named_type = ident()
            .then(type_params.or_not())
            .map(|(name, params)| match params {
                Some(params) => TypeAnnotation::Parameterized(Path::simple(name), params),
                None => TypeAnnotation::Named(Path::simple(name)),
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
                Some(mut more) => {
                    // (T,) or (T1, T2, ...) - tuple type
                    let mut elements = vec![first];
                    elements.append(&mut more);
                    TypeAnnotation::Tuple(elements)
                }
            });

        // Base type (before considering function arrow)
        let base_type = choice((empty_tuple_type, paren_type, named_type));

        // Function type: T -> U or (T, U) -> V
        // The arrow is right-associative: A -> B -> C = A -> (B -> C)
        // This is achieved by recursing into type_ann on the right side
        base_type.clone().then(
            just(Token::Arrow).ignore_then(type_ann).or_not()
        ).map(|(lhs, rhs)| {
            match rhs {
                None => lhs,
                Some(ret) => {
                    // Convert LHS to parameter list
                    let params = match lhs {
                        TypeAnnotation::Tuple(elements) => elements,
                        other => vec![other],
                    };
                    TypeAnnotation::Function(params, Box::new(ret))
                }
            }
        })
    })
}

/// Pattern parser for match arms and let bindings
fn pattern_parser<'a>() -> impl Parser<'a, &'a [Token], Pattern, extra::Err<Rich<'a, Token>>> + Clone
{
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
            // Variable (must be last among simple patterns)
            ident().map(Pattern::Var),
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
                // Check for .. and convert to appropriate ListPattern
                let rest_pos = elements
                    .iter()
                    .position(|e| matches!(e, ListPatternElement::Rest(_)));

                match rest_pos {
                    None => {
                        // No .., this is an exact pattern
                        let patterns: Vec<Pattern> = elements
                            .into_iter()
                            .map(|e| match e {
                                ListPatternElement::Pattern(p) => p,
                                ListPatternElement::Rest(_) => unreachable!(),
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
                        if elements
                            .iter()
                            .filter(|e| matches!(e, ListPatternElement::Rest(_)))
                            .count()
                            > 1
                        {
                            return Err(Rich::custom(span, "only one .. allowed in list pattern"));
                        }

                        // Extract rest binding name
                        let rest_binding = match &elements[pos] {
                            ListPatternElement::Rest(name) => name.clone(),
                            _ => unreachable!(),
                        };

                        // Split into before and after ..
                        let before: Vec<Pattern> = elements[..pos]
                            .iter()
                            .filter_map(|e| match e {
                                ListPatternElement::Pattern(p) => Some(p.clone()),
                                ListPatternElement::Rest(_) => None,
                            })
                            .collect();

                        let after: Vec<Pattern> = elements[pos + 1..]
                            .iter()
                            .filter_map(|e| match e {
                                ListPatternElement::Pattern(p) => Some(p.clone()),
                                ListPatternElement::Rest(_) => None,
                            })
                            .collect();

                        if after.is_empty() {
                            // [a, b, ..] or [a, b, rest @ ..] - prefix only
                            Ok(Pattern::List(ListPattern::Prefix {
                                patterns: before,
                                rest_binding,
                            }))
                        } else if before.is_empty() {
                            // [.., x, y] or [rest @ .., x, y] - suffix only
                            Ok(Pattern::List(ListPattern::Suffix {
                                patterns: after,
                                rest_binding,
                            }))
                        } else {
                            // [a, .., z] or [a, rest @ .., z] - prefix and suffix
                            Ok(Pattern::List(ListPattern::PrefixSuffix {
                                prefix: before,
                                suffix: after,
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
                // Check for .. and convert to appropriate TuplePattern
                let rest_pos = elements
                    .iter()
                    .position(|e| matches!(e, TuplePatternElement::Rest(_)));

                // Check for multiple .. markers
                if elements
                    .iter()
                    .filter(|e| matches!(e, TuplePatternElement::Rest(_)))
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
                                TuplePatternElement::Rest(_) => unreachable!(),
                            })
                            .collect();
                        if patterns.is_empty() {
                            Ok(Pattern::Tuple(TuplePattern::Empty))
                        } else {
                            Ok(Pattern::Tuple(TuplePattern::Exact(patterns)))
                        }
                    }
                    Some(pos) => {
                        // Extract rest binding name
                        let rest_binding = match &elements[pos] {
                            TuplePatternElement::Rest(name) => name.clone(),
                            _ => unreachable!(),
                        };

                        // Split into before and after ..
                        let before: Vec<Pattern> = elements[..pos]
                            .iter()
                            .filter_map(|e| match e {
                                TuplePatternElement::Pattern(p) => Some(p.clone()),
                                TuplePatternElement::Rest(_) => None,
                            })
                            .collect();

                        let after: Vec<Pattern> = elements[pos + 1..]
                            .iter()
                            .filter_map(|e| match e {
                                TuplePatternElement::Pattern(p) => Some(p.clone()),
                                TuplePatternElement::Rest(_) => None,
                            })
                            .collect();

                        if after.is_empty() {
                            // (a, b, ..) or (a, b, rest @ ..) - prefix only
                            Ok(Pattern::Tuple(TuplePattern::Prefix {
                                patterns: before,
                                rest_binding,
                            }))
                        } else if before.is_empty() {
                            // (.., x, y) or (rest @ .., x, y) - suffix only
                            Ok(Pattern::Tuple(TuplePattern::Suffix {
                                patterns: after,
                                rest_binding,
                            }))
                        } else {
                            // (a, .., z) or (a, rest @ .., z) - prefix and suffix
                            Ok(Pattern::Tuple(TuplePattern::PrefixSuffix {
                                prefix: before,
                                suffix: after,
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
                    let binding_pattern = pat.unwrap_or_else(|| Pattern::Var(field_name.clone()));
                    StructPatternField::Field(StructFieldPattern {
                        field_name,
                        pattern: Box::new(binding_pattern),
                    })
                }),
        ));

        // Struct pattern: Point { x, y }, Point { x: a, .. }
        let struct_pattern = ident()
            .then(
                struct_field_pattern
                    .clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .try_map(|(name, elements), span| {
                // Check for .. (rest marker)
                let has_rest = elements.iter().any(|e| matches!(e, StructPatternField::Rest));

                // Multiple .. not allowed
                if elements
                    .iter()
                    .filter(|e| matches!(e, StructPatternField::Rest))
                    .count()
                    > 1
                {
                    return Err(Rich::custom(span, "only one .. allowed in struct pattern"));
                }

                // Extract field patterns (exclude ..)
                let fields: Vec<StructFieldPattern> = elements
                    .into_iter()
                    .filter_map(|e| match e {
                        StructPatternField::Field(f) => Some(f),
                        StructPatternField::Rest => None,
                    })
                    .collect();

                let path = Path::simple(name);
                if has_rest {
                    Ok(Pattern::Struct(StructPattern::Partial { path, fields }))
                } else {
                    Ok(Pattern::Struct(StructPattern::Exact { path, fields }))
                }
            });

        // Enum pattern: Enum::Variant, Enum::Variant(patterns), Enum::Variant { fields }
        // Reuses TuplePatternElement and StructPatternField for rest pattern support

        // Tuple variant pattern fields (with rest support)
        let enum_tuple_pattern_fields = tuple_element
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .try_map(|elements, span| {
                let rest_pos = elements
                    .iter()
                    .position(|e| matches!(e, TuplePatternElement::Rest(_)));

                if elements
                    .iter()
                    .filter(|e| matches!(e, TuplePatternElement::Rest(_)))
                    .count()
                    > 1
                {
                    return Err(Rich::custom(span, "only one .. allowed in enum tuple pattern"));
                }

                match rest_pos {
                    None => {
                        let patterns: Vec<Pattern> = elements
                            .into_iter()
                            .map(|e| match e {
                                TuplePatternElement::Pattern(p) => p,
                                TuplePatternElement::Rest(_) => unreachable!(),
                            })
                            .collect();
                        if patterns.is_empty() {
                            Ok(EnumPatternFields::Tuple(TuplePattern::Empty))
                        } else {
                            Ok(EnumPatternFields::Tuple(TuplePattern::Exact(patterns)))
                        }
                    }
                    Some(pos) => {
                        // Extract rest binding name
                        let rest_binding = match &elements[pos] {
                            TuplePatternElement::Rest(name) => name.clone(),
                            _ => unreachable!(),
                        };

                        let before: Vec<Pattern> = elements[..pos]
                            .iter()
                            .filter_map(|e| match e {
                                TuplePatternElement::Pattern(p) => Some(p.clone()),
                                TuplePatternElement::Rest(_) => None,
                            })
                            .collect();

                        let after: Vec<Pattern> = elements[pos + 1..]
                            .iter()
                            .filter_map(|e| match e {
                                TuplePatternElement::Pattern(p) => Some(p.clone()),
                                TuplePatternElement::Rest(_) => None,
                            })
                            .collect();

                        if after.is_empty() {
                            Ok(EnumPatternFields::Tuple(TuplePattern::Prefix {
                                patterns: before,
                                rest_binding,
                            }))
                        } else if before.is_empty() {
                            Ok(EnumPatternFields::Tuple(TuplePattern::Suffix {
                                patterns: after,
                                rest_binding,
                            }))
                        } else {
                            Ok(EnumPatternFields::Tuple(TuplePattern::PrefixSuffix {
                                prefix: before,
                                suffix: after,
                                rest_binding,
                            }))
                        }
                    }
                }
            });

        // Struct variant pattern fields (with rest support)
        let enum_struct_pattern_fields = struct_field_pattern
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .try_map(|elements, span| {
                let has_rest = elements.iter().any(|e| matches!(e, StructPatternField::Rest));

                if elements
                    .iter()
                    .filter(|e| matches!(e, StructPatternField::Rest))
                    .count()
                    > 1
                {
                    return Err(Rich::custom(
                        span,
                        "only one .. allowed in enum struct pattern",
                    ));
                }

                let fields: Vec<StructFieldPattern> = elements
                    .into_iter()
                    .filter_map(|e| match e {
                        StructPatternField::Field(f) => Some(f),
                        StructPatternField::Rest => None,
                    })
                    .collect();

                Ok(EnumPatternFields::Struct {
                    fields,
                    is_partial: has_rest,
                })
            });

        // Turbofish type arguments in patterns: ::<Int, String>
        let pattern_turbofish = just(Token::ColonColon).ignore_then(
            type_annotation()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::Lt), just(Token::Gt)),
        );

        // Enum pattern: Enum::Variant, Enum::Variant::<T>, Enum::Variant(x), Enum::Variant { x }
        let enum_pattern = ident()
            .then_ignore(just(Token::ColonColon))
            .then(ident())
            .then(pattern_turbofish.or_not())
            .then(
                choice((enum_tuple_pattern_fields, enum_struct_pattern_fields)).or_not(),
            )
            .map(|(((enum_name, variant_name), type_args), fields)| {
                let fields = fields.unwrap_or(EnumPatternFields::Unit);
                let path = Path {
                    segments: vec![enum_name, variant_name],
                    type_args,
                };
                Pattern::Enum(EnumPattern { path, fields })
            });

        // As pattern: name @ pattern (binds entire matched value to name)
        // Note: name @ .. is handled separately in list/tuple element parsing
        let as_pattern = ident()
            .then_ignore(just(Token::At))
            .then(choice((
                list_pattern.clone(),
                tuple_pattern.clone(),
                enum_pattern.clone(),
                struct_pattern.clone(),
                simple_pattern.clone(),
            )))
            .map(|(name, inner)| Pattern::As {
                name,
                pattern: Box::new(inner),
            });

        // enum_pattern must come before struct_pattern to match :: first
        // as_pattern must come before simple_pattern to capture name @ ...
        choice((
            list_pattern,
            tuple_pattern,
            enum_pattern,
            struct_pattern,
            as_pattern,
            simple_pattern,
        ))
    })
}

fn let_binding_parser<'a>(
) -> impl Parser<'a, &'a [Token], LetBinding, extra::Err<Rich<'a, Token>>> {
    just(Token::Let)
        .ignore_then(pattern_parser())
        .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr_parser())
        .try_map(|((pattern, type_annotation), value), span| {
            // Type annotation only allowed on simple variable patterns
            if type_annotation.is_some() && !matches!(pattern, Pattern::Var(_)) {
                return Err(Rich::custom(
                    span,
                    "type annotations are only allowed on simple variable patterns",
                ));
            }
            Ok(LetBinding {
                pattern,
                type_annotation,
                value: Box::new(value),
            })
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

    // Body: { [let x = e;]* expr } OR expr
    let body = choice((
        // Braced body (block or simple expression)
        just(Token::LBrace)
            .ignore_then(
                let_binding_parser()
                    .then_ignore(just(Token::Semicolon))
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
            }),
        // Non-braced expression (simple body)
        expr_parser(),
    ));

    // fn name<T>(params) -> ReturnType { body }
    let function_def = just(Token::Fn)
        .ignore_then(ident())
        .then(type_params.clone())
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
        });

    // Struct field: name: Type
    let struct_field = ident()
        .then_ignore(just(Token::Colon))
        .then(type_annotation())
        .map(|(name, typ)| StructFieldDef { name, typ });

    // Struct fields: { field: Type, ... }
    let struct_fields = struct_field
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    // struct Name<T> { field: Type, ... }
    let struct_def = just(Token::Struct)
        .ignore_then(ident())
        .then(type_params.clone())
        .then(struct_fields.clone())
        .map(|((name, type_params), fields)| {
            Item::Struct(StructDef {
                name,
                type_params,
                fields,
            })
        });

    // Enum variant: Unit, Tuple(T, U), or Struct { field: Type }
    // Try struct variant first (has braces), then tuple (has parens), then unit
    let enum_variant_struct = ident()
        .then(struct_fields.clone())
        .map(|(name, fields)| EnumVariant {
            name,
            kind: EnumVariantKind::Struct(fields),
        });

    let enum_variant_tuple = ident()
        .then(
            type_annotation()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .map(|(name, types)| EnumVariant {
            name,
            kind: EnumVariantKind::Tuple(types),
        });

    let enum_variant_unit = ident().map(|name| EnumVariant {
        name,
        kind: EnumVariantKind::Unit,
    });

    let enum_variant = choice((enum_variant_struct, enum_variant_tuple, enum_variant_unit));

    let enum_variants = enum_variant
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    // enum Name<T> { Variant, Variant(T), Variant { field: Type } }
    let enum_def = just(Token::Enum)
        .ignore_then(ident())
        .then(type_params)
        .then(enum_variants)
        .map(|((name, type_params), variants)| {
            Item::Enum(EnumDef {
                name,
                type_params,
                variants,
            })
        });

    choice((function_def, struct_def, enum_def))
}

fn expr_parser<'a>() -> impl Parser<'a, &'a [Token], Expr, extra::Err<Rich<'a, Token>>> {
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
                // Type annotation only allowed on simple variable patterns
                if type_annotation.is_some() && !matches!(pattern, Pattern::Var(_)) {
                    return Err(Rich::custom(
                        span,
                        "type annotations are only allowed on simple variable patterns",
                    ));
                }
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
                        result // { expr } → just the expression
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

        // Path parser: `foo` or `Foo::Bar` or `Option::None::<Int>`
        let path = ident()
            .separated_by(just(Token::ColonColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .then(turbofish.or_not())
            .map(|(segments, type_args)| Path { segments, type_args });

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
                Some(mut more) => {
                    // (expr,) or (expr, expr, ...) - tuple
                    let mut elements = vec![first];
                    elements.append(&mut more);
                    Expr::Tuple(elements)
                }
            });

        // Lambda parameter: name or name: Type
        let lambda_param = ident()
            .then(just(Token::Colon).ignore_then(type_annotation()).or_not())
            .map(|(name, typ)| LambdaParam { name, typ });

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
                // Type annotation only allowed on simple variable patterns
                if type_annotation.is_some() && !matches!(pattern, Pattern::Var(_)) {
                    return Err(Rich::custom(
                        span,
                        "type annotations are only allowed on simple variable patterns",
                    ));
                }
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

        let postfix = atom.foldl(
            dot_suffix.repeated(),
            |receiver, suffix| match suffix {
                DotSuffix::MethodCall(method, args) => Expr::MethodCall {
                    receiver: Box::new(receiver),
                    method,
                    args,
                },
                DotSuffix::FieldAccess(field) => Expr::FieldAccess {
                    expr: Box::new(receiver),
                    field,
                },
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
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".to_string(),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                    Param {
                        name: "y".to_string(),
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
                name: "identity".to_string(),
                type_params: vec!["T".to_string()],
                params: vec![Param {
                    name: "x".to_string(),
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
                name: "pair".to_string(),
                type_params: vec!["A".to_string(), "B".to_string()],
                params: vec![
                    Param {
                        name: "a".to_string(),
                        typ: TypeAnnotation::Named(Path::simple("A".to_string())),
                    },
                    Param {
                        name: "b".to_string(),
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
                name: "double".to_string(),
                type_params: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                }],
                return_type: Some(TypeAnnotation::Named(Path::simple("Int".to_string()))),
                body: Expr::Call {
                    path: Path::simple("add".to_string()),
                    args: vec![Expr::Path(Path::simple("x".to_string())), Expr::Path(Path::simple("x".to_string())),],
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
        let stmts = parse_repl_str("fn foo() -> Int { 42 }").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Statement::Item(_)));
    }

    #[test]
    fn test_parse_repl_function_then_expr() {
        let stmts = parse_repl_str("fn foo() -> Int { 42 } foo()").unwrap();
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
                pattern: Pattern::Var(name),
                type_annotation: None,
                value,
            }) if name == "x" && **value == Expr::Int(42)
        ));
    }

    #[test]
    fn test_parse_let_with_type() {
        let stmts = parse_repl_str("let x: Int = 42").unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            &stmts[0],
            Statement::Let(LetBinding {
                pattern: Pattern::Var(name),
                type_annotation: Some(TypeAnnotation::Named(ty)),
                value,
            }) if name == "x" && ty.as_simple() == Some("Int") && **value == Expr::Int(42)
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
            assert!(matches!(&bindings[0].pattern, Pattern::Var(n) if n == "x"));
            assert!(matches!(&bindings[1].pattern, Pattern::Var(n) if n == "y"));
            assert!(matches!(*result, Expr::BinOp { .. }));
        } else {
            panic!("expected function with block body");
        }
    }

    #[test]
    fn test_parse_function_without_let_no_block() {
        // Without let statements, body should be a plain expression, not a block
        let item = parse_item_str("fn foo() { 42 }").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item else { panic!("expected function") };
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
                name: "add".to_string(),
                type_params: vec![],
                params: vec![
                    Param {
                        name: "x".to_string(),
                        typ: TypeAnnotation::Named(Path::simple("Int".to_string())),
                    },
                    Param {
                        name: "y".to_string(),
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
        let Item::Function(FunctionDef { body, .. }) = item else { panic!("expected function") };
        assert!(matches!(body, Expr::BinOp { op: BinOp::Mul, .. }));
    }

    use crate::ast::{MatchArm, Pattern};

    #[test]
    fn test_parse_match_with_literals() {
        let expr = parse_str("match x { 0 => 1, 1 => 2 }").unwrap();
        if let Expr::Match { scrutinee, arms } = expr {
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
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_with_wildcard() {
        let expr = parse_str("match x { 0 => 1, _ => 2 }").unwrap();
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
            assert!(matches!(&arms[0].result, Expr::Path(p) if p.as_simple() == Some("n")));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_with_strings() {
        let expr = parse_str(r#"match s { "a" => 1, "b" => 2 }"#).unwrap();
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
        let item = parse_item_str("fn f(x: Int) -> Int { match x { 0 => 0, n => n } }").unwrap();
        let Item::Function(FunctionDef { body, .. }) = item else { panic!("expected function") };
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
            } if matches!(*receiver, Expr::Path(ref p) if p.as_simple() == Some("s"))
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
            if let Pattern::List(ListPattern::Prefix { patterns, rest_binding }) = &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "head"));
                assert!(rest_binding.is_none());
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
            if let Pattern::List(ListPattern::Prefix { patterns, rest_binding }) = &arms[0].pattern
            {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Literal(lit) if **lit == Expr::Int(1)));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "x"));
                assert!(rest_binding.is_none());
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
            if let Pattern::List(ListPattern::Suffix { patterns, rest_binding }) = &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "last"));
                assert!(rest_binding.is_none());
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
            if let Pattern::List(ListPattern::Suffix { patterns, rest_binding }) = &arms[0].pattern
            {
                assert_eq!(patterns.len(), 2);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "x"));
                assert!(matches!(&patterns[1], Pattern::Var(s) if s == "y"));
                assert!(rest_binding.is_none());
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
            if let Pattern::List(ListPattern::PrefixSuffix {
                prefix,
                suffix,
                rest_binding,
            }) = &arms[0].pattern
            {
                assert_eq!(prefix.len(), 1);
                assert_eq!(suffix.len(), 1);
                assert!(matches!(&prefix[0], Pattern::Var(s) if s == "first"));
                assert!(matches!(&suffix[0], Pattern::Var(s) if s == "last"));
                assert!(rest_binding.is_none());
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
            if let Pattern::List(ListPattern::PrefixSuffix {
                prefix,
                suffix,
                rest_binding,
            }) = &arms[0].pattern
            {
                assert_eq!(prefix.len(), 2);
                assert_eq!(suffix.len(), 2);
                assert!(matches!(&prefix[0], Pattern::Var(s) if s == "a"));
                assert!(matches!(&prefix[1], Pattern::Var(s) if s == "b"));
                assert!(matches!(&suffix[0], Pattern::Var(s) if s == "y"));
                assert!(matches!(&suffix[1], Pattern::Var(s) if s == "z"));
                assert!(rest_binding.is_none());
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
        let item = parse_item_str("fn len(xs: List<Int>) -> Int { 0 }").unwrap();
        let Item::Function(FunctionDef { params, .. }) = item else { panic!("expected function") };
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
            if let Pattern::Tuple(TuplePattern::Prefix { patterns, rest_binding }) =
                &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "a"));
                assert!(rest_binding.is_none());
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
            if let Pattern::Tuple(TuplePattern::Suffix { patterns, rest_binding }) =
                &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "z"));
                assert!(rest_binding.is_none());
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
            if let Pattern::Tuple(TuplePattern::PrefixSuffix {
                prefix,
                suffix,
                rest_binding,
            }) = &arms[0].pattern
            {
                assert_eq!(prefix.len(), 1);
                assert_eq!(suffix.len(), 1);
                assert!(matches!(&prefix[0], Pattern::Var(s) if s == "a"));
                assert!(matches!(&suffix[0], Pattern::Var(s) if s == "z"));
                assert!(rest_binding.is_none());
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

    // As pattern (@) tests
    #[test]
    fn test_parse_as_pattern() {
        let expr = parse_str("match x { n @ 42 => n }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::As { name, pattern } = &arms[0].pattern {
                assert_eq!(name, "n");
                assert!(matches!(pattern.as_ref(), Pattern::Literal(lit) if **lit == Expr::Int(42)));
            } else {
                panic!("expected as pattern");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_list_rest_binding() {
        let expr = parse_str("match xs { [first, rest @ ..] => rest }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Prefix { patterns, rest_binding }) = &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "first"));
                assert_eq!(rest_binding.as_deref(), Some("rest"));
            } else {
                panic!("expected prefix list pattern with rest binding");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_list_rest_binding_suffix() {
        let expr = parse_str("match xs { [rest @ .., last] => rest }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::List(ListPattern::Suffix { patterns, rest_binding }) = &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "last"));
                assert_eq!(rest_binding.as_deref(), Some("rest"));
            } else {
                panic!("expected suffix list pattern with rest binding");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_tuple_rest_binding() {
        let expr = parse_str("match t { (a, rest @ ..) => rest }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::Tuple(TuplePattern::Prefix { patterns, rest_binding }) =
                &arms[0].pattern
            {
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0], Pattern::Var(s) if s == "a"));
                assert_eq!(rest_binding.as_deref(), Some("rest"));
            } else {
                panic!("expected prefix tuple pattern with rest binding");
            }
        } else {
            panic!("expected match expression");
        }
    }

    // Match arm block expression tests
    #[test]
    fn test_parse_match_with_commas() {
        let expr = parse_str("match x { 0 => 1, 1 => 2 }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
            assert!(matches!(&arms[0].result, Expr::Int(1)));
            assert!(matches!(&arms[1].result, Expr::Int(2)));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_with_trailing_comma() {
        let expr = parse_str("match x { 0 => 1, _ => 2, }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_braced_simple() {
        let expr = parse_str("match x { 0 => { 1 }, _ => { 2 } }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
            // Braced simple expressions should unwrap to just the expression
            assert!(matches!(&arms[0].result, Expr::Int(1)));
            assert!(matches!(&arms[1].result, Expr::Int(2)));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_braced_block() {
        let expr = parse_str("match x { 0 => { let y = 1; y + 1 }, _ => 0 }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
            if let Expr::Block { bindings, result } = &arms[0].result {
                assert_eq!(bindings.len(), 1);
                assert!(matches!(&bindings[0].pattern, Pattern::Var(n) if n == "y"));
                assert!(matches!(**result, Expr::BinOp { .. }));
            } else {
                panic!("expected block expression in first arm");
            }
            assert!(matches!(&arms[1].result, Expr::Int(0)));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_mixed() {
        // Mix of braced and non-braced arms with commas
        let expr = parse_str("match x { 0 => 1, 1 => { 2 }, _ => { let z = 3; z } }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 3);
            assert!(matches!(&arms[0].result, Expr::Int(1)));
            assert!(matches!(&arms[1].result, Expr::Int(2)));
            assert!(matches!(&arms[2].result, Expr::Block { .. }));
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_braced_block_with_semicolons() {
        let expr = parse_str("match x { n => { let a = n; let b = a * 2; a + b } }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Expr::Block { bindings, .. } = &arms[0].result {
                assert_eq!(bindings.len(), 2);
                assert!(matches!(&bindings[0].pattern, Pattern::Var(n) if n == "a"));
                assert!(matches!(&bindings[1].pattern, Pattern::Var(n) if n == "b"));
            } else {
                panic!("expected block expression");
            }
        } else {
            panic!("expected match expression");
        }
    }

    #[test]
    fn test_parse_match_braced_with_pattern_binding() {
        // Pattern binding should be usable in the block
        let expr = parse_str("match x { n => { let doubled = n * 2; doubled + 1 } }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Expr::Block { bindings, result } = &arms[0].result {
                assert_eq!(bindings.len(), 1);
                assert!(matches!(&bindings[0].pattern, Pattern::Var(n) if n == "doubled"));
                // The binding value should reference 'n' from the pattern
                if let Expr::BinOp { left, .. } = &*bindings[0].value {
                    assert!(matches!(**left, Expr::Path(ref p) if p.as_simple() == Some("n")));
                }
                // The result should reference 'doubled'
                if let Expr::BinOp { left, .. } = &**result {
                    assert!(matches!(**left, Expr::Path(ref p) if p.as_simple() == Some("doubled")));
                }
            } else {
                panic!("expected block expression");
            }
        } else {
            panic!("expected match expression");
        }
    }

    // Lambda tests
    #[test]
    fn test_parse_simple_lambda() {
        let expr = parse_str("|x| x + 1").unwrap();
        if let Expr::Lambda {
            params,
            return_type,
            body,
        } = expr
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(params[0].typ.is_none());
            assert!(return_type.is_none());
            assert!(matches!(*body, Expr::BinOp { op: BinOp::Add, .. }));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_multi_param() {
        let expr = parse_str("|x, y| x + y").unwrap();
        if let Expr::Lambda { params, .. } = expr {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "x");
            assert_eq!(params[1].name, "y");
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_with_type_annotation() {
        let expr = parse_str("|x: Int| x * 2").unwrap();
        if let Expr::Lambda { params, .. } = expr {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(matches!(
                &params[0].typ,
                Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
            ));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_with_return_type() {
        let expr = parse_str("|x| -> Int x + 1").unwrap();
        if let Expr::Lambda {
            params, return_type, ..
        } = expr
        {
            assert_eq!(params.len(), 1);
            assert!(matches!(
                return_type,
                Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
            ));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_fully_annotated() {
        let expr = parse_str("|x: Int| -> Int x * 2").unwrap();
        if let Expr::Lambda {
            params,
            return_type,
            ..
        } = expr
        {
            assert_eq!(params.len(), 1);
            assert!(matches!(
                &params[0].typ,
                Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
            ));
            assert!(matches!(
                return_type,
                Some(TypeAnnotation::Named(s)) if s.as_simple() == Some("Int")
            ));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_block_body() {
        let expr = parse_str("|x| { let y = x * 2; y + 1 }").unwrap();
        if let Expr::Lambda { body, .. } = expr {
            assert!(matches!(*body, Expr::Block { .. }));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_no_params() {
        let expr = parse_str("|| 42").unwrap();
        if let Expr::Lambda { params, body, .. } = expr {
            assert!(params.is_empty());
            assert!(matches!(*body, Expr::Int(42)));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_lambda_in_expression() {
        // Lambda as function argument (conceptually - requires let binding to use)
        let stmts = parse_repl_str("let f = |x| x + 1").unwrap();
        if let Statement::Let(binding) = &stmts[0] {
            assert!(matches!(&binding.pattern, Pattern::Var(n) if n == "f"));
            assert!(matches!(*binding.value, Expr::Lambda { .. }));
        } else {
            panic!("expected let statement");
        }
    }

    #[test]
    fn test_parse_lambda_nested() {
        let expr = parse_str("|x| |y| x + y").unwrap();
        if let Expr::Lambda { body, .. } = expr {
            assert!(matches!(*body, Expr::Lambda { .. }));
        } else {
            panic!("expected lambda");
        }
    }

    #[test]
    fn test_parse_function_type_simple() {
        // let f: Int -> Int = ...
        let stmts = parse_repl_str("let f: Int -> Int = |x| x + 1").unwrap();
        if let Statement::Let(binding) = &stmts[0] {
            assert!(matches!(
                &binding.type_annotation,
                Some(TypeAnnotation::Function(params, ret))
                if params.len() == 1
                    && matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
                    && matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int"))
            ));
        } else {
            panic!("expected let statement");
        }
    }

    #[test]
    fn test_parse_function_type_multi_param() {
        // let f: (Int, String) -> Bool = ...
        let stmts = parse_repl_str("let f: (Int, String) -> Bool = |x, y| true").unwrap();
        if let Statement::Let(binding) = &stmts[0] {
            if let Some(TypeAnnotation::Function(params, ret)) = &binding.type_annotation {
                assert_eq!(params.len(), 2);
                assert!(matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
                assert!(matches!(&params[1], TypeAnnotation::Named(n) if n.as_simple() == Some("String")));
                assert!(matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Bool")));
            } else {
                panic!("expected function type annotation");
            }
        } else {
            panic!("expected let statement");
        }
    }

    #[test]
    fn test_parse_function_type_no_params() {
        // let f: () -> Int = ...
        let stmts = parse_repl_str("let f: () -> Int = || 42").unwrap();
        if let Statement::Let(binding) = &stmts[0] {
            if let Some(TypeAnnotation::Function(params, ret)) = &binding.type_annotation {
                assert!(params.is_empty());
                assert!(matches!(ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
            } else {
                panic!("expected function type annotation");
            }
        } else {
            panic!("expected let statement");
        }
    }

    #[test]
    fn test_parse_function_type_nested() {
        // let f: Int -> Int -> Int = |x| |y| x + y
        // Should be: Int -> (Int -> Int) (right associative)
        let stmts = parse_repl_str("let f: Int -> Int -> Int = |x| |y| x + y").unwrap();
        if let Statement::Let(binding) = &stmts[0] {
            if let Some(TypeAnnotation::Function(params, ret)) = &binding.type_annotation {
                assert_eq!(params.len(), 1);
                assert!(matches!(&params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
                // ret should be Int -> Int
                if let TypeAnnotation::Function(inner_params, inner_ret) = ret.as_ref() {
                    assert_eq!(inner_params.len(), 1);
                    assert!(matches!(&inner_params[0], TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
                    assert!(matches!(inner_ret.as_ref(), TypeAnnotation::Named(n) if n.as_simple() == Some("Int")));
                } else {
                    panic!("expected nested function type");
                }
            } else {
                panic!("expected function type annotation");
            }
        } else {
            panic!("expected let statement");
        }
    }

    #[test]
    fn test_parse_function_param_with_function_type() {
        // fn apply(f: Int -> Int, x: Int) -> Int f(x)
        let item = parse_item_str("fn apply(f: Int -> Int, x: Int) -> Int f(x)").unwrap();
        let Item::Function(func) = &item else { panic!("expected function") };
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
        let Item::Struct(s) = item else { panic!("expected struct") };
        assert_eq!(s.name, "Point");
        assert_eq!(s.type_params, Vec::<String>::new());
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].name, "x");
        assert_eq!(s.fields[1].name, "y");
    }

    #[test]
    fn test_parse_struct_empty() {
        let item = parse_item_str("struct Empty {}").unwrap();
        let Item::Struct(s) = item else { panic!("expected struct") };
        assert_eq!(s.name, "Empty");
        assert_eq!(s.fields.len(), 0);
    }

    #[test]
    fn test_parse_struct_generic() {
        let item = parse_item_str("struct Pair<T, U> { first: T, second: U }").unwrap();
        let Item::Struct(s) = item else { panic!("expected struct") };
        assert_eq!(s.name, "Pair");
        assert_eq!(s.type_params, vec!["T", "U"]);
        assert_eq!(s.fields.len(), 2);
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
        if let Expr::Struct { path, fields } = expr {
            assert_eq!(path.as_simple(), Some("Point"));
            assert_eq!(fields.len(), 2);
            // Shorthand: x means x: x
            assert_eq!(fields[0].0, "x");
            assert!(matches!(&fields[0].1, Expr::Path(p) if p.as_simple() == Some("x")));
        } else {
            panic!("expected struct construct");
        }
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
        if let Expr::FieldAccess { expr: inner, field } = expr {
            assert_eq!(field, "c");
            assert!(matches!(
                *inner,
                Expr::FieldAccess { field: f, .. }
                if f == "b"
            ));
        } else {
            panic!("expected field access");
        }
    }

    #[test]
    fn test_parse_struct_pattern_exact() {
        let expr = parse_str("match p { Point { x, y } => x + y }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Struct(StructPattern::Exact { path, fields })
                if path.as_simple() == Some("Point") && fields.len() == 2
            ));
        } else {
            panic!("expected match");
        }
    }

    #[test]
    fn test_parse_struct_pattern_partial() {
        let expr = parse_str("match p { Point { x, .. } => x }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Struct(StructPattern::Partial { path, fields })
                if path.as_simple() == Some("Point") && fields.len() == 1
            ));
        } else {
            panic!("expected match");
        }
    }

    #[test]
    fn test_parse_struct_pattern_with_binding() {
        let expr = parse_str("match p { Point { x: a, y: b } => a }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            if let Pattern::Struct(StructPattern::Exact { path, fields }) = &arms[0].pattern {
                assert_eq!(path.as_simple(), Some("Point"));
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].field_name, "x");
                assert!(matches!(&*fields[0].pattern, Pattern::Var(n) if n == "a"));
            } else {
                panic!("expected exact struct pattern");
            }
        } else {
            panic!("expected match");
        }
    }

    // ===== Let Pattern Destructuring Parser Tests =====

    #[test]
    fn test_parse_let_tuple_pattern() {
        let stmts = parse_repl_str("let (a, b) = x").unwrap();
        assert!(matches!(
            &stmts[0],
            Statement::Let(LetBinding {
                pattern: Pattern::Tuple(TuplePattern::Exact(patterns)),
                ..
            }) if patterns.len() == 2
        ));
    }

    #[test]
    fn test_parse_let_struct_pattern() {
        let stmts = parse_repl_str("let Point { x, y } = p").unwrap();
        if let Statement::Let(LetBinding {
            pattern: Pattern::Struct(StructPattern::Exact { fields, .. }),
            ..
        }) = &stmts[0]
        {
            assert_eq!(fields.len(), 2);
        } else {
            panic!("expected struct pattern");
        }
    }

    #[test]
    fn test_parse_let_wildcard_pattern() {
        let stmts = parse_repl_str("let _ = expr").unwrap();
        assert!(matches!(
            &stmts[0],
            Statement::Let(LetBinding {
                pattern: Pattern::Wildcard,
                ..
            })
        ));
    }

    #[test]
    fn test_parse_let_nested_pattern() {
        let stmts = parse_repl_str("let (a, (b, c)) = x").unwrap();
        if let Statement::Let(LetBinding {
            pattern: Pattern::Tuple(TuplePattern::Exact(outer)),
            ..
        }) = &stmts[0]
        {
            assert_eq!(outer.len(), 2);
            assert!(matches!(&outer[0], Pattern::Var(_)));
            assert!(matches!(&outer[1], Pattern::Tuple(_)));
        } else {
            panic!("expected nested tuple pattern");
        }
    }

    #[test]
    fn test_parse_let_tuple_rest_pattern() {
        let stmts = parse_repl_str("let (first, ..) = tuple").unwrap();
        assert!(matches!(
            &stmts[0],
            Statement::Let(LetBinding {
                pattern: Pattern::Tuple(TuplePattern::Prefix { .. }),
                ..
            })
        ));
    }

    #[test]
    fn test_parse_let_type_annotation_on_var_only() {
        // Type annotation on simple var pattern - should succeed
        let result = parse_repl_str("let x: Int = 42");
        assert!(result.is_ok());

        // Type annotation on tuple pattern - should fail
        let result = parse_repl_str("let (a, b): (Int, Int) = x");
        assert!(result.is_err());
    }
}
