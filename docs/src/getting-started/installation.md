# Installation

Zoya requires [Rust](https://rustup.rs/) 1.85 or later.

## Building from Source

```bash
git clone https://github.com/tchak/zoya-lang
cd zoya-lang
cargo build --release
```

The binary will be at `target/release/zoya`.

## Adding to PATH

To use `zoya` from anywhere, add it to your PATH:

```bash
# Add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
export PATH="$PATH:/path/to/zoya-lang/target/release"
```

## Verifying Installation

```bash
zoya --help
```

You should see the available commands listed.
