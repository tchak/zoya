# zoya-parser

Parser for the Zoya programming language.

Transforms a token stream into an Abstract Syntax Tree using [Chumsky](https://github.com/zesterer/chumsky).

## Features

- Recursive descent parsing with operator precedence
- Rich pattern matching (lists, tuples, structs, enums, rest patterns, as-patterns)
- Type annotation parsing (generics, function types, tuples)
- Error recovery and reporting

## Usage

```rust
use zoya_lexer::lex;
use zoya_parser::{parse_file, parse_input};

// Parse a file (multiple items)
let tokens = lex("fn main() { 42 }").unwrap();
let items = parse_file(tokens).unwrap();

// Parse REPL input (items followed by expressions or let bindings)
let tokens = lex("1 + 2").unwrap();
let (items, stmts) = parse_input(tokens).unwrap();
```
