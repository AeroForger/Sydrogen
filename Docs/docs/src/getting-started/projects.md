# `.blower` Projects

A `.blower` file is the Sydrogen project configuration format. It names the
project output and declares exactly which `.anvil` files belong to the project.

```blower
Project
{
    Name = "PlaceHolder";
}

Files
{
    location = "src/*.anvil";
    location = "tests/*.anvil";
}
```

## Project and Name

The `Project` section contains project metadata. `Project.Name` is required and
becomes the executable filename. The example above produces
`build/PlaceHolder` on platforms that do not add an executable suffix.

Names may contain letters, numbers, underscores, and hyphens.

## Files and location

The `Files` section contains one or more `location` entries. Only `.anvil`
sources matched by these entries belong to the project. Entries are evaluated
relative to the directory containing the `.blower` file, regardless of the
shell's current directory.

`*` matches within one path component, `?` matches one character, and `**`
matches directories recursively. For example:

```blower
Files
{
    location = "src/*.anvil";
    location = "src/**/*.anvil";
}
```

Files matched by more than one entry are compiled once. Furnace orders source
paths deterministically.

## Building a project

Build a project through its `.blower` file:

```fish
furnace build project.blower
furnace build project.blower --backend native
furnace build project.blower --backend cranelift
furnace build project.blower --windows
```

`furnace backend native` or `furnace backend cranelift` saves the default used
when `--backend` is omitted. The saved choice persists across Furnace
invocations.

Furnace parses all matched sources as one project, performs project-wide
semantic analysis, and writes the executable to `build/` beside the `.blower`
file. Both backends use the same output path derived from `Project.Name`.

For a standalone source file, continue to use:

```fish
furnace compile file.anvil linux
furnace compile file.anvil --windows
```

The `compile` command retains its existing single-file output behavior; it does
not redirect standalone files through project builds.

## Creating a project

```fish
furnace new console -n MyProject
```

This creates an immediately buildable layout:

```text
MyProject/
├── MyProject.blower
└── src/
    └── Main.anvil
```

From `MyProject/`, run `furnace build MyProject.blower`.

To compile and immediately run all sources in a project without writing its
normal `build/` output, use `furnace run MyProject.blower`.
