# CLI Reference

## Commands

### `zoya run`

Run a Zoya file or package. Executes the `pub fn main()` function and prints its result.

```bash
zoya run -p program.zy               # Run a single file
zoya run                             # Run package in current directory (requires package.toml)
zoya run --mode test -p program.zy   # Run in test mode (includes #[test] items)
zoya run --json -p program.zy        # Output result as JSON
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |
| `--mode <mode>` | Compilation mode: `dev` (default), `test`, or `release` |
| `--json` | Output result as JSON instead of the default display format |

### `zoya check`

Type-check without executing.

```bash
zoya check -p program.zy               # Check a single file
zoya check                             # Check package in current directory
zoya check --mode test -p program.zy   # Check in test mode
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |
| `--mode <mode>` | Compilation mode: `dev` (default), `test`, or `release` |

### `zoya build`

Compile to JavaScript.

```bash
zoya build -p program.zy -o out.js          # Output to file
zoya build                                  # Build package in current directory
zoya build --mode release -o out.js         # Build in release mode
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |
| `-o, --output <path>` | Output directory (defaults to `build`) |
| `--mode <mode>` | Compilation mode: `dev` (default), `test`, or `release` |

### `zoya dev`

Start a development HTTP server. Discovers functions annotated with `#[get("/...")]`, `#[post("/...")]`, etc. and serves them as HTTP routes.

```bash
zoya dev                             # Start dev server on port 3000
zoya dev --port 8080                 # Start on a custom port
zoya dev -p my_app/                  # Start from a specific package
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |
| `--port <port>` | Port to listen on (default: `3000`) |

### `zoya repl`

Start an interactive REPL session.

```bash
zoya repl                       # Start REPL
zoya repl -p .                  # Start REPL with package context
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |

### `zoya test`

Run all `#[test]` functions in a package or file. Tests are discovered automatically and executed in sorted order.

```bash
zoya test                        # Run tests in current package
zoya test -p program.zy          # Run tests in a single file
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |

### `zoya task list`

List all `#[task]` functions in a package or file. Shows each task's path and type signature.

```bash
zoya task list                   # List tasks in current package
zoya task list -p program.zy    # List tasks in a single file
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory with `package.toml` (defaults to current directory) |

### `zoya fmt`

Format `.zy` source files. Comments between top-level items and between `impl` methods are preserved. Comments inside function bodies and expressions are stripped.

```bash
zoya fmt -p program.zy         # Format a single file
zoya fmt                       # Format all .zy files in current directory (recursive)
zoya fmt --check               # Check formatting without writing (exit 1 if not formatted)
```

| Option | Description |
|--------|-------------|
| `-p, --package <path>` | Path to a `.zy` file or directory (defaults to current directory) |
| `--check` | Check if files are formatted without writing (exit 1 if not) |

### `zoya init <path>`

Create a new Zoya project with a `package.toml` and `main.zy`.

```bash
zoya init my_project              # Create project (name derived from directory)
zoya init my_project --name app   # Create project with explicit package name
```

## Package Configuration

Zoya projects use a `package.toml` file to define project settings. The configuration lives under the `[package]` table:

```toml
[package]
name = "my-project"
main = "src/main.zy"
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | Yes | — | Package name. Must be lowercase alphanumeric with hyphens or underscores, starting with a letter. Reserved names (`root`, `self`, `super`, `std`, `zoya`) are not allowed. |
| `main` | No | `src/main.zy` | Relative path to the main entry file. |

Create a new project with `zoya init`:

```bash
zoya init my-project
```

This generates a `package.toml` with the `name` field set and a default for `main`.

## Global Options

| Option | Description |
|--------|-------------|
| `--help` | Print help information |
| `--version` | Print version information |
