<p align="center">
    <img src="assets/SydrogenLogo128.png">
    <br>
    <strong>Sydrogen, a statically typed systems programming language built in Rust.</strong>
    <br>
    Source files use the <code>.anvil</code> extension. The compiler is <code>Furnace</code>.
    <br><br>
    <a href="https://aeroforger.github.io/Sydrogen/"> Read the Sydrogen Book</a>
    &nbsp;&nbsp;•&nbsp;&nbsp;
    <a href="https://aeroforger.github.io/3d-fl/"> View Interactive 3D Logo</a>
</p>


---

## What is Sydrogen?

Sydrogen is a statically typed, C-style systems programming language designed for native execution and explicit control.

**Alpha-7** uses a compiler written in **Rust**. Furnace uses **pest** for parsing, a direct x86-64 code path for supported programs, and **Cranelift** for programs that need the typed path.

### The Stack

* **Compiler:** Rust
* **Parser:** pest
* **Code generation:** Cranelift
* **Linking:** System C compiler (`cc`)
* **Parallel semantic analysis:** Rayon

---

## Alpha-7 Features

## Native Code Generation

Furnace has a direct native code path for a supported subset of Sydrogen. It writes x86-64 instructions and creates an ELF64 Linux or PE32+ Windows executable without first creating an object file or calling an external linker.

The compiler chooses the path after parsing and semantic analysis:

```mermaid
flowchart TD
    A(Sydrogen source) --> B(Parser)
    B --> C(AST)
    C --> D(Semantic analysis)
    D --> E(Direct native path)
    E --> F(Internal IR)
    F --> G(x86-64 bytes)
    G --> H(ELF64 or PE32+ executable)
    D --> I(Cranelift path)
    I --> J(Object file)
    J --> K(cc)
    K --> L(Executable)
```

The direct path handles integer, Boolean, and `Weld` values, integer input, strings used by printing, arithmetic, comparisons, branches, loops, function calls, and `Program.Stop()`.

Programs that use floats, arrays, tuples, lists, or other typed features use the Cranelift path. That path writes an object file and uses `cc` to make the final executable.

### Direct Native Path

`src/lowering.rs` converts the checked AST into the internal representation in `src/ir.rs`. The representation contains virtual registers, stack variables, basic blocks, arithmetic and comparison instructions, calls, and branch terminators.

`src/backend/x86_64/mod.rs` assigns virtual registers to x86-64 registers or stack slots. Function arguments use `RDI`, `RSI`, `RDX`, `RCX`, `R8`, and `R9`. Function results use `RAX`.

`src/backend/x86_64/encoder.rs` writes instruction bytes for moves, integer operations, comparisons, calls, returns, jumps, and the function stack frame. It records calls and jumps whose destinations are not known yet. After all functions are written, Furnace fills in those relative addresses and embeds string data.

`src/backend/elf.rs` writes the Linux ELF64 layout, while `src/backend/pe.rs` writes the Windows PE32+ layout. Each format supplies its own aligned headers and startup entry point around the shared generated x86-64 functions.

### Typed Code Path

Programs that need floats or collection types use `src/codegen.rs`. Cranelift produces the object file, Furnace adds its C runtime helpers, and `cc` links the object and runtime into an executable.

Both paths share parsing, the AST, and semantic analysis. The difference begins after the program has been checked.

### Direct Path Limits

The direct path currently supports x86-64 output, at most six integer function arguments, integer input, and integer, Boolean, and `Weld` function values. It does not directly generate code for floats, arrays, tuples, lists, `Data`, or objects. Imports resolve before backend selection and work with both paths.

### The Language

```forge
Nunction Tick()
{
    Print("tick");
}

Open Nunction Main()
{
    Int I = 0;
    Int A = 0;
    Int B = 0;

    While (I < 100000)
    {
        A = 0;
        B = 0;

        While (A < 1000)
        {
            If (A < 500) {
                B = B + 3;
            } Else {
                B = B + 7;
            }

            A = A + 1;
        }

        Tick();
        I = I + 1;
    }

    Print(\V"{B}");
}
```

Alpha-7 currently supports:

* **Types:** `Number`, `Int`, `Float`, `Weld`, `String`, `Bool` / `Boolean`
* **Control flow:** `If`, `Else If`, `Else`, `While`, `For`
* **Functions:** `Nunction`, `function`, parameters, return values, and native function calls
* **Strings:** Plain strings and `\V` interpolation
* **Arrays:** Fixed-size `Ore` arrays
* **Tuples:** Named-field `Ore` tuples
* **Lists:** `Materials`
* **Input:** `Input(Int)`, `Input(Float)`, `Input(Weld)`
* **Arithmetic:** `+`, `-`, `*`, `/`, `%`, `**`
* **Unary operators:** `+`, `-`
* **Comparisons:** `<`, `>`, `<=`, `>=`, `==`, `!=`
* **Bitwise operators:** `And`, `Or`, `Xor`
* **Loop control:** `Stop`
* **Runtime exit:** `Program.Stop()`

### Boolean Type

Sydrogen provides a Boolean type with two values: `true` and `false`.

The keywords `Bool` and `Boolean` refer to the same type and can be used interchangeably.

```forge
Open Nunction Main()
{
    Bool IsOpen = true;
    Print(IsOpen);

    Boolean IsClosed = false;
    Print(IsClosed);

    IsOpen = false;
    Print(IsOpen);
}
```

A `Bool` variable can only be assigned `true`, `false`, or another compatible Boolean value. The compiler rejects assignments of integers, floats, or strings to `Bool` variables.

Printing a Boolean value outputs `true` or `false`.

### Parallel Semantic Analysis

Furnace contains a semantic analysis stage using **Rayon**.

