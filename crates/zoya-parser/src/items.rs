use chumsky::prelude::*;

use zoya_ast::{
    Attribute, EnumDef, EnumVariant, EnumVariantKind, Expr, FunctionDef, Item, Param, StructDef,
    StructFieldDef, StructKind, TypeAliasDef, Visibility,
};
use zoya_lexer::Token;

use crate::expressions::expr_parser;
use crate::helpers::ident;
use crate::patterns::pattern_parser;
use crate::statements::let_binding_parser;
use crate::types::type_annotation;

pub(crate) fn attribute_parser<'a>()
-> impl Parser<'a, &'a [Token], Attribute, extra::Err<Rich<'a, Token>>> + Clone {
    just(Token::Hash)
        .ignore_then(ident().delimited_by(just(Token::LBracket), just(Token::RBracket)))
        .map(|name| Attribute { name })
}

pub(crate) fn item_parser<'a>()
-> impl Parser<'a, &'a [Token], Item, extra::Err<Rich<'a, Token>>> + Clone {
    // Type parameters: <T, U>
    let type_params = ident()
        .separated_by(just(Token::Comma))
        .collect::<Vec<_>>()
        .delimited_by(just(Token::Lt), just(Token::Gt))
        .or_not()
        .map(|opt| opt.unwrap_or_default());

    // Parameter: name: Type
    let param = pattern_parser()
        .then_ignore(just(Token::Colon))
        .then(type_annotation())
        .map(|(pattern, typ)| Param { pattern, typ });

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

    // [pub] fn name<T>(params) -> ReturnType { body }
    let function_def = just(Token::Pub)
        .or_not()
        .then_ignore(just(Token::Fn))
        .then(ident())
        .then(type_params.clone())
        .then(params)
        .then(return_type)
        .then(body)
        .map(
            |(((((is_pub, name), type_params), params), return_type), body)| {
                Item::Function(FunctionDef {
                    attributes: vec![],
                    visibility: if is_pub.is_some() {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    },
                    name,
                    type_params,
                    params,
                    return_type,
                    body,
                })
            },
        );

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

    // Tuple struct fields: (Type, Type, ...)
    let struct_tuple_fields = type_annotation()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .at_least(1)
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LParen), just(Token::RParen));

    // [pub] struct Name<T> { field: Type, ... }
    // [pub] struct Name(Type, ...)  (tuple struct)
    // [pub] struct Name  (unit struct, no braces)
    let struct_def = just(Token::Pub)
        .or_not()
        .then_ignore(just(Token::Struct))
        .then(ident())
        .then(type_params.clone())
        .then(
            choice((
                struct_fields.clone().map(StructKind::Named),
                struct_tuple_fields.map(StructKind::Tuple),
            ))
            .or_not(),
        )
        .map(|(((is_pub, name), type_params), kind)| {
            Item::Struct(StructDef {
                attributes: vec![],
                visibility: if is_pub.is_some() {
                    Visibility::Public
                } else {
                    Visibility::Private
                },
                name,
                type_params,
                kind: kind.unwrap_or(StructKind::Unit),
            })
        });

    // Enum variant: Unit, Tuple(T, U), or Struct { field: Type }
    // Try struct variant first (has braces), then tuple (has parens), then unit
    let enum_variant_struct =
        ident()
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

    // [pub] enum Name<T> { Variant, Variant(T), Variant { field: Type } }
    let enum_def = just(Token::Pub)
        .or_not()
        .then_ignore(just(Token::Enum))
        .then(ident())
        .then(type_params.clone())
        .then(enum_variants)
        .map(|(((is_pub, name), type_params), variants)| {
            Item::Enum(EnumDef {
                attributes: vec![],
                visibility: if is_pub.is_some() {
                    Visibility::Public
                } else {
                    Visibility::Private
                },
                name,
                type_params,
                variants,
            })
        });

    // [pub] type Name<T> = TypeAnnotation
    let type_alias_def = just(Token::Pub)
        .or_not()
        .then_ignore(just(Token::Type))
        .then(ident())
        .then(type_params)
        .then_ignore(just(Token::Eq))
        .then(type_annotation())
        .map(|(((is_pub, name), type_params), typ)| {
            Item::TypeAlias(TypeAliasDef {
                attributes: vec![],
                visibility: if is_pub.is_some() {
                    Visibility::Public
                } else {
                    Visibility::Private
                },
                name,
                type_params,
                typ,
            })
        });

    let attributes = attribute_parser().repeated().collect::<Vec<_>>();

    attributes
        .then(choice((function_def, struct_def, enum_def, type_alias_def)))
        .map(|(attrs, item)| {
            if attrs.is_empty() {
                return item;
            }
            match item {
                Item::Function(mut f) => {
                    f.attributes = attrs;
                    Item::Function(f)
                }
                Item::Struct(mut s) => {
                    s.attributes = attrs;
                    Item::Struct(s)
                }
                Item::Enum(mut e) => {
                    e.attributes = attrs;
                    Item::Enum(e)
                }
                Item::TypeAlias(mut t) => {
                    t.attributes = attrs;
                    Item::TypeAlias(t)
                }
                Item::Use(mut u) => {
                    u.attributes = attrs;
                    Item::Use(u)
                }
            }
        })
}
