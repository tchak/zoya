use logos::Logos;

fn parse_float(lex: &logos::Lexer<Token>) -> Option<f64> {
    lex.slice().replace('_', "").parse::<f64>().ok()
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
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    Some(result)
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")]
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

    // String literals with escape sequences
    #[regex(r#""([^"\\]|\\.)*""#, parse_string)]
    String(String),

    // Float must come before Int for proper matching of `.5`
    #[regex(r"[0-9][0-9_]*\.[0-9_]*|[0-9_]*\.[0-9][0-9_]*", parse_float)]
    Float(f64),

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

    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("->")]
    Arrow,

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

    #[token(":")]
    Colon,

    #[token(",")]
    Comma,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
}

pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(input);

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => tokens.push(token),
            Err(()) => {
                return Err(LexError {
                    message: format!("unexpected character at '{}'", lexer.slice()),
                });
            }
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_integer() {
        let tokens = lex("42").unwrap();
        assert_eq!(tokens, vec![Token::Int(42)]);
    }

    #[test]
    fn test_large_integer() {
        let tokens = lex("123456789").unwrap();
        assert_eq!(tokens, vec![Token::Int(123456789)]);
    }

    #[test]
    fn test_all_operators() {
        let tokens = lex("+ - * /").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Plus, Token::Minus, Token::Star, Token::Slash]
        );
    }

    #[test]
    fn test_parentheses() {
        let tokens = lex("()").unwrap();
        assert_eq!(tokens, vec![Token::LParen, Token::RParen]);
    }

    #[test]
    fn test_full_expression() {
        let tokens = lex("2 + 3 * (4 - 1)").unwrap();
        assert_eq!(
            tokens,
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
        let tokens = lex("1+2*3").unwrap();
        assert_eq!(
            tokens,
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
        let result = lex("2 + @");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("@"));
    }

    #[test]
    fn test_integer_with_underscores() {
        let tokens = lex("1_000_000").unwrap();
        assert_eq!(tokens, vec![Token::Int(1_000_000)]);
    }

    #[test]
    fn test_integer_with_single_underscore() {
        let tokens = lex("1_0").unwrap();
        assert_eq!(tokens, vec![Token::Int(10)]);
    }

    #[test]
    fn test_integer_with_trailing_underscore() {
        let tokens = lex("100_").unwrap();
        assert_eq!(tokens, vec![Token::Int(100)]);
    }

    #[test]
    fn test_float_standard() {
        let tokens = lex("3.14").unwrap();
        assert_eq!(tokens, vec![Token::Float(3.14)]);
    }

    #[test]
    fn test_float_leading_dot() {
        let tokens = lex(".5").unwrap();
        assert_eq!(tokens, vec![Token::Float(0.5)]);
    }

    #[test]
    fn test_float_trailing_dot() {
        let tokens = lex("1.").unwrap();
        assert_eq!(tokens, vec![Token::Float(1.0)]);
    }

    #[test]
    fn test_float_with_underscores() {
        let tokens = lex("1_000.5").unwrap();
        assert_eq!(tokens, vec![Token::Float(1000.5)]);
    }

    #[test]
    fn test_float_expression() {
        let tokens = lex("1.5 + .5").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Float(1.5), Token::Plus, Token::Float(0.5)]
        );
    }

    #[test]
    fn test_fn_keyword() {
        let tokens = lex("fn").unwrap();
        assert_eq!(tokens, vec![Token::Fn]);
    }

    #[test]
    fn test_identifier() {
        let tokens = lex("foo").unwrap();
        assert_eq!(tokens, vec![Token::Ident("foo".to_string())]);
    }

    #[test]
    fn test_identifier_with_underscore() {
        let tokens = lex("foo_bar").unwrap();
        assert_eq!(tokens, vec![Token::Ident("foo_bar".to_string())]);
    }

    #[test]
    fn test_identifier_starting_with_underscore() {
        let tokens = lex("_foo").unwrap();
        assert_eq!(tokens, vec![Token::Ident("_foo".to_string())]);
    }

    #[test]
    fn test_identifier_with_numbers() {
        let tokens = lex("foo123").unwrap();
        assert_eq!(tokens, vec![Token::Ident("foo123".to_string())]);
    }

    #[test]
    fn test_fn_not_identifier() {
        // fn should be keyword, not identifier
        let tokens = lex("fn foo").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Fn, Token::Ident("foo".to_string())]
        );
    }

    #[test]
    fn test_arrow() {
        let tokens = lex("->").unwrap();
        assert_eq!(tokens, vec![Token::Arrow]);
    }

    #[test]
    fn test_braces() {
        let tokens = lex("{}").unwrap();
        assert_eq!(tokens, vec![Token::LBrace, Token::RBrace]);
    }

    #[test]
    fn test_angle_brackets() {
        let tokens = lex("<>").unwrap();
        assert_eq!(tokens, vec![Token::Lt, Token::Gt]);
    }

    #[test]
    fn test_colon_and_comma() {
        let tokens = lex(":,").unwrap();
        assert_eq!(tokens, vec![Token::Colon, Token::Comma]);
    }

    #[test]
    fn test_function_signature() {
        let tokens = lex("fn add(x: Int, y: Int) -> Int { x + y }").unwrap();
        assert_eq!(
            tokens,
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
        let tokens = lex("fn identity<T>(x: T) -> T { x }").unwrap();
        assert_eq!(
            tokens,
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
        let tokens = lex("true false").unwrap();
        assert_eq!(tokens, vec![Token::True, Token::False]);
    }

    #[test]
    fn test_string_literal() {
        let tokens = lex(r#""hello""#).unwrap();
        assert_eq!(tokens, vec![Token::String("hello".to_string())]);
    }

    #[test]
    fn test_string_with_escapes() {
        let tokens = lex(r#""hello\nworld""#).unwrap();
        assert_eq!(tokens, vec![Token::String("hello\nworld".to_string())]);
    }

    #[test]
    fn test_string_with_escaped_quote() {
        let tokens = lex(r#""say \"hi\"""#).unwrap();
        assert_eq!(tokens, vec![Token::String("say \"hi\"".to_string())]);
    }

    #[test]
    fn test_comparison_operators() {
        let tokens = lex("== != <= >= < >").unwrap();
        assert_eq!(
            tokens,
            vec![Token::EqEq, Token::Ne, Token::Le, Token::Ge, Token::Lt, Token::Gt]
        );
    }

    #[test]
    fn test_comparison_expression() {
        let tokens = lex("1 == 2").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Int(1), Token::EqEq, Token::Int(2)]
        );
    }

    #[test]
    fn test_le_ge_not_arrow() {
        // Make sure <= and >= don't interfere with -> or <>
        let tokens = lex("<= >= -> < >").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Le, Token::Ge, Token::Arrow, Token::Lt, Token::Gt]
        );
    }
}
