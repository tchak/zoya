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
use zoya_parser::{parse_module, parse_input};

// Parse a module file (for .zoya files)
let tokens = lex("mod utils\nfn main() -> Int { 42 }").unwrap();
let module = parse_module(tokens).unwrap();
// module.mods - mod declarations
// module.uses - use declarations
// module.items - function/struct/enum definitions

// Parse REPL input (expressions, let bindings, and items)
let tokens = lex("let x = 1 + 2").unwrap();
let (items, stmts) = parse_input(tokens).unwrap();
// items - any function/struct/enum definitions
// stmts - expressions and let bindings
```

## Parsing Functions

| Function | Input | Output |
|----------|-------|--------|
| `parse_module` | Module file tokens | `ModuleDef` with mods, uses, items |
| `parse_input` | REPL input tokens | `(Vec<Item>, Vec<Stmt>)` |

## Error Handling

```rust
use zoya_lexer::lex;
use zoya_parser::parse_module;

let tokens = lex("fn fn fn").unwrap();
let result = parse_module(tokens);
assert!(result.is_err());

let err = result.unwrap_err();
println!("Parse error: {}", err.message);
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-lexer](../zoya-lexer) - Token types
- [chumsky](https://github.com/zesterer/chumsky) - Parser combinator library
