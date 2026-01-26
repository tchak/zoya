use logos::Logos;

fn parse_float(lex: &logos::Lexer<Token>) -> Option<f64> {
    lex.slice().replace('_', "").parse::<f64>().ok()
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\n\r]+")]
pub enum Token {
    // Float must come before Int for proper matching of `.5`
    #[regex(r"[0-9][0-9_]*\.[0-9_]*|[0-9_]*\.[0-9][0-9_]*", parse_float)]
    Float(f64),

    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    Int(i64),

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,
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
}
