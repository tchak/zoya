# Expressions

Zoya is an expression-oriented language. All computations produce values; there are no statements other than `let` bindings within blocks.

## Literals

Literal expressions produce values of primitive types. See [Lexical Structure](lexical-structure.md) for detailed syntax.

```
literal ::= int_literal | bigint_literal | float_literal | string_literal
           | interpolated_string | bool_literal
```

```zoya
42
100n
3.14
"hello"
$"hello {name}!"
true
```

### Interpolated Strings

Interpolated strings embed expressions within a string using `$"..."` syntax. Expressions are enclosed in `{` and `}` and must be of type `String`, `Int`, `Float`, or `BigInt`.

```zoya
let name = "world";
$"hello {name}!"           // "hello world!"
$"1 + 2 = {1 + 2}"        // "1 + 2 = 3"
$"pi is {3.14}"            // "pi is 3.14"
$"literal \{ braces \}"    // "literal { braces }"
```

Non-`String` types (`Int`, `Float`, `BigInt`) are automatically converted to their string representation. Other types (e.g., `Bool`, structs, enums) cannot be interpolated and produce a type error.

## Collection Literals

### List Literals

Homogeneous sequences delimited by brackets. Elements can be individual items or spread expressions that expand a list in place.

```
list_literal ::= '[' (list_element (',' list_element)* ','?)? ']'
list_element ::= '..' expr | expr
```

```zoya
[]
[1, 2, 3]
["a", "b", "c"]
[[1, 2], [3, 4]]
[1, ..rest, 4]         // Spread: expand rest in place
[..a, ..b]             // Multiple spreads allowed
```

All elements (including spread results) must have the same type.

### Tuple Literals

Fixed-size heterogeneous sequences delimited by parentheses. Elements can be individual items or spread expressions.

```
tuple_literal   ::= '(' ')' | '(' tuple_element ',' (tuple_element (',' tuple_element)*)? ','? ')'
                   | '(' '..' expr ')'
tuple_element   ::= '..' expr | expr
```

```zoya
()              // Unit (empty tuple)
(42,)           // Single-element tuple (trailing comma required)
(1, "hello")    // Two-element tuple
(true, 42, 3.14)
(..a, 4, ..b)   // Spread: expand tuples in place
```

A single expression in parentheses without a trailing comma is a parenthesized expression, not a tuple. However, `(..expr)` is always a tuple (spread is not a standalone expression):

```zoya
(42)            // Int, not a tuple
(42,)           // (Int,) tuple
(..xs)          // Tuple with spread
```

## Path Expressions

A path identifies a variable, function, type constructor, or module-qualified name.

```
path        ::= prefix? segment ('::' segment)* turbofish?
prefix      ::= 'root' '::' | 'self' '::' | 'super' '::'
segment     ::= identifier
turbofish   ::= '::' '<' type (',' type)* '>'
```

```zoya
x                           // Simple variable
Option::None                // Qualified name
root::utils::helper         // Root-prefixed path
self::foo                   // Current module
super::bar                  // Parent module
Option::None::<Int>         // Turbofish type arguments
```

## Function and Constructor Calls

A path followed by parenthesized arguments is a call expression.

```
call_expr ::= path '(' (expr (',' expr)* ','?)? ')'
```

```zoya
add(1, 2)
Option::Some(42)
identity::<Int>(0)
foo()
```

## Struct Construction

A path followed by braced fields constructs a struct or enum struct variant. An optional spread expression (`..expr`) fills in remaining fields from another value of the same type.

```
struct_expr     ::= path '{' (struct_element (',' struct_element)* ','?)? '}'
struct_element  ::= field | '..' expr
field           ::= identifier ':' expr | identifier
```

When the field value is a variable with the same name as the field, the shorthand form omits the value:

```zoya
let x = 1.0
let y = 2.0
Point { x: 1.0, y: 2.0 }  // Explicit
Point { x, y }             // Shorthand (equivalent)
Point { x: 1.0, y }        // Mixed
```

### Struct Update Syntax

The spread operator `..expr` fills in any fields not explicitly provided. It must appear as the last element and only one spread is allowed:

