# zoya-lexer

Lexer/tokenizer for the Zoya programming language.

Converts source text into a stream of tokens using [Logos](https://github.com/maciejhirsz/logos).

## Tokens

- **Keywords** - `fn`, `let`, `match`, `struct`, `enum`, `type`, `true`, `false`
- **Literals** - integers, floats, bigints, strings
- **Operators** - `+`, `-`, `*`, `/`, `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Delimiters** - `()`, `{}`, `[]`, `<>`, `,`, `:`, `::`, `.`, `..`, `|`, `@`
- **Arrows** - `->`, `=>`

## Usage

```rust
use zoya_lexer::{lex, Token};

let tokens = lex("fn main() { 42 }").unwrap();
```
