# Linking

The Cranelift path generates a native object file. The direct native path creates an ELF64 or PE32+ executable and does not use this step. PE32+ cross-compilation therefore does not require a Windows linker or SDK.

The object file is linked using the system C compiler.

For example:

```fish
cc main.o -o main -lm
```

The linker produces the final executable.

This is the last stage of the Cranelift path. Once the object file is generated, it becomes a native binary after the system C compiler links it.

The currently supported target is `linux`.

Additional targets can be added as compiler support is implemented.

---

[← Previous](code-generation.md)
[Next →](../reference/types.md)