```zoya
let p = Point { x: 1, y: 2 }
Point { x: 10, ..p }       // Point { x: 10, y: 2 }
Point { ..p }               // Copy all fields
```

## Field Access

Dot notation accesses a named field on a value.

```
field_access ::= expr '.' identifier
```

```zoya
point.x
pair.first
nested.inner.value
```

## Tuple Index

Dot notation with an integer literal accesses an element of a tuple or tuple struct by position.

```
tuple_index ::= expr '.' integer_literal
```

```zoya
(1, "hello").0          // 1
(1, "hello").1          // "hello"
((1, 2), (3, 4)).0.1   // 2 — chained indexing
```

The index must be a non-negative integer within bounds. For tuple structs, indexing accesses positional fields:

```zoya
struct Pair(Int, String)
let p = Pair(42, "hi")
p.0                     // 42
p.1                     // "hi"
```

## Index Expressions

Bracket notation accesses a list element by index, returning `Option<T>`.

```
index_expr ::= expr '[' expr ']'
```

The receiver must be `List<T>` and the index must be `Int`. The result type is `Option<T>` — `Some(value)` for valid indices, `None` for out-of-bounds.

```zoya
[10, 20, 30][1]         // Some(20)
[10, 20, 30][5]         // None
[10, 20, 30][-1]        // Some(30) — negative indices count from end
```

Negative indices: `-1` is the last element, `-2` the second-to-last, etc. Out-of-range negatives return `None`.

```zoya
let xs = [1, 2, 3]
match xs[0] {
  Some(v) => v,
  None => -1,
}
```

Index expressions can be chained with method calls and field access:

```zoya
list.reverse()[0]
```

## Method Calls

Dot notation followed by parenthesized arguments calls a method on the receiver.

```
method_call ::= expr '.' identifier '(' (expr (',' expr)* ','?)? ')'
```

```zoya
"hello".len()
[1, 2, 3].reverse()
x.min(y)
text.trim().to_uppercase()
```

Method calls and field access can be chained and are evaluated left to right.

## Operators

### Unary Operators

```
unary_expr   ::= '-' unary_expr | postfix_expr
postfix_expr ::= atom ('.' integer_literal | '.' identifier ('(' args ')')? | '[' expr ']')*
```

Negation works on `Int`, `BigInt`, and `Float`.

```zoya
-42
-x
-(a + b)
```

### Binary Operators

```
binary_expr ::= expr op expr
```

All binary operators are left-associative, except `**` (power) which is right-associative.

| Operator | Description |
|----------|-------------|
| `**` | Exponentiation |
| `*` | Multiplication |
| `/` | Division |
| `%` | Modulo (remainder) |
| `+` | Addition |
| `-` | Subtraction |
| `==` | Equal |
| `!=` | Not equal |
| `<` | Less than |
| `>` | Greater than |
| `<=` | Less or equal |
| `>=` | Greater or equal |

Arithmetic operators (`+`, `-`, `*`, `/`, `%`, `**`) require both operands to have the same numeric type (`Int`, `BigInt`, or `Float`). Comparison operators (`<`, `>`, `<=`, `>=`) work on numeric types. Equality operators (`==`, `!=`) work on all types.

**Runtime panics:**
- `/` and `%` with `Int` or `BigInt` operands panic on zero divisor ("division by zero", "modulo by zero").
- `**` with `Int` or `BigInt` operands panics on negative exponent ("negative exponent"), since integer exponentiation with negative powers would produce fractional results.
- `Float` operations do not panic (they follow IEEE 754 semantics).

**Modulo semantics:** `%` computes the remainder (like Rust/JavaScript `%`), so `-7 % 3 == -1`.

### Operator Precedence

From highest to lowest:

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 | `-` (unary) | Right |
| 2 | `**` | Right |
| 3 | `*`, `/`, `%` | Left |
| 4 | `+`, `-` | Left |
| 5 | `==`, `!=`, `<`, `>`, `<=`, `>=` | Left |

