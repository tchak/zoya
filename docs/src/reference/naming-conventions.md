# Naming Conventions

Zoya enforces naming conventions at compile time.

## Rules

| Element | Convention | Example |
|---------|------------|---------|
| Struct names | PascalCase | `MyStruct` |
| Enum names | PascalCase | `MyEnum` |
| Enum variants | PascalCase | `Some`, `None` |
| Type parameters | PascalCase | `T`, `Key`, `Value` |
| Function names | snake_case | `my_function` |
| Variable names | snake_case | `my_variable` |
| Parameters | snake_case | `user_id` |

## Examples

```zoya
// Correct
struct MyStruct { }
enum MyEnum { MyVariant }
fn my_function(my_param: Int) { }
let my_variable = 42

// Compile errors
struct myStruct { }     // Error: expected PascalCase
fn myFunction() { }     // Error: expected snake_case
```
