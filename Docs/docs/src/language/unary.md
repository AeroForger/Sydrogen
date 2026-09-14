# Unary Operators

Alpha-7 supports unary negation and unary plus.

Example:

```forge
Int Value = -10;
```

Unary negation can also be applied to an expression:

```forge
Int Result = -(A + B);
```

Unary plus is a no-op:

```forge
Int Value = +42;
```

It exists because sometimes a language designer looks at unary minus and thinks, "why should minus get all the attention?"

## Precedence

Unary operators bind looser than `**`. Therefore:

```forge
-2 ** 2
```

is interpreted as:

```text
-(2 ** 2)
```

which produces:

```text
-4
```

This is part of the documented language rule that unary operators bind looser than the power operator.

---

[← Previous](operators.md)
[Next →](comparisons.md)
