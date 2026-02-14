var $$BigInt = {
  abs: function(x) { return x < 0n ? -x : x; },
  to_string: function(x) { return String(x); },
  min: function(x, y) { return x < y ? x : y; },
  max: function(x, y) { return x > y ? x : y; },
  pow: function(x, y) { return x ** y; },
  clamp: function(x, min, max) { return x < min ? min : x > max ? max : x; },
  signum: function(x) { return x < 0n ? -1n : x > 0n ? 1n : 0n; },
  is_positive: function(x) { return x > 0n; },
  is_negative: function(x) { return x < 0n; },
  is_zero: function(x) { return x === 0n; },
  to_int: function(x) { return Number(x); }
};