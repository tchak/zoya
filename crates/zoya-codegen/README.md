# zoya-codegen

JavaScript code generation for the Zoya programming language.

Transforms typed IR into executable JavaScript code.

## Features

- **Package codegen** - Generates JS for complete packages
- **Pattern matching** - Compiles patterns to JS conditionals and bindings
- **Function generation** - Handles generic functions, lambdas, and closures
- **Runtime helpers** - Includes prelude functions for deep equality, division checks, etc.

## Usage

```rust
use zoya_codegen::codegen;

// Generate JS for a complete package (includes prelude)
let js_code = codegen(&checked_package);
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types (for operators)
- [zoya-ir](../zoya-ir) - Typed IR types
- [zoya-package](../zoya-package) - Module path types