The semantic stage checks the parsed program before code generation. Independent parts of the AST can be analyzed concurrently.

It currently checks things including:

* Function argument counts
* `Main` parameters
* Undefined variables
* Shared-member access
* Shared-member mutation
* Invalid collection operations
* Array size mismatches
* Tuple field counts
* List element types
* Boolean type compatibility
* Invalid `Stop` usage
* Invalid `Program.Stop()` usage

---

## Getting Started

### Prerequisites

1. **Rust** installed through `rustup`
2. **A C linker**, such as `gcc`, `clang`, or `cc`
3. **The math library**, normally provided by the system C toolchain

### Building Furnace

Clone the repository and build the compiler:

```fish
git clone https://github.com/AeroForger/Sydrogen.git
cd Sydrogen
cargo build --release
```

The compiler binary will be located at:

```text
target/release/furnace
```

### Compiling a Program

The current CLI uses:

```fish
./target/release/furnace compile main.anvil linux
./target/release/furnace compile main.anvil --windows
```

Furnace parses the source and performs semantic analysis, then selects the direct native path or the Cranelift path. Linux/ELF64 is the default. `--windows` selects direct native Windows x86-64 output and writes a PE32+ `.exe`, including when Furnace runs on Linux. The Cranelift path generates an object file and links it using the system C toolchain.

### Running a Program

```fish
./target/release/furnace run main.anvil
```

### Persistent Backend Selection

Save the default backend with:

```fish
./target/release/furnace backend native
./target/release/furnace backend cranelift
```

Available backends:

* `native` writes a direct x86-64 ELF64 or PE32+ executable.
* `cranelift` writes a native object file and links it with `cc`.

The selection persists across Furnace invocations and is used by `build`,
`compile`, and `run`. Cranelift is the built-in default when no preference has
been saved.

Use `--backend` with `build`, `compile`, or `run` to override the saved choice
for one command:

```fish
./target/release/furnace build project.blower --backend native
./target/release/furnace compile main.anvil linux --backend native
./target/release/furnace run main.anvil --backend cranelift
./target/release/furnace compile main.anvil --windows
```

### Creating a Project

Create a console project with:

```fish
./target/release/furnace new console -n Project
```

This creates:

```text
Project/
└── Project.anvil
```

The generated source contains a minimal `Open Nunction Main()` program.

---

## Architecture

The compiler is divided into several stages:

1. **Parser (`pest`)**
   Reads `.anvil` source code and produces an AST.

2. **AST (`ast.rs`)**
   Stores the program in strongly typed Rust structures.

3. **Semantic Analysis (`semantic.rs`)**
   Checks the AST before code generation.

4. **Code Generation**
   Selects the direct x86-64 path for supported programs or the Cranelift path for programs that need typed features.

5. **Output**
   The direct path writes an ELF64 or PE32+ executable. The Cranelift path writes an object file and uses the system C compiler to produce an executable.

The overall pipeline runs in this order:

```mermaid
flowchart LR
    A(Sydrogen source) --> B(pest parser)
    B --> C(AST)
    C --> D(Module resolution)
    D --> E(Semantic analysis)
    E --> F(Direct x86-64 path or Cranelift path)
    F --> G(Executable)
```

---

## Current Limitations

Alpha-7 is still under development.

The direct native backend currently supports x86-64 integer, Boolean, and
`Weld` values. Linux programs that require floats, collections, `Data`, or
objects use the Cranelift and system-linker path instead. Those typed features
are not yet available for PE32+ output.

The Windows backend produces PE32+ executables and supports variables,
arithmetic, comparisons, control flow, `Switch`, function calls, and caught
typed errors. Its runtime does not yet implement `Print`, `Input`,
`Program.Stop()`, or top-level uncaught-error messages.

Other current limitations are:

* Typed `Do` / `Catch` / `Finally` / `Throw` execution is implemented by the
  direct native backend but not yet by Cranelift.
* `Finally` runs for normal completion, `Return`, and propagated errors, but
  not yet when `Stop` or `Skip` leaves a protected block.
* Division by zero is raised automatically. Automatic `Error.Overflow` and
  `Error.IO` conversion is still planned; both can currently be thrown
  explicitly.
* `Switch` supports exact `Int`, `Float`, and `Bool` cases without fallthrough.
  The reserved `Deal` keyword does not yet have defined pattern-matching
  semantics.
* Full lexical scope and shadowing, garbage collection, package management,
  multicore program execution, and self-hosting are not implemented.

For the complete language reference and current implementation details, see **[The Sydrogen Programming Language Book](https://aeroforger.github.io/Sydrogen/)**.

---

## Roadmap

### Short Term

* Add Cranelift execution for `Do`, `Catch`, `Finally`, and `Throw`.
* Complete `Finally` unwinding for `Stop` and `Skip`, and add automatic
  overflow and I/O error conversion.
* Add a Windows runtime for console output, input, process termination, and
  uncaught-error reporting.
* Improve lexical scope and shadowing diagnostics.
* Expand the systems-level standard library.

### Mid Term

* Extend direct native code generation to floats, collections, `Data`, and
  objects, including PE32+ support for those types.
* Define whether `Deal` extends `Switch` with richer pattern matching.
* Expand generic data types and generic function support.
* Add language-level multicore program execution.

### Long Term

* **Scrap:** Optional garbage collector.
* **Ironwork:** Sydrogen package manager.
* **Self-hosting:** Rewrite Furnace in Sydrogen for the 2.0 generation.

---

## Documentation

The complete Sydrogen language reference is available in **The Sydrogen Programming Language Book**.

**[Read the book](https://aeroforger.github.io/Sydrogen/)**

---

## License

GPL-3.0-only
