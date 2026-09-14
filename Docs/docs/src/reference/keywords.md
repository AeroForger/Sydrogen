# Keywords Reference

This page summarizes the keywords and reserved terms that appear in the current Sydrogen documentation.

| Keyword | Meaning |
| ------- | ------- |
| `Nunction` | Function that does not return a value |
| `function` | Function with a dynamic return type |
| `Return` | Ends a returning function and sends a value to its caller |
| `Open` | Visibility modifier |
| `Closed` | Visibility modifier |
| `Showcase` | Visibility modifier currently parsed but not code-generated |
| `Number` | Internal category for numeric types |
| `Int` | Integer type |
| `Float` | Floating-point type |
| `Weld` | String type |
| `String` | Alias of the canonical `Weld` type |
| `Bool` / `Boolean` | Boolean type |
| `Ore` | Array or tuple type indicator |
| `Materials` | List type |
| `Generic` | Generic list element type |
| `If` | Conditional statement |
| `Else` | Conditional fallback |
| `While` | While loop |
| `For` | For loop |
| `Do` | Introduces a protected error-handling block |
| `Catch` | Handles the first matching typed error |
| `Finally` | Runs cleanup after a protected block |
| `Throw` | Constructs and propagates a typed error |
| `Error` | Root of the built-in error hierarchy |
| `Stop` | Structured early exit |
| `Program` | Runtime namespace |
| `Use` | Module import path |
| `Using` | Imported symbol from a module |
| `Data` | Structured data declaration |

A number of these keywords are present in the parser or AST, but not yet fully implemented in every backend. See [Error Handling](../language/error-handling.md), [Current Limitations](../limitations.md), and the language chapters for the current status.

---

[← Previous](operators.md)
[Next →](../examples/complete-example.md)
