# Test Performance Audit

## Overall: 1789 tests in ~7.5s wall-clock

The vast majority of crates are fast. The bottleneck is **one test binary**: `examples.rs` (3 Rust tests wrapping **487 Zoya test functions**) taking ~7.5s.

## Measured Breakdown

| Suite | Tests | Build | Run | Per-test |
|-------|------:|------:|----:|--------:|
| `algorithms` | 17 | 174ms | 107ms | 6.3ms |
| `std-tests` | 150 | 176ms | 1.1s | 7.3ms |
| `tests` | 320 | **1.92s** | **3.98s** | 12.4ms |
| **Total** | **487** | **2.27s** | **5.18s** | |

Two distinct bottlenecks:

## 1. Per-test QuickJS runtime creation (~70% of run time)

Each of the 487 test functions creates a **fresh tokio runtime + QuickJS VM + re-evaluates all JS code**, then calls just one function. The entire compiled JS bundle (50-120KB) is parsed and executed 487 times.

The hot path per test (`eval.rs:92-165`):
1. `AsyncRuntime::new()` — new QuickJS heap
2. `AsyncContext::full()` — new JS context
3. `ctx.eval(code)` — **re-parses & evaluates the entire JS bundle**
4. `$$run(path)` — calls one test function
5. Everything is dropped

**Optimization**: Reuse a single QuickJS context across tests in `TestRunner::execute()`. Build once → eval JS once → call `$$run()` N times. This would eliminate 486 redundant JS parse+eval cycles for the `tests` suite alone.

## 2. Build time for `examples/tests` (1.92s)

The `tests` package has 320 tests across many files (~3400 lines), and type-checking it is the slow part (the std library itself is cached via `LazyLock`). The `algorithms` and `std-tests` builds are fast (174-176ms) because the std is already cached from the first call.

This is harder to optimize since it's real compilation work, but the build is only ~25% of total time.

## Recommendations (by impact)

### High impact — Reuse QuickJS runtime across tests in TestRunner

This is the single biggest win. Instead of `run_single_test()` calling `zoya_run::run()` (which creates a new runtime each time), `TestRunner` should:
1. Create one tokio runtime + QuickJS context
2. Evaluate the JS code once
3. Loop over tests calling `$$run(path)` on the same context

Expected savings: **~3-4s** (the run phase would drop from 5.18s to ~0.5-1s).

### Medium impact — Parallelize the 3 example suites

Currently Rust's test harness runs them in parallel, but the first test to run pays the std library init cost (~170ms). This is already reasonable, but if they were combined into a single test that builds all three in parallel (using threads or async), it would avoid the thread scheduling overhead.

### Low impact — Everything else is already fast

- `runner_tests` (87 tests, 0.19s) — fine, each test compiles tiny inline source
- `zoya-check` (613 tests, 0.14s) — excellent
- All other crates — negligible

## The one change that matters most

The QuickJS runtime reuse in `TestRunner` would cut the total `cargo test --workspace` time from ~7.5s to ~4s.
