var $$BigInt = {
  abs: function(x) { return x < 0n ? -x : x; },
  to_string: function(x) { return String(x); },
  min: function(x, y) { return x < y ? x : y; },
  max: function(x, y) { return x > y ? x : y; }
};