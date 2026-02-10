# zoya-parser

Parser for the Zoya programming language.

Transforms a token stream into an Abstract Syntax Tree using [Chumsky](https://github.com/zesterer/chumsky).

## Features

- Recursive descent parsing with operator precedence
- Rich pattern matching (lists, tuples, structs, enums, rest patterns, as-patterns)
- Type annotation parsing (generics, function types, tuples)
- Module declarations with visibility (`pub mod`)
- Use declarations with path prefixes (`root::`, `self::`, `super::`)
- Error recovery and reporting with byte-offset spans

## Usage

```rust
use zoya_lexer::lex;
use zoya_parser::{parse_module, parse_input};

// Parse a module file (for .zy files)
let tokens = lex("pub mod utils\nuse root::utils::helper\nfn main() -> Int { helper() }").unwrap();
let module = parse_module(tokens).unwrap();
// module.mods - mod declarations (with visibility)
// module.items - function/struct/enum definitions and use declarations

// Parse REPL input (expressions, let bindings, and items)
let tokens = lex("let x = 1 + 2").unwrap();
let (items, stmts) = parse_input(tokens).unwrap();
// items - any function/struct/enum definitions
// stmts - expressions and let bindings
```

## Parsing Functions

| Function | Input | Output |
|----------|-------|--------|
| `parse_module` | Module file tokens | `ModuleDef` with mods and items |
| `parse_input` | REPL input tokens | `(Vec<Item>, Vec<Stmt>)` |

## Error Handling

```rust
use zoya_lexer::lex;
use zoya_parser::{parse_module, ParseError, SyntaxError};

let tokens = lex("fn fn fn").unwrap();
let result = parse_module(tokens);
assert!(result.is_err());

// ParseError contains SyntaxErrors with byte-offset spans
if let Err(ParseError::SyntaxErrors(errors)) = result {
    for err in &errors {
        println!("at {:?}: expected {:?}, found {:?}", err.span, err.expected, err.found);
    }
}
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-lexer](../zoya-lexer) - Token types
- [chumsky](https://github.com/zesterer/chumsky) - Parser combinator library
- [thiserror](https://github.com/dtolnay/thiserror) - Error derive macros
