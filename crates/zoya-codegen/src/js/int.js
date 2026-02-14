var $$Int = {
  abs: function(x) { return Math.abs(x); },
  to_string: function(x) { return String(x); },
  to_float: function(x) { return x; },
  min: function(x, y) { return Math.min(x, y); },
  max: function(x, y) { return Math.max(x, y); },
  pow: function(x, y) { return Math.pow(x, y); },
  clamp: function(x, min, max) { return Math.min(Math.max(x, min), max); },
  signum: function(x) { return Math.sign(x); },
  is_positive: function(x) { return x > 0; },
  is_negative: function(x) { return x < 0; },
  is_zero: function(x) { return x === 0; },
  to_bigint: function(x) { return BigInt(x); }
};