```zoya
1 + 2 * 3       // 1 + (2 * 3) = 7
-x * y          // (-x) * y
a + b == c + d  // (a + b) == (c + d)
2 ** 3 ** 2     // 2 ** (3 ** 2) = 512 (right-associative)
2 * 3 ** 2      // 2 * (3 ** 2) = 18
10 % 3          // 1
```

## Lambda Expressions

Anonymous functions with closure over the enclosing scope.

```
lambda_expr  ::= '|' params '|' ('->' type)? body
params       ::= (param (',' param)* ','?)?
param        ::= pattern (':' type)?
body         ::= '{' (let_binding ';')* expr '}' | expr
```

```zoya
|x| x + 1
|x, y| x + y
|x: Int| -> Int x * 2
|(a, b)| a + b
```

Lambda bodies can be block expressions:

```zoya
|x| {
  let doubled = x * 2;
  doubled + 1
}
```

Type annotations on parameters are optional when types can be inferred from context.

## Match Expressions

Pattern matching on a scrutinee value with one or more arms.

```
match_expr ::= 'match' expr '{' arm (',' arm)* ','? '}'
arm        ::= pattern '=>' body
body       ::= '{' (let_binding ';')* expr '}' | expr
```

```zoya
match value {
  0 => "zero",
  1 => "one",
  n => "other",
}
```

Arms are comma-separated with an optional trailing comma. Each arm body can be a simple expression or a block:

```zoya
match point {
  Point { x, y } => {
    let sum = x + y;
    sum * 2
  },
}
```

See [Patterns](#patterns) for the full pattern syntax.

## Block Expressions

A sequence of let bindings followed by a result expression, enclosed in braces.

```
block_expr  ::= '{' (let_binding ';')* expr '}'
let_binding ::= 'let' pattern (':' type)? '=' expr
```

```zoya
{
  let x = 1;
  let y = 2;
  x + y
}
```

Each let binding ends with a semicolon. The final expression is the value of the block. Type annotations in let bindings are optional and only allowed on simple variable patterns:

```zoya
{
  let x: Int = 42;
  let (a, b) = (1, 2);
  a + b + x
}
```

Block expressions appear as function bodies, match arm bodies, and lambda bodies.

## Patterns

Patterns are used in `match` arms, `let` bindings, and lambda parameters.

```
pattern ::= literal_pat | wildcard_pat | path_pat | call_pat
          | struct_pat | list_pat | tuple_pat | as_pat
```

### Literal Patterns

```zoya
match x {
  0 => "zero",
  true => "yes",
  "hello" => "greeting",
  _ => "other",
}
```

### Wildcard Pattern

The `_` pattern matches any value and discards it.

### Variable Patterns

A simple identifier binds the matched value to a variable.

```zoya
match x {
  n => n + 1,
}
```

### Constructor Patterns

Unit and tuple enum variants use path and call patterns:

```zoya
match option {
  Option::Some(x) => x,
  Option::None => 0,
}
```

### Struct Patterns

Match struct fields by name, with optional shorthand and partial matching:

```zoya
match point {
  Point { x: 0, y } => y,          // Explicit field pattern
  Point { x, y } => x + y,         // Shorthand (binds to same name)
  Point { x, .. } => x,            // Partial match (ignore other fields)
}
```

### List Patterns

```zoya
match list {
  [] => "empty",
  [x] => "one",
  [x, y] => "two",
  [first, ..] => "at least one",   // Prefix with rest
  [.., last] => "at least one",    // Suffix with rest
  [first, .., last] => "at least two",
  [head, rest @ ..] => rest,       // Bind rest to variable
}
```

### Tuple Patterns

```zoya
match pair {
  (0, 0) => "origin",
  (x, 0) => "on x-axis",
  (0, y) => "on y-axis",
  (x, y) => "other",
}
```

Tuple patterns support rest syntax like list patterns: `(first, ..)`, `(.., last)`, `(first, .., last)`.

### As Patterns

Bind the entire matched value while also matching a sub-pattern:

```zoya
match option {
  n @ Option::Some(_) => n,
  Option::None => Option::None,
}
```
