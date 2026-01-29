# zoya

The Zoya programming language compiler and runtime.

This is the main binary crate that provides the `zoya` CLI tool.

## Components

- **Type checker** - Hindley-Milner type inference with unification
- **Pattern exhaustiveness** - Ensures all cases are covered (Maranget algorithm)
- **Code generator** - Compiles to JavaScript
- **Runtime** - Executes JS via QuickJS (rquickjs)
- **REPL** - Interactive development environment

## Commands

```bash
zoya run              # Start REPL
zoya run file.zoya    # Execute a file
zoya check file.zoya  # Type-check only
zoya build file.zoya  # Compile to JavaScript
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-parser](../zoya-parser) - Parser
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [rquickjs](https://github.com/deliro/rquickjs) - JavaScript runtime
- [rustyline](https://github.com/kkawakam/rustyline) - REPL line editing
