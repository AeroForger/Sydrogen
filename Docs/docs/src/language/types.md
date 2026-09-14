# Types

Alpha-7 currently includes primitive types, arrays, tuples, lists, and a limited generic type.

## Primitive Types

The primary primitive types are:

| Type           | Purpose               |
| -------------- | --------------------- |
| `Number`       | Numeric type category |
| `Int`   | Integer values        |
| `Float` | Floating-point values |
| `Weld`         | Canonical string type |
| `String`       | Alias of `Weld`       |
| `Bool`         | Boolean values        |
| `Boolean`      | Boolean values        |

`Bool` and `Boolean` refer to the same type and can be used interchangeably.

`Int` and `Float` are distinct concrete types in the `Number` category. Variable
declarations use the concrete type directly, such as `Int Count = 0;` or
`Float Ratio = 1.5;`.

Sydrogen uses static type checking.

## Arrays

Arrays use the `Ore` keyword followed by a size.

A fixed-size array:

```forge
Ore[3] FixedNums = [10, 20, 30,];
```

An array with an inferred size:

```forge
Ore[EMPTY] InferredNums = [100, 200, 300, 400,];
```

Array literals use square brackets.

A trailing comma is allowed.

Elements can be accessed by index:

```forge
Print(FixedNums[0]);
```

Elements can be assigned by index:

```forge
FixedNums[1] = 55;
```

Arrays expose `.Length`:

```forge
Print(FixedNums.Length);
```

The semantic analyzer checks that an explicitly declared array size matches the initializer count.

Arrays have a fixed number of elements after creation.

Your array will not spontaneously grow because it has decided that four elements are not enough. That job belongs to `Materials`.

## Tuples

Tuples use `Ore` followed by named fields.

Example:

```forge
Ore(Int Number1, Int Number2) TwoNumbers = {1, 2};
```

A tuple containing different field types:

```forge
Ore(Int Age, Weld Name) Person = {14, "Den"};
```

Fields are accessed by name:

```forge
Print(Person.Age);
Print(Person.Name);
```

Fields can be assigned:

```forge
Person.Age = 15;
```

The semantic analyzer checks the number and types of tuple initializer values.

## Lists

Lists use the `Materials` keyword.

A list initialized with values:

```forge
Materials Int Numbers = (10, 20, 30,);
```

An empty list:

```forge
Materials Int new EmptyList;
```

An empty list starts with zero elements and an initial capacity of four.

Elements can be read and assigned by index:

```forge
Print(Numbers[0]);
Numbers[1] = 50;
```

Lists expose `.Length` and `.Len`:

```forge
Print(Numbers.Length);
```

Available methods include:

| Method             | Description                                  |
| ------------------ | -------------------------------------------- |
| `.Add(value)`      | Adds an element to the list                  |
| `.Remove(index)`   | Removes an element and shifts later elements |
| `.RemoveAt(index)` | Alias for `.Remove(index)`                   |

Example:

```forge
Numbers.Add(40);

Print(Numbers[3]);
Print(Numbers.Length);

Numbers.Remove(0);

Print(Numbers[0]);
Print(Numbers.Length);
```

## Generic Types

`Generic` can be used as the element type of a list.

Example:

```forge
Materials Generic new Items;

Items.Add(999);
Items.Add(1234);

Print(Items[0]);
Print(Items[1]);
```

The current implementation stores generic list elements as integers internally.

Mixed-type generic storage is not currently implemented as a type-checked feature.

## Booleans

Booleans are declared with `Bool` or `Boolean`. The two keywords are identical.

```forge
Bool IsOpen = true;
Boolean IsClosed = false;
```

A `Bool` variable holds exactly one of two values: `true` or `false`. These are recognized as Boolean literals by the lexer and parser, and are represented internally as Boolean values.

A `Bool` variable can be reassigned:

```forge
Bool Flag = true;
Flag = false;
Flag = true;
```

A `Bool` variable may only contain `true`, `false`, or another `Bool` variable. The compiler rejects assignments of integers, floats, or strings to `Bool` variables.

```forge
Bool Value = 1;
```

produces:

```text
error: Type error: cannot assign Int to Bool variable 'Value'
```

Similarly:

```forge
Bool IsOpen = true;
IsOpen = "true";
```

produces:

```text
error: Type error: cannot assign Weld to Bool variable 'IsOpen'
```

`Print` outputs `true` or `false` for Boolean values.

```forge
Open Nunction Main()
{
    Bool Flag = true;
    Print(Flag);
    Flag = false;
    Print(Flag);
}
```

This program outputs:

```text
true
false
```

Boolean variables can be used directly in conditions:

```forge
Bool Running = true;

If (Running)
{
    Print("running");
}
```

The value `true` is treated as a true condition and `false` is treated as a false condition.

---

[← Previous](variables.md)
[Next →](integers.md)
