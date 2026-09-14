# Operators

Alpha-7 supports arithmetic, comparison, bitwise, unary, and loop increment operators.

## Arithmetic

| Operator | Operation      |
| -------- | -------------- |
| `+`      | Addition       |
| `-`      | Subtraction    |
| `*`      | Multiplication |
| `/`      | Division       |
| `%`      | Modulo         |
| `**`     | Power          |

Example:

```forge
Int A = 10;
Int B = 5;

Int C = A + B;
Int D = A - B;
Int E = A * B;
Int F = A / B;
Int G = A % B;
```

`%` requires integer operands and returns the integer remainder.

## Power

The `**` operator performs exponentiation.

```forge
Int Result = 2 ** 8;
```

Power expressions are right-associative.

For example:

```text
A ** B ** C
```

is interpreted as:

```text
A ** (B ** C)
```

The current implementation lowers power operations through the C `pow` function.

## Increment and Decrement

`++` and `--` are postfix operators currently used in the increment section of a `For` loop.

Example:

```forge
For (Int I = 0; I < 10; I++)
{
    Print(\V"{I}");
}
```

Decrementing is also supported:

```forge
For (Int I = 10; I > 0; I--)
{
    Print(\V"{I}");
}
```

See [Unary Operators](unary.md), [Comparisons](comparisons.md), and [Logical Operators](logical.md) for the related operator families.

---

[← Previous](input.md)
[Next →](unary.md)
