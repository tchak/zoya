# zoya-codegen

JavaScript code generation for the Zoya programming language.

Transforms typed IR into executable JavaScript code.

## Features

- **Expression codegen** - Generates JS for all typed expressions
- **Pattern matching** - Compiles patterns to JS conditionals and bindings
- **Function generation** - Handles generic functions, lambdas, and closures
- **Runtime helpers** - Provides prelude functions for deep equality, division checks, etc.

## Usage

```rust
use zoya_codegen::{codegen, codegen_items, codegen_let, prelude};

// Generate JS for all function definitions
let js_code = codegen_items(&checked_items);

// Generate JS for a single expression
let js_expr = codegen(&typed_expr);

// Get runtime helper functions
let helpers = prelude();
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for operators)
- [zoya-ir](../zoya-ir) - Typed IR types
