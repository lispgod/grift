# Roadmap

This document tracks planned work for Grift, organized by area. Items are
roughly priority-ordered within each section but there is no fixed timeline.

## Language Features

### String and Character Operations

The `grift_unicode` crate already provides case mapping, case folding, and
character property queries. The next step is wiring it into the interpreter
and exposing string/character builtins:

- [ ] `char?` type predicate
- [ ] `char=?`, `char<?`, `char>?`, `char<=?`, `char>=?` comparisons
- [ ] `char->integer`, `integer->char` conversions
- [ ] `char-alphabetic?`, `char-numeric?`, `char-whitespace?`, `char-upper-case?`, `char-lower-case?`
- [ ] `char-upcase`, `char-downcase`, `char-foldcase`
- [ ] `string?` type predicate
- [ ] `string-length`, `string-ref`
- [ ] `string=?`, `string<?`, `string>?`, `string<=?`, `string>=?`
- [ ] `string-append`, `substring`
- [ ] `string->symbol`, `symbol->string`
- [ ] `number->string`, `string->number`

### Additional List Operations

The standard library defines `map`, `filter`, `length`, and `append`. More
list utilities would close gaps with the Kernel spec:

- [ ] `reverse`
- [ ] `assoc` — association list lookup
- [ ] `member` — membership test
- [ ] `for-each` — side-effecting list traversal
- [ ] `reduce` — fold a list with a binary combiner
- [ ] `list-tail`, `list-ref`
- [ ] `zip` — interleave multiple lists

### Numeric Extensions

Grift currently supports checked integer (`isize`) arithmetic. Future work
may extend the numeric tower:

- [ ] `abs`, `min`, `max`
- [ ] `modulo`, `remainder`
- [ ] `zero?`, `positive?`, `negative?`, `odd?`, `even?`
- [ ] Investigate rational or fixed-point support for embedded use

### Error Handling

Kernel specifies a guard/handler system for errors. Currently Grift propagates
errors as Rust `Result` values with no user-level handler mechanism:

- [ ] `guard-continuation` or equivalent error handler
- [ ] `error-object?`, `error-object-message`
- [ ] User-defined error types (tagged error values)

### Pattern Matching and Destructuring

Parameter tree destructuring already works in `lambda`, `vau`, `define!`, and
`let`. Extensions could include:

- [ ] `match` or `cond-match` operative for general-purpose pattern matching
- [ ] Predicate-based dispatch (match on type predicates)

### Tail Forms

Some Kernel combiners are specified as tail-context forms but are not yet
implemented:

- [ ] `let*` — sequential binding with TCO
- [ ] `letrec` — recursive binding with TCO
- [ ] `letrec*` — sequential recursive binding with TCO
- [ ] `when`, `unless` — one-armed conditionals returning `#inert`
- [ ] `case` — dispatch on `equal?`

## Documentation

### Tutorials and Guides

- [ ] Getting started guide — install, REPL basics, first program
- [ ] Vau calculus tutorial — operatives vs applicatives, when to use each
- [ ] Embedding guide — using Grift as a library in a Rust project
- [ ] Porting guide — running Grift on a new `no_std` target

### Reference Improvements

- [ ] Complete builtin reference with examples for every primitive
- [ ] Error catalog — every `ArenaError` variant with causes and fixes
- [ ] Arena sizing guide — how to choose the const generic capacity
- [ ] GC tuning guide — occupancy thresholds, `gc-collect` usage, disabling GC

### Specification Compliance

- [ ] Kernel spec coverage matrix — which sections of R⁻¹RK are implemented
- [ ] Document intentional deviations from the spec (immutable pairs, no `call/cc`)
- [ ] Semantic test suite mapping spec examples to Grift tests

## Tooling

### REPL Enhancements

- [ ] Multi-line input support (detect incomplete expressions)
- [ ] Tab completion for symbols bound in the current environment
- [ ] `:env` command — inspect current environment bindings
- [ ] `:type` command — show the type of a value
- [ ] `:gc` command — show arena and GC statistics
- [ ] `:load` command — evaluate a file

### Developer Tooling

- [ ] `grift fmt` — S-expression formatter / pretty-printer
- [ ] `grift check` — static analysis pass (unused bindings, arity mismatches)
- [ ] LSP server for editor integration (completions, hover, diagnostics)
- [ ] Tree-sitter grammar for syntax highlighting

### Build and CI

- [ ] Continuous benchmarking (track fib_bench and other workloads across commits)
- [ ] Code coverage reporting for the test suite
- [ ] Miri runs for arena internals validation
- [ ] `no_std` smoke test on an actual embedded target (e.g. thumbv7em-none-eabihf)
- [ ] Fuzz testing for the parser and evaluator

## Analysis and Optimization

### Performance

- [ ] Profile and optimize symbol interning (currently linear scan)
- [ ] Investigate hash-based symbol table within arena constraints
- [ ] Benchmark GC pause times and optimize mark-phase batching
- [ ] Reduce `Value` enum size if possible (currently two words + tag)
- [ ] Explore compile-time partial evaluation for constant expressions

### Memory

- [ ] Arena fragmentation analysis under long-running workloads
- [ ] Investigate generational or incremental GC within the fixed arena
- [ ] String storage compaction — measure overhead of linked-list representation
- [ ] Memory usage profiling tool (per-type slot counts, peak usage)

### Correctness

- [ ] Property-based testing (randomized S-expressions, GC stress tests)
- [ ] Mutation testing — expand the existing mutation test coverage
- [ ] Cycle detection audit — verify all recursive operations handle cycles
- [ ] Formal verification of arena allocator invariants

## Ecosystem

### Library Distribution

- [ ] Publish `grift_arena` to crates.io as a standalone no_std arena
- [ ] Publish `grift_unicode` to crates.io as a standalone no_std unicode utility
- [ ] Publish `grift` to crates.io with feature flags for REPL and stdlib

### Interoperability

- [ ] Foreign function interface — register Rust closures as Grift builtins at runtime
- [ ] Serialization — dump and restore arena state
- [ ] WASM target — compile Grift to WebAssembly for browser/edge use
- [ ] C API — thin FFI wrapper for embedding Grift in C/C++ projects

### Community

- [ ] Example programs — more than fib_bench (sorting, tree traversal, interpreters)
- [ ] Contribution guide — beyond INTERNALS.md, cover PR process and style
- [ ] Changelog — track changes per release
