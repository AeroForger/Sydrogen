# Current Limitations

Alpha-6 is an early development release.

The following features are not currently fully implemented in the backend:

- `Data` declaration code generation
- Object instantiation code generation
- `Switch` / `Deal` / `Base` pattern matching
- Error handling in the Cranelift backend
- Windows top-level uncaught-error printing
- `Finally` handling for `Stop` and `Skip`
- Package and dependency management
- Complete lexical scope handling
- Multicore Sydrogen program execution
- Garbage collection
- Self-hosting Furnace
- Complete systems-level standard library

Some of these features are already represented in the grammar or AST.

There is an important distinction:

**Parsed** means Furnace can recognize the syntax.

**Semantically checked** means Furnace can inspect the construct and report certain errors.

**Code generated** means Furnace can produce executable native code for the construct.

A feature being parsed does not mean that it can currently be used in an executable program.

This distinction saves everyone from discovering that the compiler supports something only after the compiler politely refuses to compile it.

---

[← Previous](examples/complete-example.md)
[Next →](roadmap.md)
