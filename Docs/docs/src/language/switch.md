# Switch Statements

`Switch` selects one branch by comparing a value with the cases in source
order:

```forge
Switch (X) {
    Case 1 {
        Print("One");
    }

    Case 2 or 3 {
        Print("Two or three");
    }

    Base {
        Print("Other");
    }
}
```

The expression in `Switch (...)` is evaluated exactly once. The compiler then
compares that retained result with each Case value. Only the first matching
branch runs, and execution continues after the whole Switch when its body
finishes. Cases never fall through, so no `Break` or other terminator is
required.

Switch values and Case values currently support `Int`, `Float`, and `Bool`.
Every value in a Case must have exactly the selector's type; Furnace does not
insert numeric conversions to make a Case match. Float Switch statements use
the Linux typed backend and are not currently available for Windows output.

## Case forms

A Case can use a block:

```forge
Case 1 {
    Print("One");
}
```

For one statement, `->` provides a compact form:

```forge
Case 1 -> Print("One");
```

The alternate arrow spelling places the arrow before the values and a colon
before the action:

```forge
Case -> 1: Print("One");
```

Both arrow forms have exactly the same behavior as a one-statement block.

Lowercase `or` gives one Case multiple values:

```forge
Case 2 or 3 {
    Print("Two or three");
}

Case 4 or 5 -> Print("Four or five");
```

Repeated constant values are rejected, including repetitions hidden in an
`or` list or a later Case.

## Base fallback

`Base` runs when no Case matches. It supports block and arrow bodies:

```forge
Base {
    Print("Other");
}

Base -> Print("Other");
```

A Switch may omit Base; if nothing matches, it simply does nothing. At most
one Base is allowed, and it must be the final branch.

## Switch and errors

Switch uses ordinary compiler control flow, so typed errors retain their usual
behavior:

```forge
Do {
    Switch (Value) {
        Case 0 -> Throw Error.InvalidValue("Zero is invalid");
        Base -> Print("Valid");
    }
}
Catch Error.InvalidValue => ERROR {
    Print(ERROR);
}
Finally {
    Print("Finished");
}
```

`->` is only the compact Switch action operator. `=>` remains the distinct
Catch binding operator.

Switch control flow is supported by the direct native ELF64 and PE32+ paths
and by Cranelift. The usual backend restrictions still apply to statements
inside a branch—for example, Windows output cannot currently use `Print`.

---

[← Previous](conditionals.md)
[Next →](while.md)
