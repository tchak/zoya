# CLI Reference

## Commands

### `zoya run [path]`

Run a Zoya file or package. Executes the `pub fn main()` function and prints its result.

```bash
zoya run program.zy        # Run a single file
zoya run                   # Run package in current directory (requires package.toml)
```

### `zoya check [path]`

Type-check without executing.

```bash
zoya check program.zy      # Check a single file
zoya check                 # Check package in current directory
```

### `zoya build [path]`

Compile to JavaScript.

```bash
zoya build program.zy             # Output to stdout
zoya build program.zy -o out.js   # Output to file
zoya build                        # Build package in current directory
```

### `zoya repl [path]`

Start an interactive REPL session.

```bash
zoya repl                  # Start REPL
zoya repl .                # Start REPL with package context
```

### `zoya fmt [path]`

Format `.zy` source files.

```bash
zoya fmt program.zy        # Format a single file
zoya fmt                   # Format all .zy files in current directory (recursive)
zoya fmt --check           # Check formatting without writing (exit 1 if not formatted)
```

### `zoya new <path>`

Create a new Zoya project with a `package.toml` and `main.zy`.

```bash
zoya new my_project              # Create project (name derived from directory)
zoya new my_project --name app   # Create project with explicit package name
```

## Global Options

| Option | Description |
|--------|-------------|
| `--help` | Print help information |
| `--version` | Print version information |
