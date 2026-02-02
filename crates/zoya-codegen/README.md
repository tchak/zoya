# zoya-codegen

JavaScript code generation for the Zoya programming language.

Transforms typed IR into executable JavaScript code.

## Features

- **Module codegen** - Generates JS for complete module trees
- **Pattern matching** - Compiles patterns to JS conditionals and bindings
- **Function generation** - Handles generic functions, lambdas, and closures
- **Runtime helpers** - Includes prelude functions for deep equality, division checks, etc.

## Usage

```rust
use zoya_codegen::codegen;

// Generate JS for a complete module tree (includes prelude)
let js_code = codegen(&checked_module_tree);
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for operators)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-module](../zoya-module) - Module path types
