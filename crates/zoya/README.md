# zoya

The Zoya programming language CLI.

This is the main binary crate that provides the `zoya` CLI tool for compiling and running Zoya programs.

## Commands

```bash
zoya repl                      # Start interactive REPL
zoya repl file.zoya            # REPL with file preloaded
zoya run file.zoya             # Execute a file
zoya check file.zoya           # Type-check without running
zoya build file.zoya           # Compile to JavaScript (stdout)
zoya build file.zoya -o out.js # Compile to file
```

## REPL Features

- Persistent function and type definitions across inputs
- Let bindings accumulate in scope
- Load files with `zoya repl file.zoya`
- Expression evaluation with immediate results

```
> fn double(x: Int) -> Int { x * 2 }
defined: double
> double(21)
42
> let nums = [1, 2, 3]
let nums: List<Int>
> nums.len()
3
```

## Programmatic Usage

For programmatic execution, use the [zoya-run](../zoya-run) crate:

```rust
use zoya_run::{run_source, run_file, Value};
use std::path::Path;

// Run from source string
let result = run_source("fn main() -> Int { 42 }")?;
assert_eq!(result, Value::Int(42));

// Run from file
let result = run_file(Path::new("program.zoya"))?;
println!("Result: {}", result);
```

## Dependencies

- [zoya-ast](../zoya-ast) - AST types
- [zoya-check](../zoya-check) - Type checker
- [zoya-codegen](../zoya-codegen) - JavaScript code generation
- [zoya-ir](../zoya-ir) - Typed IR and type definitions
- [zoya-lexer](../zoya-lexer) - Tokenizer
- [zoya-loader](../zoya-loader) - Package file loading
- [zoya-package](../zoya-package) - Package data structures
- [zoya-parser](../zoya-parser) - Parser
- [zoya-run](../zoya-run) - Runtime execution
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [rustyline](https://github.com/kkawakam/rustyline) - REPL line editing
