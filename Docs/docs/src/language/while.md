# While Loops

Sydrogen supports `While` loops.

Basic syntax:

```forge
While (Condition)
{
    // body
}
```

Example:

```forge
Int I = 0;

While (I < 10)
{
    I = I + 1;
}
```

The condition is evaluated before every iteration.

## Nested Loops

`While` loops can be nested:

```forge
Int I = 0;
Int V = 0;

While (I < 100)
{
    V = 0;

    While (V < 100)
    {
        V = V + 1;
    }

    I = I + 1;
}
```

Nested loops are compiled to native control flow.

Normal Sydrogen `While` loops execute on one thread.

The compiler using multiple CPU cores for semantic analysis does not make the generated loop multicore. The compiler cannot simply yell "parallel!" at a loop and hope for the best.

---

[← Previous](switch.md)
[Next →](for.md)
