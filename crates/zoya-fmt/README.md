# zoya-fmt

Source code formatter for the Zoya programming language.

Produces canonical pretty-printed Zoya source code using the [pretty](https://github.com/Marwes/pretty.rs) crate for optimal line-width layout.

## Features

- **Canonical ordering** - Mod declarations first, then use declarations, then other items
- **Visibility sorting** - `pub` items before private items within each group
- **Pretty printing** - Optimal line breaking at 120 character width
- **Idempotent** - Formatting already-formatted code produces identical output

## Usage

```rust
use zoya_lexer::lex;
use zoya_parser::parse_module;
use zoya_fmt::fmt;

// Parse a source file
let tokens = lex(source).unwrap();
let items = parse_module(tokens).unwrap();

// Format to canonical source
let formatted = fmt(items);
println!("{}", formatted);
```

## Ordering Rules

1. **Mod declarations** first (`pub` before private, original order preserved within each group)
2. **Use declarations** next (`pub` before private, original order preserved within each group)
3. **Other items** last (original parsed order preserved)

Blank lines: none between consecutive mods, none between consecutive uses, blank line between each other item. Blank line separating groups. Trailing newline.

## Public API

| Function | Description |
|----------|-------------|
| `fmt(items)` | Format parsed items into canonical source code |

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [pretty](https://github.com/Marwes/pretty.rs) - Pretty printing library
