use chumsky::prelude::*;

use zoya_ast::{
    Attribute, EnumDef, EnumVariant, EnumVariantKind, Expr, FunctionDef, ImplBlock, ImplMethod,
    Item, Param, StructDef, StructFieldDef, StructKind, TypeAliasDef, Visibility,
};
use zoya_lexer::Token;

use crate::expressions::expr_parser;
use crate::helpers::{ident, mod_decl_parser, use_decl_parser};
use crate::patterns::pattern_parser;
use crate::statements::let_binding_parser;
use crate::types::type_annotation;

pub(crate) fn attribute_parser<'a>()
-> impl Parser<'a, &'a [Token], Attribute, extra::Err<Rich<'a, Token>>> + Clone {
    let args = ident()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<String>>()
        .delimited_by(just(Token::LParen), just(Token::RParen))
        .or_not();

    just(Token::Hash)
        .ignore_then(
            ident()
                .then(args)
                .delimited_by(just(Token::LBracket), just(Token::RBracket)),
        )
        .map(|(name, args)| Attribute { name, args })
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
        .clone()
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
        .then(return_type.clone())
        .then(body.clone())
        .map(
            |(((((is_pub, name), type_params), params), return_type), body)| {
                Item::Function(FunctionDef {
                    leading_comments: vec![],
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
                leading_comments: vec![],
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
                leading_comments: vec![],
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
        .then(type_params.clone())
        .then_ignore(just(Token::Eq))
        .then(type_annotation())
        .map(|(((is_pub, name), type_params), typ)| {
            Item::TypeAlias(TypeAliasDef {
                leading_comments: vec![],
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

    // impl<T> TypeAnnotation { methods... }
    let impl_method_params = {
        // self [, param: Type, ...]
        let self_params = just(Token::Self_)
            .ignore_then(
                just(Token::Comma)
                    .ignore_then(
                        param
                            .clone()
                            .separated_by(just(Token::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .or_not(),
            )
            .map(|rest| (true, rest.unwrap_or_default()));

        // param: Type, ... (no self)
        let no_self_params = param
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .map(|params| (false, params));

        choice((self_params, no_self_params)).delimited_by(just(Token::LParen), just(Token::RParen))
    };

    // Parse interleaved comments and attributes before each impl method
    let method_preamble = choice((
        select! { Token::LineComment(text) => MethodPreamble::Comment(text) },
        attribute_parser().map(MethodPreamble::Attr),
    ))
    .repeated()
    .collect::<Vec<MethodPreamble>>();

    let impl_method = method_preamble
        .then(just(Token::Pub).or_not())
        .then_ignore(just(Token::Fn))
        .then(ident())
        .then(type_params.clone())
        .then(impl_method_params)
        .then(return_type.clone())
        .then(body.clone())
        .map(
            |(
                (
                    ((((preamble, is_pub), name), method_type_params), (has_self, params)),
                    return_type,
                ),
                body,
            )| {
                let (comments, attrs) = split_method_preamble(preamble);
                ImplMethod {
                    leading_comments: comments,
                    attributes: attrs,
                    visibility: if is_pub.is_some() {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    },
                    name,
                    type_params: method_type_params,
                    has_self,
                    params,
                    return_type,
                    body,
                }
            },
        );

    // Trailing comments after last method (before `}`) are discarded
    let trailing_comments = select! { Token::LineComment(_text) => () }
        .repeated()
        .collect::<Vec<_>>();
    let impl_methods = impl_method
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(trailing_comments)
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    let impl_def = just(Token::Impl)
        .ignore_then(type_params)
        .then(type_annotation())
        .then(impl_methods)
        .map(|((impl_type_params, target_type), methods)| {
            Item::Impl(ImplBlock {
                leading_comments: vec![],
                attributes: vec![],
                type_params: impl_type_params,
                target_type,
                methods,
            })
        });

    let mod_decl = mod_decl_parser().map(Item::ModDecl);
    let use_decl = use_decl_parser().map(Item::Use);

    // Parse interleaved comments and attributes as a preamble before each item
    let preamble = choice((
        select! { Token::LineComment(text) => ItemPreamble::Comment(text) },
        attribute_parser().map(ItemPreamble::Attr),
    ))
    .repeated()
    .collect::<Vec<ItemPreamble>>();

    preamble
        .then(choice((
            mod_decl,
            use_decl,
            function_def,
            struct_def,
            enum_def,
            type_alias_def,
            impl_def,
        )))
        .map(|(preamble, item)| {
            let (comments, attrs) = split_item_preamble(preamble);
            if comments.is_empty() && attrs.is_empty() {
                return item;
            }
            match item {
                Item::Function(mut f) => {
                    f.leading_comments = comments;
                    f.attributes = attrs;
                    Item::Function(f)
                }
                Item::Struct(mut s) => {
                    s.leading_comments = comments;
                    s.attributes = attrs;
                    Item::Struct(s)
                }
                Item::Enum(mut e) => {
                    e.leading_comments = comments;
                    e.attributes = attrs;
                    Item::Enum(e)
                }
                Item::TypeAlias(mut t) => {
                    t.leading_comments = comments;
                    t.attributes = attrs;
                    Item::TypeAlias(t)
                }
                Item::Use(mut u) => {
                    u.leading_comments = comments;
                    u.attributes = attrs;
                    Item::Use(u)
                }
                Item::Impl(mut i) => {
                    i.leading_comments = comments;
                    i.attributes = attrs;
                    Item::Impl(i)
                }
                Item::ModDecl(mut m) => {
                    m.leading_comments = comments;
                    m.attributes = attrs;
                    Item::ModDecl(m)
                }
            }
        })
}

/// Element in the preamble before an item: either a comment or an attribute
enum ItemPreamble {
    Comment(String),
    Attr(Attribute),
}

/// Split preamble into comments and attributes, preserving order
fn split_item_preamble(preamble: Vec<ItemPreamble>) -> (Vec<String>, Vec<Attribute>) {
    let mut comments = Vec::new();
    let mut attrs = Vec::new();
    for item in preamble {
        match item {
            ItemPreamble::Comment(text) => comments.push(text),
            ItemPreamble::Attr(attr) => attrs.push(attr),
        }
    }
    (comments, attrs)
}

/// Element in the preamble before an impl method: either a comment or an attribute
enum MethodPreamble {
    Comment(String),
    Attr(Attribute),
}

/// Split method preamble into comments and attributes
fn split_method_preamble(preamble: Vec<MethodPreamble>) -> (Vec<String>, Vec<Attribute>) {
    let mut comments = Vec::new();
    let mut attrs = Vec::new();
    for item in preamble {
        match item {
            MethodPreamble::Comment(text) => comments.push(text),
            MethodPreamble::Attr(attr) => attrs.push(attr),
        }
    }
    (comments, attrs)
}
