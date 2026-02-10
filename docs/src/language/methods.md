# Methods

Zoya provides built-in methods on primitive and collection types.

## String Methods

```zoya
"hello".len()              // 5
"hello".is_empty()         // false
"hello".contains("ell")    // true
"hello".starts_with("he")  // true
"hello".ends_with("lo")    // true
"hello".to_uppercase()     // "HELLO"
"HELLO".to_lowercase()     // "hello"
"  hi  ".trim()            // "hi"
```

## Int Methods

```zoya
(-5).abs()              // 5
42.to_string()          // "42"
42.to_float()           // 42.0
3.min(5)                // 3
3.max(5)                // 5
```

## BigInt Methods

```zoya
(-5n).abs()             // 5n
42n.to_string()         // "42"
3n.min(5n)              // 3n
3n.max(5n)              // 5n
```

## Float Methods

```zoya
3.14.floor()            // 3.0
3.14.ceil()             // 4.0
3.14.round()            // 3.0
4.0.sqrt()              // 2.0
3.14.abs()              // 3.14
3.14.to_string()        // "3.14"
3.7.to_int()            // 3
3.14.min(2.0)           // 2.0
3.14.max(5.0)           // 5.0
```

## List Methods

Lists support index access with bracket notation, returning `Option<T>`:

```zoya
[10, 20, 30][0]         // Some(10)
[10, 20, 30][-1]        // Some(30)
[10, 20, 30][5]         // None
```

All list operations return new lists (immutable):

```zoya
[1, 2].len()            // 2
[1, 2].is_empty()       // false
[1, 2].push(3)          // [1, 2, 3]
[1, 2].concat([3, 4])   // [1, 2, 3, 4]
[1, 2, 3].reverse()     // [3, 2, 1]
```
