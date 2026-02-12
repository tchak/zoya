# Lexical Structure

## Source Encoding

Zoya source files are encoded in UTF-8.

## Whitespace

Whitespace characters (space, tab, newline, carriage return) are not significant and are ignored between tokens.

## Comments

Line comments begin with `//` and extend to the end of the line.

```
comment ::= '//' <any character except newline>*
```

Example:
```zoya
// This is a comment
let x = 42  // inline comment
```

## Keywords

The following identifiers are reserved as keywords:

```
enum    false   fn      let     match   mod
pub     root    self    struct  super   true
type    use
```

### Reserved Names

The following names are not keywords but are reserved and cannot be used as package or module names:

```
std     zoya
```

Note: `root`, `self`, and `super` are already keywords, so they are also rejected as package/module names. Together, the full set of reserved package/module names is: `root`, `self`, `super`, `std`, `zoya`.

## Identifiers

An identifier starts with a letter (`a-z`, `A-Z`) or underscore (`_`), followed by zero or more letters, digits (`0-9`), or underscores.

```
identifier ::= [a-zA-Z_][a-zA-Z0-9_]*
```

Keywords cannot be used as identifiers.

## Literals

### Integer Literals

```
int_literal ::= digit (digit | '_')*
digit       ::= [0-9]
```

Underscores may appear between digits for readability and are ignored.

Examples: `42`, `1_000_000`, `100_`

### BigInt Literals

```
bigint_literal ::= digit (digit | '_')* 'n'
```

Examples: `42n`, `9_000_000_000n`

### Float Literals

```
float_literal ::= digit (digit | '_')* '.' digit (digit | '_')*
```

Both integer and fractional parts are required.

Examples: `3.14`, `1_000.5`, `0.5`

Invalid: `.5`, `1.`, `1`

### String Literals

```
string_literal ::= '"' string_char* '"'
string_char    ::= <any character except '"' or '\'> | escape_sequence
escape_sequence ::= '\n' | '\t' | '\r' | '\\' | '\"'
```

Unknown escape sequences (e.g., `\x`) are preserved literally as backslash followed by the character.

Examples: `"hello"`, `"line\nbreak"`, `"say \"hi\""`

### Boolean Literals

Boolean literals are the keywords `true` and `false`.

## Operators and Punctuation

### Operators

| Token | Name |
|-------|------|
| `+` | Plus |
| `-` | Minus |
| `*` | Star |
| `**` | Power |
| `/` | Slash |
| `%` | Percent |
| `==` | Equal |
| `!=` | Not Equal |
| `<` | Less Than |
| `>` | Greater Than |
| `<=` | Less or Equal |
| `>=` | Greater or Equal |
| `=` | Assignment |
| `->` | Arrow |
| `=>` | Fat Arrow |

### Delimiters

| Token | Name |
|-------|------|
| `(` `)` | Parentheses |
| `{` `}` | Braces |
| `[` `]` | Brackets |
| `<` `>` | Angle Brackets |
| `;` | Semicolon |
| `:` | Colon |
| `::` | Path Separator |
| `,` | Comma |
| `.` | Dot |
| `..` | Rest |
| `\|` | Pipe |
| `@` | At |
| `#` | Hash |
