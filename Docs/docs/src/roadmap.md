# Alpha-6 Roadmap

Alpha-6 continues development of the Rust-based Furnace compiler.

## Short Term

### `Switch` / Pattern Matching

Planned constructs include:

```text
Switch
Deal
Base
```

These are intended to provide pattern matching.

### Error Handling Follow-up

`Do`, `Catch`, `Finally`, and `Throw` are implemented by the direct native
backend. Follow-up work includes Cranelift support, Windows uncaught-error
printing, automatic overflow and I/O conversion, and `Finally` handling for
`Stop` and `Skip`.

### Generic Data Types

`Generic` is currently limited.

Future versions are planned to support generic data types and generic function parameters.

## Mid Term

### Module Packaging

Sydrogen now has local and `.blower` project imports based around:

```text
Use
Using
```

The implemented import system applies `Open` and `Closed` visibility. Future
module work can extend standard-library and package integration and define the
role of:

```text
Showcase
```

visibility rules.

### Additional Function Features

Parameterized functions and return-value functions are supported by the current native function call path.

Future work includes clearer diagnostics for control-flow paths that do not return and broader support for generic function types.

### Multicore Runtime

Alpha-6 uses Rayon for compiler-side parallel analysis.

Future versions are planned to provide mechanisms for Sydrogen programs to execute work on multiple CPU cores.

Possible constructs include:

```text
Spawn
Join
```

The exact syntax and safety rules are not final.

The compiler's parallel analysis and a program's parallel execution are separate features.

## Long-Term Plans

### Scrap

**Scrap** is planned as an optional garbage collector for Sydrogen.

It is intended to be disabled by default.

The default memory model is intended to keep memory management explicit.

Scrap would provide another memory-management option without making garbage collection mandatory.

### Ironwork

**Ironwork** is planned as the Sydrogen package manager.

Its planned responsibilities include:

- Package management
- Dependency resolution
- Library distribution
- Project management
- Sydrogen package integration

### Self-Hosting

A long-term goal is to rewrite Furnace in Sydrogen itself.

This is targeted for the **2.0 generation** of Sydrogen.

The compiler will need sufficient language features, standard library support, and tooling before this becomes practical.

---

[← Previous](limitations.md)
[Next →](implementation-notes.md)
