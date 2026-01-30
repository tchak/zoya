use chumsky::prelude::*;

use zoya_ast::{ModDecl, Path, PathPrefix, Pattern, TypeAnnotation};
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

/// Validate that a type annotation is only used with a simple variable pattern.
///
/// Type annotations like `let x: Int = ...` are only permitted on simple variable
/// patterns (not destructuring patterns like tuples, lists, or structs).
///
/// # Arguments
/// * `pattern` - The pattern being bound
/// * `type_ann` - Optional type annotation
/// * `span` - Source span for error reporting
///
/// # Returns
/// `Ok(())` if valid, or `Err` with a descriptive parse error
pub(crate) fn validate_typed_pattern<'a>(
    pattern: &Pattern,
    type_ann: &Option<TypeAnnotation>,
    span: SimpleSpan,
) -> Result<(), Rich<'a, Token>> {
    if type_ann.is_some() && !matches!(pattern, Pattern::Var(_)) {
        return Err(Rich::custom(
            span,
            "type annotations are only allowed on simple variable patterns",
        ));
    }
    Ok(())
}

/// Result of processing a collection with potential rest (`..`) elements.
///
/// Used to separate patterns before and after a rest marker in list, tuple,
/// and call patterns.
pub(crate) enum RestSplit<T> {
    /// No rest marker present - all elements are patterns
    Exact(Vec<T>),
    /// Rest marker present with prefix patterns, optional binding name, and suffix patterns
    WithRest {
        prefix: Vec<T>,
        rest_binding: Option<String>,
        suffix: Vec<T>,
    },
}

/// Process a collection of elements that may contain a rest (`..`) marker.
///
/// This helper extracts the common logic for handling rest patterns in lists,
/// tuples, and call patterns. It validates that at most one rest marker exists
/// and splits elements into prefix and suffix groups.
///
/// # Type Parameters
/// * `T` - The element type (e.g., Pattern)
///
/// # Arguments
/// * `elements` - Collection of elements, some of which may be rest markers
/// * `is_rest` - Predicate that returns `Some(binding_name)` for rest elements
/// * `span` - Source span for error reporting
/// * `pattern_name` - Name of the pattern type for error messages (e.g., "list", "tuple")
///
/// # Returns
/// `RestSplit` indicating whether rest was found and the split elements
pub(crate) fn process_rest_elements<'a, T, F>(
    elements: Vec<T>,
    is_rest: F,
    span: SimpleSpan,
    pattern_name: &str,
) -> Result<RestSplit<T>, Rich<'a, Token>>
where
    F: Fn(&T) -> Option<Option<String>>,
{
    // Find position of rest marker
    let rest_pos = elements.iter().position(|e| is_rest(e).is_some());

    // Validate at most one rest marker
    let rest_count = elements.iter().filter(|e| is_rest(e).is_some()).count();
    if rest_count > 1 {
        return Err(Rich::custom(
            span,
            format!("only one .. allowed in {pattern_name} pattern"),
        ));
    }

    match rest_pos {
        None => {
            // No rest marker - collect non-rest elements (should be all of them)
            let patterns = elements
                .into_iter()
                .filter(|e| is_rest(e).is_none())
                .collect();
            Ok(RestSplit::Exact(patterns))
        }
        Some(pos) => {
            // Extract binding name from rest element
            let rest_binding = is_rest(&elements[pos]).flatten();

            // Split into prefix and suffix
            let mut iter = elements.into_iter();
            let prefix: Vec<T> = iter
                .by_ref()
                .take(pos)
                .filter(|e| is_rest(e).is_none())
                .collect();

            // Skip the rest element
            iter.next();

            let suffix: Vec<T> = iter.filter(|e| is_rest(e).is_none()).collect();

            Ok(RestSplit::WithRest {
                prefix,
                rest_binding,
                suffix,
            })
        }
    }
}
