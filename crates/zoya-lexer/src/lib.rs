use logos::Logos;

pub type Span = std::ops::Range<usize>;
pub type Spanned = (Token, Span);

fn parse_float(lex: &logos::Lexer<Token>) -> Option<f64> {
    lex.slice().replace('_', "").parse::<f64>().ok()
}

fn parse_bigint(lex: &logos::Lexer<Token>) -> Option<i64> {
    let s = lex.slice();
    // Strip trailing 'n' and underscores
    s[..s.len() - 1].replace('_', "").parse::<i64>().ok()
}

fn parse_string(lex: &logos::Lexer<Token>) -> Option<String> {
    let s = lex.slice();
    // Strip surrounding quotes
    let inner = &s[1..s.len() - 1];
    // Handle escape sequences
    let mut result = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // The regex ensures every backslash is followed by a character
            match chars.next().unwrap() {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                '"' => result.push('"'),
                other => {
                    result.push('\\');
                    result.push(other);
                }
            }
        } else {
            result.push(c);
        }
    }
    Some(result)
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")]
#[logos(skip(r"//[^\n]*", allow_greedy = true))]
pub enum Token {
    // Keywords (must come before Ident)
    #[token("fn")]
    Fn,

    #[token("true")]
    True,

    #[token("false")]
    False,

    #[token("let")]
    Let,

    #[token("match")]
    Match,

    #[token("struct")]
    Struct,

    #[token("enum")]
    Enum,

    #[token("type")]
    Type,

    #[token("mod")]
    Mod,

    #[token("use")]
    Use,

    #[token("pub")]
    Pub,

    #[token("root")]
    Root,

    #[token("self")]
    Self_,

    #[token("super")]
    Super,

    // String literals with escape sequences
    #[regex(r#""([^"\\]|\\.)*""#, parse_string)]
    String(String),

    // Float requires both integer and decimal parts (e.g., 1.0, not .5 or 1.)
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", parse_float)]
    Float(f64),

    // BigInt literals with 'n' suffix (must come before Int to match first)
    #[regex(r"[0-9][0-9_]*n", parse_bigint)]
    BigInt(i64),

    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    Int(i64),

    // Identifiers (after keywords)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Operators
    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("**")]
    StarStar,

    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    #[token("->")]
    Arrow,

    #[token("=>")]
    FatArrow,

    #[token("==")]
    EqEq,

    #[token("!=")]
    Ne,

    #[token("<=")]
    Le,

    #[token(">=")]
    Ge,

    #[token("=")]
    Eq,

    #[token(";")]
    Semicolon,

    // Delimiters
    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("<")]
    Lt,

    #[token(">")]
    Gt,

    #[token("::")]
    ColonColon,

    #[token(":")]
    Colon,

    #[token(",")]
    Comma,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("..")]
    DotDot,

    #[token(".")]
    Dot,

    #[token("|")]
    Pipe,

    #[token("@")]
    At,

    #[token("#")]
    Hash,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LexError {
    #[error("unexpected character '{slice}' at {span:?}")]
    UnexpectedCharacter { slice: String, span: Span },
}

pub fn lex(input: &str) -> Result<Vec<Spanned>, LexError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(input);

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => tokens.push((token, lexer.span())),
            Err(()) => {
                return Err(LexError::UnexpectedCharacter {
                    slice: lexer.slice().to_string(),
                    span: lexer.span(),
                });
            }
        }
    }

    split_dot_float(&mut tokens, input);

    Ok(tokens)
}

