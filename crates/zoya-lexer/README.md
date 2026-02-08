# zoya-lexer

Lexer/tokenizer for the Zoya programming language.

Converts source text into a stream of tokens using [Logos](https://github.com/maciejhirsz/logos).

## Tokens

- **Keywords** - `fn`, `let`, `match`, `struct`, `enum`, `type`, `mod`, `use`, `pub`, `true`, `false`, `root`, `self`, `super`
- **Literals** - Integers (`42`, `1_000`), floats (`3.14`), bigints (`42n`), strings (`"hello"`)
- **Operators** - `+`, `-`, `*`, `/`, `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Delimiters** - `()`, `{}`, `[]`, `<>`, `,`, `:`, `::`, `.`, `..`, `|`, `@`
- **Arrows** - `->`, `=>`
- **Comments** - Line comments (`// ...`)

## Usage

```rust
use zoya_lexer::{lex, Token};

// Tokenize source code
let tokens = lex("fn main() -> Int { 42 }").unwrap();

// Tokens are returned as Vec<Token>
assert!(matches!(tokens[0], Token::Fn));
assert!(matches!(tokens[1], Token::Ident(ref s) if s == "main"));
assert!(matches!(tokens[2], Token::LParen));

// Comments are stripped during lexing
let tokens = lex("fn main() -> Int { 42 } // this is ignored").unwrap();
```

## Error Handling

```rust
use zoya_lexer::lex;

// Returns LexError on invalid input
let result = lex("let x = #invalid");
assert!(result.is_err());

let err = result.unwrap_err();
println!("Lexer error: {}", err.message);
```

## Dependencies

- [logos](https://github.com/maciejhirsz/logos) - Fast lexer generator
