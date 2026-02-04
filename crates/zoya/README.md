# zoya

The Zoya programming language compiler and runtime.

This is the main binary crate that provides the `zoya` CLI tool for compiling and running Zoya programs.

## Components

- **Type checker** - Hindley-Milner type inference with unification
- **Pattern exhaustiveness** - Ensures all cases are covered (Maranget algorithm)
- **Code generator** - Compiles to JavaScript (ESM)
- **Runtime** - Executes JS via QuickJS (rquickjs)
- **REPL** - Interactive development environment with persistent state

## Commands

```bash
zoya repl                      # Start interactive REPL
zoya repl file.zoya            # REPL with file preloaded
zoya run file.zoya             # Execute a file
zoya check file.zoya           # Type-check without running
zoya build file.zoya           # Compile to JavaScript (stdout)
zoya build file.zoya -o out.js # Compile to file
```

## Library Usage

The crate also exposes a library API for programmatic use:

```rust
use zoya::runner::{run, RunInput};

// Run from source string
let result = run(RunInput::Source("fn main() -> Int { 42 }"))?;
println!("Result: {:?}", result);

// Run from file
let result = run(RunInput::File(Path::new("program.zoya")))?;
```

## REPL Features

- Persistent function and type definitions across inputs
- Let bindings accumulate in scope
- Load files with `zoya repl file.zoya`
- Expression evaluation with immediate results

```
> fn double(x: Int) -> Int { x * 2 }
fn double: Int -> Int
> double(21)
42
> let nums = [1, 2, 3]
let nums: List<Int>
> nums.len()
3
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
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [rquickjs](https://github.com/DelSkaorth/rquickjs) - JavaScript runtime
- [rustyline](https://github.com/kkawakam/rustyline) - REPL line editing