/// Rewrite `Dot, Float(...)` sequences into `Dot, Int, Dot, Int`.
/// This enables chained tuple indexing like `t.0.1` without parentheses.
/// `t.0.1` lexes as `Ident, Dot, Float(0.1)` → rewrites to `Ident, Dot, Int(0), Dot, Int(1)`
/// Standalone floats like `3.14` have no preceding `Dot`, so they're untouched.
fn split_dot_float(tokens: &mut Vec<Spanned>, input: &str) {
    let mut i = 0;
    while i + 1 < tokens.len() {
        if tokens[i].0 == Token::Dot && matches!(tokens[i + 1].0, Token::Float(_)) {
            let float_span = tokens[i + 1].1.clone();
            let float_str = &input[float_span.clone()];
            if let Some(dot_pos) = float_str.find('.') {
                let int_part: i64 = float_str[..dot_pos].replace('_', "").parse().unwrap();
                let dec_part: i64 = float_str[dot_pos + 1..].replace('_', "").parse().unwrap();
                let int1_end = float_span.start + dot_pos;
                let int2_start = int1_end + 1;
                tokens.splice(
                    i + 1..i + 2,
                    [
                        (Token::Int(int_part), float_span.start..int1_end),
                        (Token::Dot, int1_end..int2_start),
                        (Token::Int(dec_part), int2_start..float_span.end),
                    ],
                );
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(input: &str) -> Vec<Token> {
        lex(input).unwrap().into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn test_single_integer() {
        let toks = toks("42");
        assert_eq!(toks, vec![Token::Int(42)]);
    }

    #[test]
    fn test_large_integer() {
        let toks = toks("123456789");
        assert_eq!(toks, vec![Token::Int(123456789)]);
    }

    #[test]
    fn test_all_operators() {
        let toks = toks("+ - * / % **");
        assert_eq!(
            toks,
            vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Percent,
                Token::StarStar,
            ]
        );
    }

    #[test]
    fn test_star_star_token() {
        let toks = toks("2 ** 3");
        assert_eq!(toks, vec![Token::Int(2), Token::StarStar, Token::Int(3)]);
    }

    #[test]
    fn test_percent_token() {
        let toks = toks("10 % 3");
        assert_eq!(toks, vec![Token::Int(10), Token::Percent, Token::Int(3)]);
    }

    #[test]
    fn test_star_vs_star_star() {
        let toks = toks("* ** *");
        assert_eq!(toks, vec![Token::Star, Token::StarStar, Token::Star]);
    }

    #[test]
    fn test_parentheses() {
        let toks = toks("()");
        assert_eq!(toks, vec![Token::LParen, Token::RParen]);
    }

    #[test]
    fn test_full_expression() {
        let toks = toks("2 + 3 * (4 - 1)");
        assert_eq!(
            toks,
            vec![
                Token::Int(2),
                Token::Plus,
                Token::Int(3),
                Token::Star,
                Token::LParen,
                Token::Int(4),
                Token::Minus,
                Token::Int(1),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn test_no_whitespace() {
        let toks = toks("1+2*3");
        assert_eq!(
            toks,
            vec![
                Token::Int(1),
                Token::Plus,
                Token::Int(2),
                Token::Star,
                Token::Int(3),
            ]
        );
    }

    #[test]
    fn test_invalid_character() {
        let result = lex("2 + $");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("$"));
    }

    #[test]
    fn test_hash_token() {
        let toks = toks("#");
        assert_eq!(toks, vec![Token::Hash]);
    }

    #[test]
    fn test_annotation_tokens() {
        let toks = toks("#[test]");
        assert_eq!(
            toks,
            vec![
                Token::Hash,
                Token::LBracket,
                Token::Ident("test".to_string()),
                Token::RBracket,
            ]
        );
    }

    #[test]
    fn test_annotation_with_args_tokens() {
        let toks = toks("#[mode(test)]");
        assert_eq!(
            toks,
            vec![
                Token::Hash,
                Token::LBracket,
                Token::Ident("mode".to_string()),
                Token::LParen,
                Token::Ident("test".to_string()),
                Token::RParen,
                Token::RBracket,
            ]
        );
    }

    #[test]
    fn test_annotation_with_multiple_args_tokens() {
        let toks = toks("#[mode(test, foo)]");
        assert_eq!(
            toks,
            vec![
                Token::Hash,
                Token::LBracket,
                Token::Ident("mode".to_string()),
                Token::LParen,
                Token::Ident("test".to_string()),
                Token::Comma,
                Token::Ident("foo".to_string()),
                Token::RParen,
                Token::RBracket,
            ]
        );
    }

    #[test]
    fn test_at_token() {
        let toks = toks("@");
        assert_eq!(toks, vec![Token::At]);
    }

    #[test]
    fn test_as_pattern_tokens() {
        let toks = toks("n @ 42");
        assert_eq!(
            toks,
            vec![Token::Ident("n".to_string()), Token::At, Token::Int(42)]
        );
    }

    #[test]
    fn test_rest_binding_tokens() {
        let toks = toks("rest @ ..");
        assert_eq!(
            toks,
            vec![Token::Ident("rest".to_string()), Token::At, Token::DotDot]
        );
    }

    #[test]
    fn test_integer_with_underscores() {
        let toks = toks("1_000_000");
        assert_eq!(toks, vec![Token::Int(1_000_000)]);
    }

    #[test]
    fn test_integer_with_single_underscore() {
        let toks = toks("1_0");
        assert_eq!(toks, vec![Token::Int(10)]);
    }

    #[test]
    fn test_integer_with_trailing_underscore() {
        let toks = toks("100_");
        assert_eq!(toks, vec![Token::Int(100)]);
    }

    #[test]
    fn test_float_standard() {
        let toks = toks("3.15");
        assert_eq!(toks, vec![Token::Float(3.15)]);
    }

    #[test]
    fn test_float_with_underscores() {
        let toks = toks("1_000.5");
        assert_eq!(toks, vec![Token::Float(1000.5)]);
    }

    #[test]
    fn test_float_expression() {
        let toks = toks("1.5 + 0.5");
        assert_eq!(
            toks,
            vec![Token::Float(1.5), Token::Plus, Token::Float(0.5)]
        );
    }

    #[test]
    fn test_fn_keyword() {
        let toks = toks("fn");
        assert_eq!(toks, vec![Token::Fn]);
    }

    #[test]
    fn test_identifier() {
        let toks = toks("foo");
        assert_eq!(toks, vec![Token::Ident("foo".to_string())]);
    }

    #[test]
    fn test_identifier_with_underscore() {
        let toks = toks("foo_bar");
        assert_eq!(toks, vec![Token::Ident("foo_bar".to_string())]);
    }

    #[test]
    fn test_identifier_starting_with_underscore() {
        let toks = toks("_foo");
        assert_eq!(toks, vec![Token::Ident("_foo".to_string())]);
    }

    #[test]
    fn test_identifier_with_numbers() {
        let toks = toks("foo123");
        assert_eq!(toks, vec![Token::Ident("foo123".to_string())]);
    }

    #[test]
    fn test_fn_not_identifier() {
        // fn should be keyword, not identifier
        let toks = toks("fn foo");
        assert_eq!(toks, vec![Token::Fn, Token::Ident("foo".to_string())]);
    }

    #[test]
    fn test_arrow() {
        let toks = toks("->");
        assert_eq!(toks, vec![Token::Arrow]);
    }

    #[test]
    fn test_braces() {
        let toks = toks("{}");
        assert_eq!(toks, vec![Token::LBrace, Token::RBrace]);
    }

    #[test]
    fn test_angle_brackets() {
        let toks = toks("<>");
        assert_eq!(toks, vec![Token::Lt, Token::Gt]);
    }

    #[test]
    fn test_colon_and_comma() {
        let toks = toks(":,");
        assert_eq!(toks, vec![Token::Colon, Token::Comma]);
    }

    #[test]
    fn test_function_signature() {
        let toks = toks("fn add(x: Int, y: Int) -> Int { x + y }");
        assert_eq!(
            toks,
            vec![
                Token::Fn,
                Token::Ident("add".to_string()),
                Token::LParen,
                Token::Ident("x".to_string()),
                Token::Colon,
                Token::Ident("Int".to_string()),
                Token::Comma,
                Token::Ident("y".to_string()),
                Token::Colon,
                Token::Ident("Int".to_string()),
                Token::RParen,
                Token::Arrow,
                Token::Ident("Int".to_string()),
                Token::LBrace,
                Token::Ident("x".to_string()),
                Token::Plus,
                Token::Ident("y".to_string()),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_generic_function_signature() {
        let toks = toks("fn identity<T>(x: T) -> T { x }");
        assert_eq!(
            toks,
            vec![
                Token::Fn,
                Token::Ident("identity".to_string()),
                Token::Lt,
                Token::Ident("T".to_string()),
                Token::Gt,
                Token::LParen,
                Token::Ident("x".to_string()),
                Token::Colon,
                Token::Ident("T".to_string()),
                Token::RParen,
                Token::Arrow,
                Token::Ident("T".to_string()),
                Token::LBrace,
                Token::Ident("x".to_string()),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_bool_literals() {
        let toks = toks("true false");
        assert_eq!(toks, vec![Token::True, Token::False]);
    }

    #[test]
    fn test_string_literal() {
        let toks = toks(r#""hello""#);
        assert_eq!(toks, vec![Token::String("hello".to_string())]);
    }

    #[test]
    fn test_string_with_escapes() {
        let toks = toks(r#""hello\nworld""#);
        assert_eq!(toks, vec![Token::String("hello\nworld".to_string())]);
    }

    #[test]
    fn test_string_with_escaped_quote() {
        let toks = toks(r#""say \"hi\"""#);
        assert_eq!(toks, vec![Token::String("say \"hi\"".to_string())]);
    }

    #[test]
    fn test_string_with_tab_escape() {
        let toks = toks(r#""hello\tworld""#);
        assert_eq!(toks, vec![Token::String("hello\tworld".to_string())]);
    }

    #[test]
    fn test_string_with_carriage_return_escape() {
        let toks = toks(r#""line\rend""#);
        assert_eq!(toks, vec![Token::String("line\rend".to_string())]);
    }

    #[test]
    fn test_string_with_backslash_escape() {
        let toks = toks(r#""path\\to\\file""#);
        assert_eq!(toks, vec![Token::String("path\\to\\file".to_string())]);
    }

    #[test]
    fn test_string_with_unknown_escape() {
        // Unknown escape sequences are preserved literally
        let toks = toks(r#""test\xvalue""#);
        assert_eq!(toks, vec![Token::String("test\\xvalue".to_string())]);
    }

    #[test]
    fn test_comparison_operators() {
        let toks = toks("== != <= >= < >");
        assert_eq!(
            toks,
            vec![
                Token::EqEq,
                Token::Ne,
                Token::Le,
                Token::Ge,
                Token::Lt,
                Token::Gt
            ]
        );
    }

    #[test]
    fn test_comparison_expression() {
        let toks = toks("1 == 2");
        assert_eq!(toks, vec![Token::Int(1), Token::EqEq, Token::Int(2)]);
    }

    #[test]
    fn test_le_ge_not_arrow() {
        // Make sure <= and >= don't interfere with -> or <>
        let toks = toks("<= >= -> < >");
        assert_eq!(
            toks,
            vec![Token::Le, Token::Ge, Token::Arrow, Token::Lt, Token::Gt]
        );
    }

    #[test]
    fn test_brackets() {
        let toks = toks("[]");
        assert_eq!(toks, vec![Token::LBracket, Token::RBracket]);
    }

    #[test]
    fn test_list_literal() {
        let toks = toks("[1, 2, 3]");
        assert_eq!(
            toks,
            vec![
                Token::LBracket,
                Token::Int(1),
                Token::Comma,
                Token::Int(2),
                Token::Comma,
                Token::Int(3),
                Token::RBracket,
            ]
        );
    }

    #[test]
    fn test_dot_dot() {
        let toks = toks("..");
        assert_eq!(toks, vec![Token::DotDot]);
    }

    #[test]
    fn test_dot_vs_dot_dot() {
        // Make sure .. is separate from .
        let toks = toks(". .. .");
        assert_eq!(toks, vec![Token::Dot, Token::DotDot, Token::Dot]);
    }

    #[test]
    fn test_list_pattern_tokens() {
        let toks = toks("[x, ..]");
        assert_eq!(
            toks,
            vec![
                Token::LBracket,
                Token::Ident("x".to_string()),
                Token::Comma,
                Token::DotDot,
                Token::RBracket,
            ]
        );
    }

    #[test]
    fn test_bigint_simple() {
        let toks = toks("42n");
        assert_eq!(toks, vec![Token::BigInt(42)]);
    }

    #[test]
    fn test_bigint_with_underscores() {
        let toks = toks("9_000_000_000n");
        assert_eq!(toks, vec![Token::BigInt(9_000_000_000)]);
    }

    #[test]
    fn test_bigint_in_expression() {
        let toks = toks("42n + 1n");
        assert_eq!(toks, vec![Token::BigInt(42), Token::Plus, Token::BigInt(1)]);
    }

    #[test]
    fn test_bigint_zero() {
        let toks = toks("0n");
        assert_eq!(toks, vec![Token::BigInt(0)]);
    }

    #[test]
    fn test_pipe() {
        let toks = toks("|");
        assert_eq!(toks, vec![Token::Pipe]);
    }

    #[test]
    fn test_lambda_tokens() {
        let toks = toks("|x| x + 1");
        assert_eq!(
            toks,
            vec![
                Token::Pipe,
                Token::Ident("x".to_string()),
                Token::Pipe,
                Token::Ident("x".to_string()),
                Token::Plus,
                Token::Int(1),
            ]
        );
    }

    #[test]
    fn test_lambda_with_type_annotation() {
        let toks = toks("|x: Int32| -> Int32 x");
        assert_eq!(
            toks,
            vec![
                Token::Pipe,
                Token::Ident("x".to_string()),
                Token::Colon,
                Token::Ident("Int32".to_string()),
                Token::Pipe,
                Token::Arrow,
                Token::Ident("Int32".to_string()),
                Token::Ident("x".to_string()),
            ]
        );
    }

    #[test]
    fn test_struct_keyword() {
        let toks = toks("struct");
        assert_eq!(toks, vec![Token::Struct]);
    }

    #[test]
    fn test_struct_definition_tokens() {
        let toks = toks("struct Point { x: Int32, y: Int32 }");
        assert_eq!(
            toks,
            vec![
                Token::Struct,
                Token::Ident("Point".to_string()),
                Token::LBrace,
                Token::Ident("x".to_string()),
                Token::Colon,
                Token::Ident("Int32".to_string()),
                Token::Comma,
                Token::Ident("y".to_string()),
                Token::Colon,
                Token::Ident("Int32".to_string()),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_enum_keyword() {
        let toks = toks("enum");
        assert_eq!(toks, vec![Token::Enum]);
    }

    #[test]
    fn test_type_keyword() {
        let toks = toks("type");
        assert_eq!(toks, vec![Token::Type]);
    }

    #[test]
    fn test_type_alias_tokens() {
        let toks = toks("type UserId = Int");
        assert_eq!(
            toks,
            vec![
                Token::Type,
                Token::Ident("UserId".to_string()),
                Token::Eq,
                Token::Ident("Int".to_string()),
            ]
        );
    }

    #[test]
    fn test_colon_colon() {
        let toks = toks("::");
        assert_eq!(toks, vec![Token::ColonColon]);
    }

    #[test]
    fn test_colon_vs_colon_colon() {
        let toks = toks(": :: :");
        assert_eq!(toks, vec![Token::Colon, Token::ColonColon, Token::Colon]);
    }

    #[test]
    fn test_qualified_path() {
        let toks = toks("Option::Some");
        assert_eq!(
            toks,
            vec![
                Token::Ident("Option".to_string()),
                Token::ColonColon,
                Token::Ident("Some".to_string()),
            ]
        );
    }

    #[test]
    fn test_enum_definition_tokens() {
        let toks = toks("enum Option<T> { None, Some(T) }");
        assert_eq!(
            toks,
            vec![
                Token::Enum,
                Token::Ident("Option".to_string()),
                Token::Lt,
                Token::Ident("T".to_string()),
                Token::Gt,
                Token::LBrace,
                Token::Ident("None".to_string()),
                Token::Comma,
                Token::Ident("Some".to_string()),
                Token::LParen,
                Token::Ident("T".to_string()),
                Token::RParen,
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_mod_keyword() {
        let toks = toks("mod");
        assert_eq!(toks, vec![Token::Mod]);
    }

    #[test]
    fn test_mod_declaration_tokens() {
        let toks = toks("mod foo");
        assert_eq!(toks, vec![Token::Mod, Token::Ident("foo".to_string())]);
    }

    #[test]
    fn test_use_keyword() {
        let toks = toks("use");
        assert_eq!(toks, vec![Token::Use]);
    }

    #[test]
    fn test_use_statement_tokens() {
        let toks = toks("use root::foo::bar");
        assert_eq!(
            toks,
            vec![
                Token::Use,
                Token::Root,
                Token::ColonColon,
                Token::Ident("foo".to_string()),
                Token::ColonColon,
                Token::Ident("bar".to_string()),
            ]
        );
    }

    #[test]
    fn test_pub_keyword() {
        let toks = toks("pub");
        assert_eq!(toks, vec![Token::Pub]);
    }

    #[test]
    fn test_pub_fn_tokens() {
        let toks = toks("pub fn foo() 1");
        assert_eq!(
            toks,
            vec![
                Token::Pub,
                Token::Fn,
                Token::Ident("foo".to_string()),
                Token::LParen,
                Token::RParen,
                Token::Int(1),
            ]
        );
    }

    #[test]
    fn test_root_keyword() {
        let toks = toks("root");
        assert_eq!(toks, vec![Token::Root]);
    }

    #[test]
    fn test_self_keyword() {
        let toks = toks("self");
        assert_eq!(toks, vec![Token::Self_]);
    }

    #[test]
    fn test_super_keyword() {
        let toks = toks("super");
        assert_eq!(toks, vec![Token::Super]);
    }

    #[test]
    fn test_root_path_tokens() {
        let toks = toks("root::foo");
        assert_eq!(
            toks,
            vec![
                Token::Root,
                Token::ColonColon,
                Token::Ident("foo".to_string()),
            ]
        );
    }

    #[test]
    fn test_self_path_tokens() {
        let toks = toks("self::bar");
        assert_eq!(
            toks,
            vec![
                Token::Self_,
                Token::ColonColon,
                Token::Ident("bar".to_string()),
            ]
        );
    }

    #[test]
    fn test_super_path_tokens() {
        let toks = toks("super::baz");
        assert_eq!(
            toks,
            vec![
                Token::Super,
                Token::ColonColon,
                Token::Ident("baz".to_string()),
            ]
        );
    }

    #[test]
    fn test_line_comment() {
        let toks = toks("// this is a comment");
        assert_eq!(toks, vec![]);
    }

    #[test]
    fn test_comment_after_code() {
        let toks = toks("42 // comment");
        assert_eq!(toks, vec![Token::Int(42)]);
    }

    #[test]
    fn test_comment_before_code() {
        let toks = toks("// comment\n42");
        assert_eq!(toks, vec![Token::Int(42)]);
    }

    #[test]
    fn test_multiple_comments() {
        let toks = toks("// first\n1\n// second\n2");
        assert_eq!(toks, vec![Token::Int(1), Token::Int(2)]);
    }

    #[test]
    fn test_comment_with_operators() {
        let toks = toks("// + - * /");
        assert_eq!(toks, vec![]);
    }

    #[test]
    fn test_empty_comment() {
        let toks = toks("//");
        assert_eq!(toks, vec![]);
    }

    #[test]
    fn test_spans_simple_expression() {
        let spanned = lex("42 + 3").unwrap();
        assert_eq!(spanned.len(), 3);
        assert_eq!(spanned[0], (Token::Int(42), 0..2));
        assert_eq!(spanned[1], (Token::Plus, 3..4));
        assert_eq!(spanned[2], (Token::Int(3), 5..6));
    }

    #[test]
    fn test_spans_identifier_and_keyword() {
        let spanned = lex("fn foo").unwrap();
        assert_eq!(spanned.len(), 2);
        assert_eq!(spanned[0], (Token::Fn, 0..2));
        assert_eq!(spanned[1], (Token::Ident("foo".to_string()), 3..6));
    }

    #[test]
    fn test_spans_string_literal() {
        let spanned = lex(r#""hello" + 1"#).unwrap();
        assert_eq!(spanned[0], (Token::String("hello".to_string()), 0..7));
        assert_eq!(spanned[1], (Token::Plus, 8..9));
        assert_eq!(spanned[2], (Token::Int(1), 10..11));
    }

    #[test]
    fn test_lex_error_span() {
        let err = lex("2 + $").unwrap_err();
        match err {
            LexError::UnexpectedCharacter { slice, span } => {
                assert_eq!(slice, "$");
                assert_eq!(span, 4..5);
            }
        }
    }

    #[test]
    fn test_tuple_index_single() {
        let toks = toks("t.0");
        assert_eq!(
            toks,
            vec![Token::Ident("t".to_string()), Token::Dot, Token::Int(0)]
        );
    }

    #[test]
    fn test_tuple_index_chained() {
        // t.0.1 lexes as Ident, Dot, Float(0.1) → Ident, Dot, Int(0), Dot, Int(1)
        let toks = toks("t.0.1");
        assert_eq!(
            toks,
            vec![
                Token::Ident("t".to_string()),
                Token::Dot,
                Token::Int(0),
                Token::Dot,
                Token::Int(1),
            ]
        );
    }

    #[test]
    fn test_standalone_float_unaffected() {
        // Standalone 0.1 has no preceding Dot, so it remains Float
        let toks = toks("0.1");
        assert_eq!(toks, vec![Token::Float(0.1)]);
    }

    #[test]
    fn test_tuple_index_larger_number() {
        let toks = toks("t.0.10");
        assert_eq!(
            toks,
            vec![
                Token::Ident("t".to_string()),
                Token::Dot,
                Token::Int(0),
                Token::Dot,
                Token::Int(10),
            ]
        );
    }
}
