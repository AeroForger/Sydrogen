# Compilation

This page covers the practical steps of building Furnace and using it to compile a Sydrogen program.

## Build Furnace

Furnace is built using Cargo:

```fish
cargo build --release
```

The release compiler is located at:

```text
target/release/furnace
```

You can also install Furnace using Cargo:

```fish
cd Sydrogen
cargo install --path .
```

After installation, Cargo places `furnace` in its executable path, allowing you to invoke it directly without specifying `target/release/furnace`.

For example:

```fish
furnace build Project.blower
furnace compile main.anvil linux
furnace compile main.anvil --windows
furnace run main.anvil
furnace backend native
furnace backend cranelift
furnace new console -n Project
furnace update
furnace -help
furnace --help
furnace -version
furnace --version
```

This means you can use `furnace` directly from any directory, provided Cargo's binary directory is available in your `PATH`.

Documentation will use `cargo build --release` instead of `cargo install --path .` 

## Compile a Sydrogen Program

For a project, use its `.blower` configuration:

```fish
./target/debug/furnace build Project.blower
```

Project sources come from `Files.location` patterns relative to the project
file. The executable is named by `Project.Name` and written to `build/`.

For one standalone source, use:

Linux/ELF64 is the default, so either of these commands produces `main`:

```fish
./target/debug/furnace compile main.anvil linux
./target/debug/furnace compile main.anvil
```

The compile process:

1. Checks that the input file uses the `.anvil` extension.
2. Reads the source file.
3. Parses the source.
4. Builds the AST.
5. Performs semantic analysis.
6. Selects the direct native path or the Cranelift path.
7. Generates functions and calls.
8. Writes an ELF64 or PE32+ executable directly, or creates an object file and invokes the Linux platform linker.
9. Produces the executable.

Example output:

```text
Compiling main.anvil...
Linking...
Build successful!
Output: ./main
```

The CLI uses the `Platform` enum in `Cli/platform.rs`.

The supported direct-native targets are:

```text
linux (default): x86-64 ELF64
windows: x86-64 PE32+
```

To cross-compile from Linux to Windows without an external linker or Windows
SDK, run:

```fish
furnace compile main.anvil --windows
```

The result is `main.exe`. `furnace compile main.anvil windows` is equivalent,
and `furnace build Project.blower --windows` writes
`build/<Project.Name>.exe`.

Windows output currently excludes `Print`, `Input`, `Program.Stop()`, floats,
collections, `Data`, and objects. Furnace reports these as unsupported instead
of falling back to an ELF executable. Windows also requires `--backend native`;
the flag selects it automatically when no backend is explicitly supplied.

## Run a Sydrogen Program

The CLI can compile and execute a program directly:

```fish
./target/debug/furnace run main.anvil
```

This command:

1. Compiles the source.
2. Writes the direct executable or links the generated object.
3. Executes the resulting binary.
4. Forwards the program's standard output and standard error.
5. Returns the child process exit code.

## Select the Default Backend

Use the `backend` command to persist the default backend:

```fish
./target/debug/furnace backend native
./target/debug/furnace backend cranelift
```

Available backends:

- `native` writes a direct x86-64 ELF64 or PE32+ executable.
- `cranelift` writes a native object file and links it with `cc`.

The selection is reused by `build`, `compile`, and `run` in later Furnace
processes. If no preference has been saved, Cranelift remains the built-in
default. Furnace stores the preference in the platform configuration directory:
`$XDG_CONFIG_HOME/furnace/config` or `~/.config/furnace/config` on Unix, and
`%APPDATA%\furnace\config` on Windows.

Use `--backend` with `build`, `compile`, or `run` to override the saved backend
for only that command:

```fish
./target/debug/furnace build Project.blower --backend native
./target/debug/furnace compile main.anvil linux --backend native
./target/debug/furnace run main.anvil --backend cranelift
./target/debug/furnace compile main.anvil --windows
```

## Version and Help

Furnace exposes its version through centralized compiler metadata.

The current version is:

```rust
pub const VERSION: &str = "Alpha-7";
```

Version information can be requested with:

```fish
./target/debug/furnace -version
```

Help can be requested with:

```fish
./target/debug/furnace -help
```

Example version output:

```text
Furnace Alpha-7
```

Usage:

```text
Usage:
    Furnace compile <file>.anvil [linux|windows] [--windows] [--backend native|cranelift]
    Furnace run <file>.anvil [--windows] [--backend native|cranelift]
    Furnace backend <native|cranelift>
    Furnace new <APP_TYPE> -n <NAME>
    Furnace -version
    Furnace -help

Available backends:
    native: direct x86-64 ELF64 or PE32+ executable
    cranelift: native object file linked with cc
```

## Create a Project

Create a console project with:

```fish
./target/debug/furnace new console -n Project
```

The command creates `Project/Project.blower` and `Project/src/Main.anvil`. The
project file declares `Name = "Project"` and `location = "src/*.anvil"`, so it
can immediately be built with `furnace build Project/Project.blower`.

The generated source contains:

```forge
Open Nunction Main()
{
}
```

The supported application type is `console`. Furnace rejects unknown types, empty names, and existing project directories.

## Link the Object Manually

Furnace produces a native object file that can be linked separately:

```fish
cc main.o -o main -lm
```

The exact libraries required may depend on the generated program and target platform.

## Run the Resulting Executable

The resulting executable can be started normally:

```fish
./main
```

---

[← Previous](program-structure.md)
[Next →](../language/functions.md)
