# Run all tests (Rust + JS + dashboard + tree-sitter)
test-all: test test-runtime test-dashboard test-grammar

# Run Rust tests
test *args:
    cargo test --workspace {{ args }}

# Run JS runtime tests
test-runtime:
    cd packages/zoya-runtime && npm test

# Run dashboard tests
test-dashboard:
    cd packages/zoya-dashboard && npm test

# Run tree-sitter grammar tests
test-grammar:
    cd editors/tree-sitter-zoya && npx tree-sitter test

# Lint and check formatting
lint:
    cargo clippy --workspace
    cargo fmt --check
    cd packages/zoya-runtime && npm run lint
    cd packages/zoya-runtime && npm run format:check
    cd packages/zoya-dashboard && npm run lint
    cd packages/zoya-dashboard && npm run format:check

# Format code
fmt:
    cargo fmt
    cd packages/zoya-runtime && npm run format
    cd packages/zoya-dashboard && npm run format

# Build JS runtime bundle
build-runtime:
    cd packages/zoya-runtime && npm run build

# Build dashboard SPA
build-dashboard:
    cd packages/zoya-dashboard && npm run build

# Regenerate tree-sitter parser from grammar.js
generate-grammar:
    cd editors/tree-sitter-zoya && npx tree-sitter generate

# Parse all std & example files with tree-sitter
parse-all:
    cd editors/tree-sitter-zoya && npm run parse-all

# Install local copy of zoya CLI
install:
    cargo install --path crates/zoya

# Run dev server on examples/tests
dev-tests:
    cargo run -p zoya -- dev --package examples/tests

# Run dev server on examples/std-tests
dev-std-tests:
    cargo run -p zoya -- dev --package examples/std-tests

# Generate coverage report
coverage:
    cargo llvm-cov --workspace --html

# Count lines of code
loc:
    tokei -e *.c -e *.h
