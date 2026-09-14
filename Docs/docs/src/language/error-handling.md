# Error Handling

Sydrogen provides typed, structured errors through `Do`, `Catch`, `Finally`,
and `Throw`. Error propagation is part of the language runtime; it does not use
Rust panics, C++ exceptions, or Windows SEH.

## Throwing an error

`Throw` constructs an error with a built-in type and a `Weld` message, stops
the current block, and begins propagation:

```sydrogen
Throw Error.InvalidValue("Value cannot be negative");
```

The initial built-in hierarchy is:

- `Error`, the general base type
- `Error.InvalidValue`
- `Error.DivisionByZero`
- `Error.Overflow`
- `Error.IO`

Integer division or remainder by zero automatically throws
`Error.DivisionByZero` in the direct native backend.

## Do and Catch

`Do` protects a block. It must be followed by at least one `Catch` or a
`Finally` block:

```sydrogen
Do {
    Throw Error.InvalidValue("bad input");
}
Catch Error.InvalidValue => Err {
    Print(Err);
}
```

The name after `=>` is user-defined and exists only in that catch block.
Printing it writes the error's human-readable message.

Catches are tested from top to bottom and only the first match runs. A general
`Catch Error` matches every Sydrogen error and must appear last:

```sydrogen
Do {
    DangerousOperation();
}
Catch Error.DivisionByZero => DivisionError {
    Print(DivisionError);
}
Catch Error.IO => IOError {
    Print(IOError);
}
Catch Error => OtherError {
    Print(OtherError);
}
```

Furnace rejects duplicate catches and catches placed after `Catch Error`.

## Finally and propagation

`Finally` is optional and always runs after its protected sequence: after
success, after a caught error, and while an unmatched error propagates. A
return from the protected body also runs `Finally` before returning.

```sydrogen
Do {
    Int Result = Divide(10, 0);
    Print(Result);
}
Catch Error.DivisionByZero => ERROR {
    Print(ERROR);
}
Catch Error => ERROR {
    Print(ERROR);
}
Finally {
    Print("Operation finished");
}
```

Nested handlers propagate to the next enclosing matching catch. An inner
handler that only catches `Error.IO`, for example, does not consume an
`Error.InvalidValue`; its `Finally` runs and the outer search continues.

On native Linux, an error that reaches the program entry point prints its type
and message and exits with status 1:

```text
Error.InvalidValue: Value cannot be negative
```

## Backend status

The direct native backend implements error propagation for ELF64/Linux and for
PE32+/Windows code, including nested handlers and `Finally`. Linux also
implements the top-level uncaught-error diagnostic.

Current limitations:

- The Cranelift backend rejects programs using error handling with an explicit
  diagnostic; use `--backend native`.
- Windows PE32+ cannot yet print an uncaught error because Windows console I/O
  is not implemented. The image returns to the loader cleanly, but the Linux
  type-and-message diagnostic is unavailable.
- `Stop` and `Skip` leaving a protected block do not yet run `Finally`; return
  and error propagation do.
- Built-in overflow and I/O errors currently require an explicit `Throw`.

[← Previous](scope.md)
[Next →](comments.md)
