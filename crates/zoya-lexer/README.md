# zoya-lexer

Lexer/tokenizer for the Zoya programming language.

Converts source text into a stream of spanned tokens using [Logos](https://github.com/maciejhirsz/logos).

## Tokens

- **Keywords** - `fn`, `let`, `match`, `struct`, `enum`, `type`, `mod`, `use`, `pub`, `impl`, `true`, `false`, `root`, `self`, `super`
- **Literals** - Integers (`42`, `1_000`), floats (`3.14`), bigints (`42n`), strings (`"hello"`), interpolated strings (`$"hello {name}!"`)
- **Operators** - `+`, `-`, `*`, `/`, `%`, `**`, `++`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`
- **Delimiters** - `()`, `{}`, `[]`, `<>`, `,`, `:`, `::`, `.`, `..`, `|`, `@`, `#`
- **Arrows** - `->`, `=>`
- **Comments** - Line comments (`// ...`)

## Usage

```rust
use zoya_lexer::{lex, Token, Span, Spanned};

// Tokenize source code — returns Vec<Spanned> where Spanned = (Token, Span)
let tokens = lex("fn main() -> Int { 42 }").unwrap();

// Each token carries its byte-offset span
let (token, span) = &tokens[0];
assert!(matches!(token, Token::Fn));

assert!(matches!(&tokens[1], (Token::Ident(s), _) if s == "main"));
assert!(matches!(&tokens[2].0, Token::LParen));

// Comments are stripped during lexing
let tokens = lex("fn main() -> Int { 42 } // this is ignored").unwrap();

// Interpolated strings produce a single token with parts
let tokens = lex(r#"$"hello {name}!""#).unwrap();
assert!(matches!(&tokens[0].0, Token::InterpolatedString(_)));
```

## Error Handling

```rust
use zoya_lexer::{lex, LexError};

// Returns LexError on invalid input
let result = lex("let x = #invalid");
assert!(result.is_err());

// LexError variants provide the offending slice and span
let err = result.unwrap_err();
println!("Lexer error: {}", err);
```

## Dependencies

- [logos](https://github.com/maciejhirsz/logos) - Fast lexer generator
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
