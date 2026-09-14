# Sydrogen Alpha-6 Language Documentation

**Sydrogen** is a statically typed systems programming language that compiles to native machine code.

Sydrogen source files use the `.anvil` extension. If you drop one on the floor, it will not make a sound.

Sydrogen projects use a `.blower` configuration file:

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

`Project.Name` names the executable. Each `Files.location` glob selects project
sources relative to the `.blower` file; multiple matches are deduplicated and
ordered deterministically. `*` matches within a directory and `**` supports
recursive discovery.

Build the project with `furnace build project.blower`. Its executable is written
beside the project file as `build/PlaceHolder`. Both Native and Cranelift use
this output path. A standalone source remains available through
`furnace compile file.anvil linux`.

`furnace new console -n MyProject` creates `MyProject/MyProject.blower` and
`MyProject/src/Main.anvil`, ready for `furnace build MyProject.blower` from the
new project directory.

The compiler is **Furnace**.

> **Status:** Alpha-6 \
> **Compiler:** Furnace \
> **Implementation:** Rust \
> **Parser:** pest \
> **Code generation:** Direct x86-64 path or Cranelift \
> **Output:** ELF64 executable or native object code 

---

# Table of Contents

1. [Introduction](#1-introduction)
2. [Hello World](#2-hello-world)
3. [Program Structure](#3-program-structure)
4. [Functions](#4-functions)

   4.1. [`Nunction`](#41-nunction)

   4.2. [Returning Functions](#42-returning-functions)

   4.3. [The `Return` Keyword](#43-the-return-keyword)

   4.4. [Function Parameters](#44-function-parameters)
   
   4.5. [Native Function Calls](#45-native-function-calls)
5. [The Main Function](#5-the-main-function)
6. [Variables](#6-variables)

   6.1. [Declaration](#61-declaration)

   6.2. [Assignment](#62-assignment)
7. [Types](#7-types)

   7.1. [Primitive Types](#71-primitive-types)

   7.2. [Arrays](#72-arrays)

   7.3. [Tuples](#73-tuples)

   7.4. [Lists](#74-lists)

   7.5. [Generic Types](#75-generic-types)

   7.6. [Booleans](#76-booleans)
8. [Integers](#8-integers)
9. [Floating-Point Numbers](#9-floating-point-numbers)
10. [Strings](#10-strings)

    10.1. [String Escapes](#101-string-escapes)
11. [String Interpolation](#11-string-interpolation)
12. [Input](#12-input)
13. [Operators](#13-operators)

    13.1. [Arithmetic](#131-arithmetic)

    13.2. [Power](#132-power)

    13.3. [Increment and Decrement](#133-increment-and-decrement)
14. [Unary Operators](#14-unary-operators)
15. [Comparisons](#15-comparisons)
16. [Logical Operators](#16-logical-operators)
17. [Conditionals](#17-conditionals)
18. [While Loops](#18-while-loops)
19. [For Loops](#19-for-loops)
20. [The `Stop` Statement](#20-the-stop-statement)
21. [The `Program` Namespace](#21-the-program-namespace)
22. [Function Calls](#22-function-calls)
23. [Recursion](#23-recursion)
24. [Scope](#24-scope)
25. [Comments](#25-comments)
26. [Visibility](#26-visibility)

    26.1. [`Showcase`](#261-showcase)
27. [Data Declarations](#27-data-declarations)
28. [Object Instantiation](#28-object-instantiation)
29. [Imports](#29-imports)
30. [Semantic Analysis](#30-semantic-analysis)
31. [Compiler Architecture](#31-compiler-architecture)

    31.6. [Native Code Generation](#316-native-code-generation)
32. [Compilation](#32-compilation)
33. [Complete Example](#33-complete-example)
34. [Current Limitations](#34-current-limitations)
35. [Alpha-6 Roadmap](#35-alpha-6-roadmap)

---

# 1. Introduction

Sydrogen is a statically typed programming language that compiles source code to native machine code.

Compiled Sydrogen programs do not require a virtual machine or interpreter at runtime.

The compiler stages are:

```mermaid
flowchart LR
    A(Sydrogen source) --> B(pest)
    B --> C(AST)
    C --> D(Semantic analysis)
    D --> E(Direct native path or Cranelift path)
    E --> F(Executable)
```

Furnace is written in Rust.

Alpha-6 uses:

* **Rust** for the compiler
* **pest** for parsing
* **x86-64 instruction encoding** for the direct native path
* **Cranelift** for programs that need the typed code path
* **Rayon** for parallel semantic analysis
* **cc** for linking

---

# 2. Hello World

A minimal Sydrogen program is:

```forge
Open Nunction Main()
{
    Print("Hello, World!");
}
```

`Main` is the entry point of the program.

`Print` writes a value to standard output.

---

# 3. Program Structure

A Sydrogen program consists of declarations and statements.

A simple program looks like:

```forge
Open Nunction Main()
{
    Int I = 0;

    While (I < 10)
    {
        Print(\V"{I}");
        I = I + 1;
    }
}
```

Sydrogen uses braces `{}` to delimit function and control-flow bodies.

Statements are terminated with `;`.

Top-level declarations can include:

* Function definitions
* Variable declarations
* Data declarations
* Import statements

The exact behavior of some declarations depends on the current compiler implementation.

---

# 4. Functions

Sydrogen functions are declared with either:

* `Nunction` for a function that does not return a value
* A return type, such as `Int`, `Float`, `Weld`, `Bool`, `Ore`, or `Materials`, for a function that returns data

Furnace compiles each user-defined function as an independent native function.
Calls use the function's declared parameter and return types.

## 4.1 `Nunction`

A `Nunction` is a function that does not return a value.

```forge
Nunction Tick()
{
    Print("tick");
}
```

It can be called with:

```forge
Tick();
```

`Nunction` calls do not return a value. They can take parameters and call other functions.

## 4.2 Returning Functions

A returning function declares its return type before its name. It sends a value back to its caller with the `Return` keyword.

For example, this function declares an `Int` return type:

```forge
Int Add(Int A, Int B)
{
    Return A + B;
}
```

A returned value can be used like this:

```forge
Int Result = Add(10, 20);
```

The returned value can be assigned to a compatible variable or used directly in another expression.

Returning functions can also return collection data. The declared shape and element types must match the returned value:

```forge
Ore(Int Number, Weld Name) MakePerson()
{
    Return {14, "Den"};
}
```

## 4.3 The `Return` Keyword

`Return` ends the current function call and sends its expression back to the caller:

```forge
Weld Greeting()
{
    Return "hello";
}
```

`Return` is case-sensitive and must be written with a capital `R`.

A `Nunction` cannot return data because it has no return type. This is an error:

```forge
Nunction Bad()
{
    Return 42;
}
```

Furnace reports `Void function Bad cannot return a value`.

The returned expression must also be compatible with the function's declared return type. Returning a mismatched data type is an error:

```forge
Int Bad()
{
    Return "wrong";
}
```

Furnace reports `Return type mismatch in Bad: expected Int, got Weld`.

## 4.4 Function Parameters

The grammar accepts parameter declarations:

```forge
Nunction PrintNumber(Int Value)
{
    Print(\V"{Value}");
}
```

A call can be written as:

```forge
PrintNumber(42);
```

Furnace checks argument count and argument types during semantic analysis.

## 4.5 Native Function Calls

For example:

```forge
Nunction Tick()
{
    Print("tick");
}

Open Nunction Main()
{
    Tick();
}
```

The compiler compiles `Tick` as an independent native function and emits a call from `Main`.

The same call path supports parameters, return values, calls inside `If`, `While`, and `For`, and recursive calls.

---

# 5. The Main Function

Every executable Sydrogen program requires a `Main` entry point.

The standard form is:

```forge
Open Nunction Main()
{
    // program
}
```

`Open` controls visibility.

`Main` is used as the root of executable code generation.

Furnace searches for a function named `Main`.

---

# 6. Variables

Variables are declared using a type, a name, and optionally an initial value.

Example:

```forge
Int I = 0;
```

A variable can later be assigned:

```forge
I = 10;
```

Variables can be modified multiple times:

```forge
I = I + 1;
I = I + 5;
```

## 6.1 Declaration

General form:

```
Type Name = Value;
```

Examples:

```forge
Int Age = 14;
Float Height = 181.0;
Weld Name = "Sydrogen";
Ore[3] Numbers = [10, 20, 30,];
Materials Int List = (1, 2, 3,);
```

## 6.2 Assignment

General form:

```
Name = Value;
```

Example:

```forge
Age = Age + 1;
```

The assigned value must be compatible with the variable's type.

---

# 7. Types

Alpha-6 currently includes primitive types, arrays, tuples, lists, and a limited generic type.

## 7.1 Primitive Types

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

## 7.2 Arrays

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

## 7.3 Tuples

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

## 7.4 Lists

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

## 7.5 Generic Types

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

## 7.6 Booleans

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

```
error: Type error: cannot assign Int to Bool variable 'Value'
```

Similarly:

```forge
Bool IsOpen = true;
IsOpen = "true";
```

produces:

```
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

```
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

# 8. Integers

Integer values use:

```forge
Int
```

Example:

```forge
Int Counter = 0;
```

Integer arithmetic supports:

```forge
Int A = 10;
Int B = 5;

Int Add = A + B;
Int Subtract = A - B;
Int Multiply = A * B;
Int Divide = A / B;
```

Integer variables can be modified:

```forge
Counter = Counter + 1;
```

---

# 9. Floating-Point Numbers

Floating-point values use:

```forge
Float
```

Example:

```forge
Float Temperature = 21.5;
```

Floating-point arithmetic supports the standard arithmetic operators:

```forge
Float A = 10.5;
Float B = 2.0;

Float Result = A + B;
```

---

# 10. Strings

`Weld` is the canonical string type. `String` is an alias for `Weld`; both
names resolve to the same internal type.

Example:

```forge
Weld Name = "Sydrogen";
String Alias = Name;
```

String literals use double quotes:

```forge
"Hello"
```

Strings can be passed to `Print`:

```forge
Print("Hello, World!");
```

## 10.1 String Escapes

String literals support:

| Sequence | Meaning         |
| -------- | --------------- |
| `\n`     | Newline         |
| `\t`     | Tab             |
| `\r`     | Carriage return |
| `\0`     | Null byte       |
| `\\`     | Backslash       |
| `\"`     | Double quote    |

Example:

```forge
Weld Line = "Hello\nWorld";
```

---

# 11. String Interpolation

Sydrogen supports interpolation using the `\V` string form.

Example:

```forge
Int I = 42;

Print(\V"{I}");
```

The value of `I` is inserted into the string.

Multiple values can be used:

```forge
Int A = 10;
Int B = 20;

Print(\V"A = {A}, B = {B}");
```

Member access is also supported:

```forge
Ore(Int Age, Weld Name) Person = {14, "Den"};

Print(\V"{Person.Name} is {Person.Age}");
```

---

# 12. Input

Sydrogen provides typed input through `Input`.

## 12.1 Integer Input

```forge
Int Value = Input(Int);
```

This reads an integer from standard input.

## 12.2 Floating-Point Input

```forge
Float Value = Input(Float);
```

This reads a floating-point value.

## 12.3 String Input

```forge
Weld Value = Input(Weld);
```

This reads a string from standard input.

The current implementation uses standard C input facilities internally.

---

# 13. Operators

Alpha-6 supports arithmetic, comparison, bitwise, unary, and loop increment operators.

## 13.1 Arithmetic

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

## 13.2 Power

The `**` operator performs exponentiation.

```forge
Int Result = 2 ** 8;
```

Power expressions are right-associative.

For example:

```
A ** B ** C
```

is interpreted as:

```
A ** (B ** C)
```

The current implementation lowers power operations through the C `pow` function.

## 13.3 Increment and Decrement

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

---

# 14. Unary Operators

Alpha-6 supports unary negation and unary plus.

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

---

# 15. Comparisons

Comparison operators can be used in conditions and loops.

| Operator | Meaning               |
| -------- | --------------------- |
| `<`      | Less than             |
| `>`      | Greater than          |
| `<=`     | Less than or equal    |
| `>=`     | Greater than or equal |
| `==`     | Equal                 |
| `!=`     | Not equal             |

Example:

```forge
If (A < B)
{
    Print("A is smaller");
}
```

---

# 16. Logical Operators

Sydrogen currently provides `And`, `Or`, and `Xor` as bitwise operators for integer values.

| Operator | Operation   |
| -------- | ----------- |
| `And`    | Bitwise AND |
| `Or`     | Bitwise OR  |
| `Xor`    | Bitwise XOR |

Precedence is:

```
And > Or > Xor
```

Example:

```forge
Int A = 12;
Int B = 10;

Int C = A And B;
Int D = A Or B;
Int E = A Xor B;
```

These operators currently operate on integer operands.

---

# 17. Conditionals

Sydrogen supports:

* `If`
* `Else If`
* `Else`

## 17.1 If

```forge
If (I < 10)
{
    Print("Less than ten");
}
```

## 17.2 Else

```forge
If (I < 10)
{
    Print("Small");
}
Else
{
    Print("Large");
}
```

## 17.3 Else If

```forge
If (I < 10)
{
    Print("Small");
}
Else If (I < 100)
{
    Print("Medium");
}
Else
{
    Print("Large");
}
```

Conditions must evaluate to a valid condition for the current compiler.

---

# 18. While Loops

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

## 18.1 Nested Loops

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

# 19. For Loops

Sydrogen supports C-style `For` loops.

Basic syntax:

```forge
For (Init; Condition; Increment)
{
    // body
}
```

Example:

```forge
For (Int I = 0; I < 10; I++)
{
    Print(\V"{I}");
}
```

A `For` loop has three parts:

1. **Init**
   Runs once before the loop.

2. **Condition**
   Is evaluated before each iteration.

3. **Increment**
   Runs after each iteration.

The increment expression currently uses `++` or `--`.

Example:

```forge
For (Int I = 10; I > 0; I--)
{
    Print(\V"{I}");
}
```

A `For` loop without braces produces an error.

---

# 20. The `Stop` Statement

`Stop` provides an early exit from a loop or conditional block.

Example:

```forge
While (I < 10)
{
    If (I == 5)
    {
        Stop;
    }

    I = I + 1;
}
```

The semantic analyzer currently enforces these rules:

* `Stop` cannot be used inside `Main`
* `Stop` must be inside a loop or `If` statement

In the compiler, `Stop` is represented as `Statement::Stop` in `src/ast.rs`.

The semantic checks are implemented in `src/semantic.rs`.

Code generation is handled by `FunctionCompiler::compile_statement` in `src/codegen.rs`.

`Stop` is not an exception mechanism. It represents a structured early exit.

---

# 21. The `Program` Namespace

The `Program` namespace provides runtime operations.

The currently implemented member is:

```forge
Program.Stop();
```

This terminates the program by calling the C `exit` function with a status of `0`.

`Program.Stop()` is treated as a namespace operation rather than a normal user-defined function call.

The semantic analyzer checks that:

* It is used inside a function
* It has zero arguments
* The namespace and operation are valid

Example:

```forge
Open Nunction Main()
{
    Print("before");
    Program.Stop();
    Print("after");
}
```

The second `Print` is never reached.

---

# 22. Function Calls

Functions are called using their name followed by parentheses.

A zero-argument call:

```forge
Tick();
```

A call with arguments:

```forge
PrintNumber(42);
```

A call with multiple arguments:

```forge
Add(10, 20);
```

Furnace compiles calls as native calls to independently compiled Sydrogen functions.

Semantic analysis checks the function name, argument count, and argument types before code generation.

---

# 23. Recursion

The grammar can represent recursive functions.

Example:

```forge
Nunction Countdown(Int I)
{
    If (I > 0)
    {
        Print(\V"{I}");
        Countdown(I - 1);
    }
}
```

Parameterized calls and return values are supported by the current native function-call path. Recursive syntax is accepted; backend support still depends on the types and operations used by the function.

---

# 24. Scope

Variables declared inside a block are intended to belong to that block.

Example:

```forge
Open Nunction Main()
{
    Int I = 10;

    If (I > 0)
    {
        Int V = 20;
    }
}
```

`V` is declared inside the `If` block.

The AST represents nested blocks, while the current backend uses a function-level variable map.

Full lexical scope, shadowing, and capture rules are still under development.

---

# 25. Comments

Sydrogen supports single-line comments using `//`.

Example:

```forge
// This is a comment

Int I = 0;
```

Comments are ignored by the compiler.

---

# 26. Visibility

Sydrogen uses visibility modifiers to control declaration visibility.

The current visibility keywords are:

* `Open`
* `Closed`
* `Showcase`

Example:

```forge
Open Nunction Main()
{
}
```

`Open` declarations can cross module boundaries through `Use` and `Using`.
`Closed` declarations remain private to their source module.

## 26.1 `Showcase`

`Showcase` is a third visibility modifier.

It is currently parsed and stored in the AST but does not affect code generation.

It is reserved for future functionality related to exported declarations, documentation, or REPL introspection.

Example:

```forge
Showcase Nunction Helper()
{
    Print("helper");
}
```

---

# 27. Data Declarations

Sydrogen supports the `Data` keyword for declaring structured types.

Example:

```forge
Data Person
{
    Int Age;
    Weld Name;
}
```

A `Data` declaration contains typed fields.

The `Data` declaration can use a visibility modifier:

```forge
Open Data Point
{
    Int X;
    Int Y;
}
```

Data declarations are currently parsed and represented in the AST.

The current backend does not generate executable code for `Data` declarations.

---

# 28. Object Instantiation

Instances of `Data` types use object declaration syntax.

Example:

```forge
Person Den
{
    Age = 14;
    Name = "Den";
}
```

Members can also be separated using commas:

```forge
Person Den { Age = 14, Name = "Den"; }
```

Nested member paths are supported by the parser:

```forge
Person Den { Address.City = "Den", Age = 14; }
```

Object declarations are represented as `Statement::ObjectDecl`.

The current backend does not generate executable code for object declarations.

They are currently checked for structural validity.

---

# 29. Imports

Sydrogen provides `Use` and `Using` for importing modules and symbols. Import
resolution happens before semantic analysis and is shared by Native and
Cranelift.

## 29.1 Use

```forge
Use File;

Open Nunction Main()
{
    File.Function1();
}
```

`Use` imports a module while preserving its namespace. It does not make
`Function1()` directly available.

## 29.2 Using

```forge
Using File: Function1;

Open Nunction Main()
{
    Function1();
}
```

`Using` imports one specific symbol directly into the current module's scope.
It does not expose every symbol from that module.

```forge
Use File;
File.Function1();

Using File: Function1;
Function1();
```

## 29.3 Module Discovery

For `.blower` projects, only files selected by `Files.location` belong to the
module table and can be imported. Module names come from `.anvil` filenames, so
`src/File.anvil` defines module `File`. Duplicate module filenames are rejected
instead of resolved ambiguously.

For standalone `furnace compile Main.anvil linux`, `Use File;` resolves
`File.anvil` relative to `Main.anvil`. Nested import paths such as
`Use System.Math;` are represented structurally and resolve a matching
`System/Math.anvil`; this syntax does not pretend that an unavailable standard
library exists.

## 29.4 Visibility, Conflicts, and Cycles

Only `Open` declarations are accessible across module boundaries. Functions
are callable through either import form; existing named declarations such as
`Data` types can be selected with `Using` where their normal syntax permits.
Repeated identical imports are idempotent. Furnace rejects missing modules,
missing or private symbols, duplicate module names, selective-import name
conflicts, and imports that conflict with local declarations.

Imports belong to the module that declares them and do not implicitly re-export
nested dependencies. Furnace builds a dependency graph once and rejects direct
and longer cycles:

```text
error: circular import detected: A -> B -> C -> A
```

`Use` and `Using` resolve only local or project-declared source modules. Package
registries, downloads, versions, wildcard imports, and remote modules are not
part of this feature.

---

# 30. Semantic Analysis

Furnace performs semantic analysis before code generation.

The semantic stage checks the parsed AST for invalid programs.

It currently handles checks including:

* Undefined variables
* Invalid function calls
* Incorrect function argument counts
* Invalid `Main` parameters
* Invalid shared-member access
* Forbidden shared-member mutation
* Invalid `Stop` usage
* Invalid `Program.Stop()` usage
* Unknown collection methods
* Array size mismatches
* Tuple field count mismatches
* List element type mismatches
* Invalid empty-list declarations
* Boolean type compatibility for `Bool` and `Boolean` variables
* Other language-level errors

Semantic analysis happens before either code generation path.

This keeps invalid programs from being passed directly to the backend.

## 30.1 Parallel Semantic Analysis

Furnace uses **Rayon** for parallel semantic analysis.

Independent portions of the AST can be analyzed concurrently.

For example, independent function declarations can be processed concurrently.

This applies to compiler analysis only.

A Sydrogen program containing:

```forge
While (Condition)
{
    // work
}
```

still executes that loop on one thread unless future language features explicitly introduce parallel execution.

---

# 31. Compiler Architecture

Furnace is divided into several stages.

## 31.1 Parser

The parser uses **pest**, a PEG parser generator for Rust.

It converts Sydrogen source into the AST.

The grammar uses explicit precedence rules rather than left-recursive expression rules.

Operator precedence, from highest to lowest, is:

```mermaid
flowchart LR
    A(primary) --> B(postfix)
    B --> C(power)
    C --> D(unary)
    D --> E(multiplicative)
    E --> F(additive)
    F --> G(comparison)
    G --> H(and)
    H --> I(or)
    I --> J(xor)
```

A documented language rule is that unary operators bind looser than `**`.

Therefore:

```forge
-2 ** 2
```

is interpreted as:

```
-(2 ** 2)
```

which produces:

```
-4
```

## 31.2 AST

The AST is represented using Rust structures.

It acts as the representation shared between parsing, semantic analysis, and code generation.

The compiler works with the AST rather than passing raw source text between compiler stages.

## 31.3 Semantic Analysis

`src/semantic.rs` validates the AST before code generation.

This stage handles language-level checks such as:

* Type compatibility
* Variable lookup
* Function lookup
* Function argument counts
* Collection operations
* Scope-related checks
* Control-flow restrictions

## 31.4 Code Generation

Furnace has two code generation paths. The direct native path lowers supported programs to the internal representation in `src/ir.rs`, writes x86-64 instruction bytes, and creates an ELF64 executable. The path in `src/codegen.rs` converts programs that need typed features to **Cranelift IR** and produces an object file.

The paths share the AST and semantic analysis. Furnace selects the path after semantic analysis based on the types and statements used by the program.

Cranelift handles:

* Instruction selection
* Register allocation
* Machine code generation
* Target-specific code generation
* Object-file generation

The Cranelift path produces a native object file from the generated code.

### 31.4.1 Native Function Calls

The Cranelift path compiles each Sydrogen function as an independent Cranelift function and emits calls from the caller.

This supports parameters, return values, calls inside `If`, `While`, and `For`, and recursive calls.

### 31.4.2 Collection Layout

The current backend uses `malloc` for heap allocation of arrays, tuples, and lists.

Their layouts are:

**Array (`Ore`)**

| Offset    | Contents     |
| --------- | ------------ |
| 0         | Length       |
| 8         | Element size |
| 16 onward | Element data |

Each element currently occupies 8 bytes.

**List (`Materials`)**

| Offset | Contents       |
| ------ | -------------- |
| 0      | Length         |
| 8      | Capacity       |
| 16     | Buffer pointer |

**Tuple (`Ore` with named fields)**

Fields are stored starting at offset 0 in declaration order.

Each field currently occupies 8 bytes.

These layouts are implementation details of the current backend and may change in future compiler versions.

## 31.5 Linking

The direct native path creates an ELF64 executable itself, so it does not call a linker.

The Cranelift path generates a native object file.

The object file is linked using the system C compiler.

For example:

```fish
cc main.o -o main -lm
```

The linker produces the final executable.

## 31.6 Native Code Generation

The direct native path is implemented in `src/lowering.rs`, `src/ir.rs`, `src/backend/x86_64/`, and `src/backend/elf.rs`.

### 31.6.1 Lowering the AST

`src/lowering.rs` converts the checked AST into the internal representation in `src/ir.rs`.

The representation contains:

* Virtual registers for temporary values
* Named stack variables
* Basic blocks with labels
* Arithmetic and comparison instructions
* Function calls and parameters
* Print and input instructions
* Jumps, conditional branches, and returns

`If`, `While`, and `For` statements become basic blocks. Each branch receives a label. Each loop has a condition block and an exit block. The code still uses virtual registers at this stage.

### 31.6.2 Placing Values

`src/backend/x86_64/mod.rs` assigns virtual registers to `RBX`, `R12`, `R13`, `R14`, and `R15`. Values that do not fit in those registers use stack slots. Function variables also use stack slots.

Function arguments use the first six integer registers from the System V x86-64 calling convention:

```text
RDI, RSI, RDX, RCX, R8, R9
```

Function results are returned in `RAX`.

### 31.6.3 Writing Instructions

`src/backend/x86_64/encoder.rs` writes instruction bytes into a byte buffer. It handles moves, integer arithmetic, bitwise operations, comparisons, calls, returns, jumps, function stack frames, and the system calls used by the startup code.

The encoder targets x86-64 directly. It does not send this code to another compiler.

### 31.6.4 Fixing Addresses

Function calls and jumps can refer to code that has not been placed yet. Furnace first writes a temporary relative offset and records its position. After all functions and blocks are placed, it fills in the offsets for function calls, branch jumps, helper calls, and pointers to string data.

### 31.6.5 Building the ELF64 File

`src/backend/elf.rs` writes:

1. An ELF64 header
2. A loadable program header
3. A startup entry point
4. The generated functions and helper routines
5. The string data used by the program

The startup entry point calls `Main`, moves its return value into the Linux exit-status register, and makes the exit system call. The command-line compiler writes the resulting bytes to the output file and marks it executable.

### 31.6.6 Path Selection

The direct path currently handles integer, Boolean, and `Weld` values, integer input, strings used by printing, arithmetic, comparisons, branches, loops, function calls, and `Program.Stop()`.

Programs that use floats, arrays, tuples, lists, or other typed features use the Cranelift path. That path creates an object file, adds the C runtime helpers, and calls `cc` to make the final executable.

Both paths share parsing, the AST, and semantic analysis. The difference begins after the program has been checked.

### 31.6.7 Direct Path Limits

The direct path currently has these limits:

* x86-64 instruction output only
* At most six integer function arguments
* Integer, Boolean, and `Weld` function values
* Integer input only
* No direct float, array, tuple, or list code generation
* No direct `Data` or object code generation

Imports resolve before backend selection, so imported functions work through
the direct path without backend-specific import instructions. Other syntax can
be parsed and semantically checked before code generation rejects it or sends it
to the other path.

---

# 32. Compilation

## 32.1 Build Furnace

Furnace is built using Cargo:

```fish
cargo build --release
```

The resulting release compiler is located at:

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
furnace compile main.anvil linux
furnace run main.anvil
furnace backend native
furnace backend cranelift
furnace -help
furnace --help
furnace -version
furnace --version
```

This means you can use `furnace` directly from any directory, provided Cargo's binary directory is available in your `PATH`.

Documentation will use `cargo build --release` instead of `cargo install --path .`,However `cargo install --path` is a better version.

## 32.2 Compile a Sydrogen Program

The current f command is:

```fish
./target/debug/furnace compile main.anvil linux
```

The compile process:

1. Checks that the input file uses the `.anvil` extension.
2. Reads the source file.
3. Parses the source.
4. Builds the AST.
5. Performs semantic analysis.
6. Selects the direct native path or the Cranelift path.
7. Generates functions and calls.
8. Writes an ELF64 executable directly, or creates an object file and invokes the platform linker.
9. Produces the executable.

Example output:

```
Compiling main.anvil...
Linking...
Build successful!
Output: ./main
```

The CLI uses the `Platform` enum in `Cli/platform.rs`.

The currently supported target is:

```
linux
```

Additional targets can be added as compiler support is implemented.

## 32.3 Run a Sydrogen Program

The CLI can compile and execute a program directly:

```fish
./target/debug/furnace run main.anvil
```

This command:

1. Compiles the source.
2. Links the generated object.
3. Executes the resulting binary.
4. Forwards the program's standard output and standard error.
5. Returns the child process exit code.

## 32.4 Persistent Backend Selection

Use the `backend` command to save the default backend:

```fish
./target/debug/furnace backend native
./target/debug/furnace backend cranelift
```

Available backends:

* `native` writes a direct x86-64 ELF64 executable.
* `cranelift` writes a native object file and links it with `cc`.

The saved backend is automatically used by `furnace build`, `furnace compile`,
and `furnace run`, and persists across separate terminal sessions and Furnace
processes. Cranelift remains the built-in default when no setting has been
saved.

On Unix, the setting is stored at `$XDG_CONFIG_HOME/furnace/config`, falling
back to `~/.config/furnace/config`. On Windows it is stored at
`%APPDATA%\furnace\config`.

Use `--backend` with `build`, `compile`, or `run` for a one-command override:

```fish
./target/debug/furnace build Project.blower --backend native
./target/debug/furnace compile main.anvil linux --backend native
./target/debug/furnace run main.anvil --backend cranelift
```

Backend resolution priority is the explicit command override, the saved
preference, and then the built-in default. Invalid saved configuration is
reported as an error.

## 32.5 Create a Project

Create a console project with:

```fish
./target/debug/furnace new console -n Project
```

The command creates:

```text
Project/
└── Project.anvil
```

The generated file contains a minimal `Open Nunction Main()` program. The supported application type is `console`. Furnace rejects unknown types, empty names, and existing project directories.

## 32.6 Version and Help

Furnace exposes its version through centralized compiler metadata.

The current version is:

```rust
pub const VERSION: &str = "Alpha-6";
```
> Fancy way of saying Alpha-6

Version information can be requested with:

```fish
./target/debug/furnace -version
```

Downloading the newest Furnace version:

```fish
# If you havent switched to cargo install:
./target/debug/furnace update 
# cargo install version:
furnace update
```

Help can be requested with:

```fish
./target/debug/furnace -help
```

Example version output:

```
Furnace Alpha-6
```

Usage:

```
Usage:
    furnace compile <file>.anvil <platform> [--backend native|cranelift]
    furnace run <file>.anvil [--backend native|cranelift]
    furnace backend <native|cranelift>
    furnace new <APP_TYPE> -n <NAME>
    furnace -version
    furnace -help

Available backends:
    native: direct x86-64 ELF64 executable
    cranelift: native object file linked with cc
```

## 32.7 Link the Object Manually

Furnace produces a native object file that can be linked separately:

```fish
cc main.o -o main -lm
```

The exact libraries required may depend on the generated program and target platform.

## 32.8 Run

The resulting executable can be started normally:

```fish
./main
```

---

# 33. Complete Example

The following program demonstrates several features available in Alpha-6:

* `Nunction`
* Variables
* Arrays
* Tuples
* Lists
* `While`
* `For`
* `If`
* `Else`
* Arithmetic
* String interpolation
* `Stop`
* Bitwise operations
* Zero-argument function calls
* Boolean values

```forge
Nunction Tick()
{
    Print("tick");
}

Open Nunction Main()
{
    // Array
    Ore[3] FixedNums = [10, 20, 30,];

    Print(FixedNums[0]);

    FixedNums[1] = 55;

    Print(FixedNums[1]);
    Print(FixedNums.Length);

    // Tuple
    Ore(Int Age, Weld Name) Person = {14, "Den"};

    Print(Person.Age);
    Print(Person.Name);

    Person.Age = 15;

    Print(Person.Age);

    // List
    Materials Int Numbers = (10, 20, 30,);

    Print(Numbers[0]);
    Print(Numbers.Length);

    Numbers.Add(40);

    Print(Numbers[3]);

    Numbers.Remove(0);

    Print(Numbers[0]);

    // While loop
    Int I = 0;

    While (I < 5)
    {
        Print(\V"{I}");
        I = I + 1;
    }

    // For loop
    For (Int J = 0; J < 3; J++)
    {
        Print(\V"J = {J}");
    }

    // Boolean
    Bool Flag = true;
    Print(Flag);
    Flag = false;
    Print(Flag);

    // Stop inside a loop
    Int K = 0;

    While (K < 10)
    {
        If (K == 3)
        {
            Stop;
        }

        Print(\V"{K}");
        K = K + 1;
    }

    // Zero-argument Nunction call
    Tick();

    // Bitwise operations
    Int A = 12;
    Int B = 10;

    Print(A And B);
    Print(A Or B);
    Print(A Xor B);
}
```

---

# 34. Current Limitations

Alpha-6 is an early development release.

The following features are not currently fully implemented in the backend:

* `Data` declaration code generation
* Object instantiation code generation
* `Switch` / `Deal` / `Base` pattern matching
* Error handling in the Cranelift backend and Windows uncaught-error printing
* Package and dependency management
* Complete lexical scope handling
* Multicore Sydrogen program execution
* Garbage collection
* Self-hosting Furnace
* Complete systems-level standard library

Some of these features are already represented in the grammar or AST.

There is an important distinction:

**Parsed** means Furnace can recognize the syntax.

**Semantically checked** means Furnace can inspect the construct and report certain errors.

**Code generated** means Furnace can produce executable native code for the construct.

A feature being parsed does not mean that it can currently be used in an executable program.

This distinction saves everyone from discovering that the compiler supports something only after the compiler politely refuses to compile it.

---

# 35. Alpha-6 Roadmap

Alpha-6 continues development of the Rust-based Furnace compiler.

## Short Term

### `Switch` / Pattern Matching

Planned constructs include:

```
Switch
Deal
Base
```

These are intended to provide pattern matching.

### Error Handling Follow-up

`Do`, `Catch`, `Finally`, and `Throw` are implemented by the direct native
backend. Cranelift support and Windows uncaught-error printing remain planned.

### Generic Data Types

`Generic` is currently limited.

Future versions are planned to support generic data types and generic function parameters.

## Mid Term

### Module Packaging

Sydrogen now has local and `.blower` project imports based around:

```
Use
Using
```

The implemented import system applies `Open` and `Closed` visibility. Future
module work can extend standard-library and package integration and define the
role of:

```
Showcase
```

visibility rules.

### Additional Function Features

Parameterized functions and return-value functions are supported by the current native function call path.

Future work includes clearer diagnostics for control-flow paths that do not return and broader support for generic function types.

### Multicore Runtime

Alpha-6 uses Rayon for compiler-side parallel analysis.

Future versions are planned to provide mechanisms for Sydrogen programs to execute work on multiple CPU cores.

Possible constructs include:

```
Spawn
Join
```

The exact syntax and safety rules are not final.

The compiler's parallel analysis and a program's parallel execution are separate features.

## Long-Term Plans

### Scrap

**Scrap** is planned as an optional garbage collector for Sydrogen.

It is intended to be disabled by default.

The default memory model is intended to keep memory management explicit.

Scrap would provide another memory-management option without making garbage collection mandatory.

### Ironwork

**Ironwork** is planned as the Sydrogen package manager.

Its planned responsibilities include:

* Package management
* Dependency resolution
* Library distribution
* Project management
* Sydrogen package integration

### Self-Hosting

A long-term goal is to rewrite Furnace in Sydrogen itself.

This is targeted for the **2.0 generation** of Sydrogen.

The compiler will need sufficient language features, standard library support, and tooling before this becomes practical.

---

# Alpha-6 Implementation Notes

Alpha-6 uses a different compiler implementation from the earlier experimental versions of Sydrogen.

Earlier versions used:

Alpha 1.x and 2.x used:

```
Python
ANTLR
LLVM
```

Alpha 3.x+ uses:

```
Rust
pest
Cranelift
```

The current compiler pipeline is:

```mermaid
flowchart LR
    A(Sydrogen source) --> B(pest)
    B --> C(AST)
    C --> D(Semantic analysis)
    D --> E(Direct native path or Cranelift path)
    E --> F(Executable)
```

The change to Rust also makes the compiler itself part of the Sydrogen project's systems-level development work.

Alpha-6 should not be treated as a finished language specification.

Some syntax exists before its backend implementation.

Some AST structures exist before their code generation.

Some planned language features are already represented in the parser even though the compiler cannot execute them yet.

That is normal for a compiler under active development.

For now, Furnace can compile a growing subset of Sydrogen to native code while the rest of the language catches up.
