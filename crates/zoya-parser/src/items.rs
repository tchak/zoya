use chumsky::prelude::*;

use zoya_ast::{
    EnumDef, EnumVariant, EnumVariantKind, Expr, FunctionDef, Item, Param, StructDef,
    StructFieldDef, TypeAliasDef,
};
use zoya_lexer::Token;

use crate::expressions::expr_parser;
use crate::helpers::ident;
use crate::patterns::pattern_parser;
use crate::statements::let_binding_parser;
use crate::types::type_annotation;

pub(crate) fn item_parser<'a>(
) -> impl Parser<'a, &'a [Token], Item, extra::Err<Rich<'a, Token>>> + Clone {
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
        .then(type_params.clone())
        .then(enum_variants)
        .map(|((name, type_params), variants)| {
            Item::Enum(EnumDef {
                name,
                type_params,
                variants,
            })
        });

    // type Name<T> = TypeAnnotation
    let type_alias_def = just(Token::Type)
        .ignore_then(ident())
        .then(type_params)
        .then_ignore(just(Token::Eq))
        .then(type_annotation())
        .map(|((name, type_params), typ)| {
            Item::TypeAlias(TypeAliasDef {
                name,
                type_params,
                typ,
            })
        });

    choice((function_def, struct_def, enum_def, type_alias_def))
}
