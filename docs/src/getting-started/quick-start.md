# Quick Start

## Using the REPL

The fastest way to try Zoya is the interactive REPL:

```bash
zoya repl
```

```
> let greeting = "Hello, Zoya!"
let greeting: String
> greeting.len()
12
> let add = |x, y| x + y
let add: (?0, ?0) -> ?0
> add(1, 2)
3
```

## Running a File

Create a file called `hello.zy`:

```zoya
fn main() -> String {
    "Hello, World!"
}
```

Run it:

```bash
zoya run hello.zy
```

Output:
```
"Hello, World!"
```

## Type Checking

Validate types without executing:

```bash
zoya check program.zy
```

## Compiling to JavaScript

Generate JavaScript output:

```bash
zoya build program.zy           # Output to stdout
zoya build program.zy -o out.js # Output to file
```
