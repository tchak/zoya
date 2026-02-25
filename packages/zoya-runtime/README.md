# zoya-runtime

JavaScript runtime library for the Zoya programming language.

Provides the runtime functions and data structures that compiled Zoya code depends on when executed in QuickJS. Built with [tsdown](https://github.com/nicolo-ribaudo/tsdown) and bundled into a single module that the Rust `zoya-codegen` crate embeds.

## Features

- **Value conversion** - Bidirectional conversion between Zoya values and JavaScript representations
- **Persistent data structures** - HAMT-backed `Dict` and `Set` for immutable collections
- **Structural equality** - Deep equality comparison for all value types
- **Safe arithmetic** - Checked division, modulo, and power operations
- **JSON bridge** - Convert between Zoya values and JSON
- **Type method implementations** - Methods for Int, Float, BigInt, String, List, Bytes

## Modules

| Module | Exports | Description |
|--------|---------|-------------|
| `zoya.ts` | `$$zoya_to_js`, `$$js_to_zoya`, `$$run`, `$$enqueue` | Core value conversion and function execution |
| `hamt.ts` | `$$Dict` | Persistent hash array mapped trie for `Dict<K, V>` |
| `set.ts` | `$$Set` | Persistent hash set for `Set<T>` |
| `equality.ts` | `$$eq`, `$$is_obj` | Structural equality and type checking |
| `arithmetic.ts` | `$$div`, `$$mod`, `$$pow` + BigInt variants | Safe arithmetic with division-by-zero checks |
| `json.ts` | `$$json_to_zoya`, `$$zoya_to_json` | JSON serialization and deserialization |
| `list.ts` | `$$List` | List utility methods |
| `list-idx.ts` | `$$list_idx` | Safe list indexing with bounds checking |
| `int.ts` | `$$Int` | Integer operations and conversions |
| `float.ts` | `$$Float` | Float operations |
| `bigint.ts` | `$$BigInt` | BigInt operations and conversions |
| `string.ts` | `$$String` | String operations |
| `bytes.ts` | `$$Bytes` | Byte array (`Uint8Array`) utilities |
| `task.ts` | `$$Task` | Async task wrapper |
| `error.ts` | `$$ZoyaError`, `$$throw` | Error handling |

## Build

```bash
npm run build       # Bundle with tsdown to dist/
npm test            # Run tests with vitest
npm run typecheck   # Type-check with tsc
npm run lint        # Lint with eslint
npm run format      # Format with prettier
```

The built bundle in `dist/` is committed to the repository and embedded by the Rust `zoya-codegen` crate at compile time.

## Integration

The runtime is loaded into QuickJS by `zoya-run` before executing any user code. All exports are attached to the global scope with `$$` prefixes to avoid name collisions with user code. The `zoya-codegen` crate generates calls to these runtime functions (e.g., `$$eq()` for equality, `$$Dict` for dict operations).
