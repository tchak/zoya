# Methods

Zoya provides built-in methods on primitive types.

## String Methods

```zoya
"hello".len()           // 5
"hello".is_empty()      // false
"hello".contains("ell") // true
"hello".to_uppercase()  // "HELLO"
"hello".to_lowercase()  // "hello"
```

## Int Methods

```zoya
(-5).abs()              // 5
42.to_string()          // "42"
3.min(5)                // 3
3.max(5)                // 5
```

## Float Methods

```zoya
3.14.floor()            // 3.0
3.14.ceil()             // 4.0
3.14.round()            // 3.0
4.0.sqrt()              // 2.0
3.14.abs()              // 3.14
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
[1, 2, 3].first()       // Option::Some(1)
[1, 2, 3].last()        // Option::Some(3)
```
