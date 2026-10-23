<p align="center">
    <img src="assets/SydrogenLogo128.png">
    <br>
    <strong>Sydrogen</strong>
    <br>
    A statically typed, C-style systems programming language written in Rust.
    <br><br>
    <a href="https://aeroforger.github.io/Sydrogen/">Read the Sydrogen book</a>
</p>

> [!WARNING]
> This project is discontinued and no longer maintained. It is looking for a new maintainer. If you are interested, open an issue.

## About

Sydrogen was my first real attempt at building a compiler. It is a statically typed language with C-style syntax, aimed at native programs and direct control over what the machine does.

The last release is **Alpha-7**. The compiler is called **Furnace**, and source files use the `.anvil` extension.

## Why the name changed

Sydrogen used to be called ForgeLang. Many hobby languages use that name, and I did not want this project to be mixed up with them, so I renamed it to something more specific. The code did not change.

## How it works

Furnace parses the source, runs semantic checks, and then picks one of two backends.

* **Native backend.** Writes x86-64 machine code directly and produces an ELF64 (Linux) or PE32+ (Windows) executable. It does not call an external linker.
* **Cranelift backend.** Used for programs that need typed features the native backend does not support yet. It writes an object file and links it with the system C compiler (`cc`).

Both backends share the parser, the AST, and the semantic analysis. The split happens after checking.

The stack is:

* Compiler: Rust
* Parser: pest
* Code generation: Cranelift
* Linking: system C compiler (`cc`)
* Semantic analysis: Rayon for parallel checks

## Example

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

## Language features in Alpha-7

* **Types:** `Number`, `Int`, `Float`, `Weld`, `String`, `Bool` and `Boolean` (same type)
* **Control flow:** `If`, `Else If`, `Else`, `While`, `For`, `Switch`
* **Functions:** `Nunction` or `function`, parameters, return values
* **Strings:** plain strings and `\V` interpolation
* **Collections:** fixed-size arrays (`Ore`), named tuples (`Ore`), and lists (`Materials`)
* **Input:** `Input(Int)`, `Input(Float)`, `Input(Weld)`
* **Operators:** `+`, `-`, `*`, `/`, `%`, `**`, comparisons, and bitwise `And`, `Or`, `Xor`
* **Errors:** `Do`, `Catch`, `Finally`, `Throw` (native backend only)
* **Exit:** `Stop` for loops and `Program.Stop()` for the program

### Booleans

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

Assigning an integer, float, or string to a `Bool` is a compile error.

## Building

You need:

1. Rust, installed with `rustup`
2. A C linker such as `gcc`, `clang`, or `cc`
3. The math library, which normally comes with the C toolchain

Clone the repository and build:

```fish
git clone https://github.com/AeroForger/Sydrogen.git
cd Sydrogen
cargo build --release
```

The binary is at `target/release/furnace`.

To build Alpha-7 specifically, check out its tag or commit before building. The `install.sh` script installs the newest tag, which may not be Alpha-7.

## Usage

```fish
# Compile for Linux (default)
./target/release/furnace compile main.anvil linux

# Compile a Windows executable
./target/release/furnace compile main.anvil --windows

# Compile and run
./target/release/furnace run main.anvil

# Choose a backend for one command
./target/release/furnace compile main.anvil linux --backend native
./target/release/furnace run main.anvil --backend cranelift

# Save a default backend
./target/release/furnace backend native
./target/release/furnace backend cranelift

# Create a console project
./target/release/furnace new console -n Project
```

Cranelift is the default when no backend has been saved.

## Known limitations

These are the limits at Alpha-7.

* The native backend supports integers, Booleans, `Weld` values, integer input, string printing, arithmetic, comparisons, branches, loops, and function calls. Floats, arrays, tuples, lists, `Data`, and objects need the Cranelift backend.
* The Windows backend does not yet implement `Print`, `Input`, `Program.Stop()`, or uncaught-error messages.
* `Do`, `Catch`, `Finally`, and `Throw` run only on the native backend.
* `Finally` does not run when `Stop` or `Skip` leaves a protected block.
* Division by zero raises an error automatically. Automatic `Error.Overflow` and `Error.IO` conversion was not finished.
* `Deal` is reserved but has no pattern-matching behavior.
* Full lexical scope and shadowing, garbage collection, a package manager, multicore execution, and self-hosting were not implemented.

For the full language reference, see the [Sydrogen book](https://aeroforger.github.io/Sydrogen/).

## License

GPL-3.0-only
