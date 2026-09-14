# CLI Reference

This page documents the commands exposed by the `furnace` binary.

## Update

The `update` command installs the latest tagged release of Furnace from its source repository.

```fish
furnace update
```

The command:

1. Checks that `git` and `cargo` are available.
2. Clones the Furnace repository into a temporary directory.
3. Fetches tags and selects the latest tag sorted by version.
4. Checks out that tag.
5. Runs `cargo install --path . --locked` in the checked-out tree.
6. Replaces the existing `furnace` binary on the `PATH`.

The repository URL is defined as `FURNACE_REPO` in `Cli/commands/update.rs`.

Example output:

```text
Downloading source...
Latest tag: alpha-6
Installing Furnace...
Furnace updated successfully.
```

Failure modes:

- `git` or `cargo` is missing from the `PATH`.
- The repository cannot be cloned.
- No tags are found in the repository.
- `cargo install` fails to build or link.

On failure the command exits with a non-zero code and prints an error message.

## Other Commands

```text
Usage:
    furnace build <project>.blower [--windows] [--backend native|cranelift]
    furnace compile <file>.anvil [linux|windows] [--windows] [--backend native|cranelift]
    furnace run <file>.anvil|<project>.blower [--windows] [--backend native|cranelift]
    furnace backend <native|cranelift>
    furnace new <APP_TYPE> -n <NAME>
    furnace update
    furnace -version
    furnace -help
```

## Build

`furnace build project.blower` loads a Sydrogen project, discovers the source
files declared by its `Files.location` entries, and writes the named executable
to the project's `build/` directory. Use `--backend native` or
`--backend cranelift` to select a backend.

`build` accepts only `.blower` targets. Compile an independent `.anvil` file
with `furnace compile file.anvil` (or the equivalent explicit Linux target,
`furnace compile file.anvil linux`).

## Windows Target

Linux x86-64 and ELF64 are the default native target and executable format.
Use either form below to cross-compile a Windows x86-64 PE32+ executable:

```fish
furnace compile main.anvil --windows
furnace compile main.anvil windows
```

Both commands select the direct native backend and write `main.exe`. The same
`--windows` target option is accepted by `build`, which adds `.exe` to the
project output name. Cross-compilation works when Furnace is running on Linux
and does not require a Windows linker or SDK.

The Windows backend currently supports integer and Boolean computation, `Weld`
value passing, variables, control flow, and Sydrogen function calls. It does not yet support
`Print`, `Input`, `Program.Stop()`, floats, collections, `Data`, or objects.
Those features produce an explicit compiler error because their native runtime
or typed fallback is currently Linux-only. Cranelift Windows linking is also
unsupported; selecting `--backend cranelift` with a Windows target is an error.

`run --windows` is recognized, but Furnace cannot execute the cross-compiled
file on a non-Windows host; use `compile` or `build` for cross-compilation.

## Persistent Backend Selection

The existing backend command saves the default used by future commands:

```fish
furnace backend native
furnace backend cranelift
```

The selected backend is automatically used by `furnace build`, `furnace
compile`, and `furnace run`, including later Furnace processes. With no saved
configuration, Cranelift remains the built-in default.

On Unix, Furnace stores the setting in `$XDG_CONFIG_HOME/furnace/config`, or
`~/.config/furnace/config` when `XDG_CONFIG_HOME` is unset. On Windows it uses
`%APPDATA%\furnace\config`. The file contains, for example:

```text
backend = "native"
```

An existing `--backend native|cranelift` option overrides the saved preference
for one `build`, `compile`, or `run` command. Resolution priority is the command
override, saved preference, then built-in default. Malformed configuration is
reported rather than silently ignored.
