# CLI Reference

## Commands

### `zoya repl`

Start an interactive REPL session.

```bash
zoya repl
```

### `zoya run <file>`

Run a Zoya file. Executes the `main` function and prints its result.

```bash
zoya run program.zoya
```

### `zoya check <file>`

Type-check a file without executing it.

```bash
zoya check program.zoya
```

### `zoya build <file>`

Compile a Zoya file to JavaScript.

```bash
zoya build program.zoya           # Output to stdout
zoya build program.zoya -o out.js # Output to file
```

## Global Options

| Option | Description |
|--------|-------------|
| `--help` | Print help information |
| `--version` | Print version information |
