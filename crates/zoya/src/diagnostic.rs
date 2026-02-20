use miette::{LabeledSpan, MietteDiagnostic, NamedSource, Report};
use zoya_loader::{FilePath, LoaderError};
use zoya_parser::{ParseError, SyntaxError};
use zoya_run::EvalError;

/// Try to render a diagnostic from an anyhow error.
///
/// Downcasts to `LoaderError<FilePath>` or `EvalError` (for the `LoadError` variant
/// wrapping `LoaderError<String>`), and renders lex/parse errors via miette.
///
/// Returns `true` if the error was handled and rendered.
pub fn try_render_diagnostic(error: &anyhow::Error) -> bool {
    if let Some(loader_err) = error.downcast_ref::<LoaderError<FilePath>>() {
        return try_render_loader_error(loader_err, |p| p.to_string());
    }

    if let Some(EvalError::LoadError(loader_err)) = error.downcast_ref::<EvalError>() {
        return try_render_loader_error(loader_err, |p| p.clone());
    }

    false
}

/// Try to render a diagnostic for a lex or parse error when source text is already available.
///
/// Used by the REPL and fmt command where errors come from bare `LexError`/`ParseError`
/// rather than `LoaderError`.
///
/// Returns `true` if the error was handled and rendered.
pub fn try_render_diagnostic_with_source(error: &anyhow::Error, name: &str, source: &str) -> bool {
    if let Some(lex_err) = error.downcast_ref::<zoya_lexer::LexError>() {
        render_lex_error(name, source, lex_err);
        return true;
    }
    if let Some(parse_err) = error.downcast_ref::<ParseError>() {
        render_parse_error(name, source, parse_err);
        return true;
    }
    false
}

fn try_render_loader_error<P: Clone + std::fmt::Debug + std::fmt::Display>(
    err: &LoaderError<P>,
    display_path: impl Fn(&P) -> String,
) -> bool {
    match err {
        LoaderError::LexError {
            path,
            source_text,
            source,
        } => {
            render_lex_error(&display_path(path), source_text, source);
            true
        }
        LoaderError::ParseError {
            path,
            source_text,
            source,
        } => {
            render_parse_error(&display_path(path), source_text, source);
            true
        }
        _ => false,
    }
}

/// Render a lex error with source annotations via miette.
pub fn render_lex_error(path: &str, source_text: &str, error: &zoya_lexer::LexError) {
    match error {
        zoya_lexer::LexError::UnexpectedCharacter { slice, span } => {
            let diagnostic =
                MietteDiagnostic::new("lexer error").with_labels(vec![LabeledSpan::at(
                    span.start..span.end,
                    format!("unexpected character '{slice}'"),
                )]);
            let report = Report::new(diagnostic)
                .with_source_code(NamedSource::new(path, source_text.to_string()));
            eprintln!("{:?}", report);
        }
    }
}

/// Render a parse error with source annotations via miette.
pub fn render_parse_error(path: &str, source_text: &str, error: &ParseError) {
    match error {
        ParseError::SyntaxErrors(errors) => {
            let labels: Vec<LabeledSpan> = errors.iter().map(syntax_error_label).collect();
            let diagnostic = MietteDiagnostic::new("parse error").with_labels(labels);
            let report = Report::new(diagnostic)
                .with_source_code(NamedSource::new(path, source_text.to_string()));
            eprintln!("{:?}", report);
        }
    }
}

fn syntax_error_label(e: &SyntaxError) -> LabeledSpan {
    let message = match (&e.label, e.found.as_ref()) {
        (Some(label), _) => label.clone(),
        (None, Some(found)) => {
            let expected = join_expected(&e.expected);
            format!("found {found}, expected {expected}")
        }
        (None, None) => {
            let expected = join_expected(&e.expected);
            format!("unexpected end of input, expected {expected}")
        }
    };
    LabeledSpan::at(e.span.start..e.span.end, message)
}

fn join_expected(expected: &[String]) -> String {
    match expected.len() {
        0 => "something else".to_string(),
        1 => expected[0].clone(),
        _ => {
            let last = &expected[expected.len() - 1];
            let rest = &expected[..expected.len() - 1];
            format!("{} or {}", rest.join(", "), last)
        }
    }
}
