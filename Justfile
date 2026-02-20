# Run all tests (Rust + JS + tree-sitter)
test-all: test test-runtime test-grammar

# Run Rust tests
test *args:
    cargo test --workspace {{args}}

# Run JS runtime tests
test-runtime:
    cd packages/zoya-runtime && npm test

# Run tree-sitter grammar tests
test-grammar:
    cd editors/tree-sitter-zoya && npx tree-sitter test

# Lint and check formatting
lint:
    cargo clippy --workspace
    cargo fmt --check

# Format Rust code
fmt:
    cargo fmt

# Build JS runtime bundle
build-runtime:
    cd packages/zoya-runtime && npm run build

# Regenerate tree-sitter parser from grammar.js
generate-grammar:
    cd editors/tree-sitter-zoya && npx tree-sitter generate

# Parse all std & example files with tree-sitter
parse-all:
    cd editors/tree-sitter-zoya && npm run parse-all

# Generate coverage report
coverage:
    cargo llvm-cov --workspace --html
