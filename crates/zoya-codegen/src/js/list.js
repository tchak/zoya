var $$List = {
  len: function(xs) { return xs.length; },
  reverse: function(xs) { return [...xs].reverse(); },
  push: function(xs, item) { return [...xs, item]; },
  map: function(xs, f) { return xs.map(f); },
  filter: function(xs, f) { return xs.filter(f); },
  fold: function(xs, init, f) { return xs.reduce(f, init); },
  filter_map: function(xs, f) {
    var r = [];
    for (var i = 0; i < xs.length; i++) {
      var v = f(xs[i]);
      if (v.$tag === "Some") r.push(v.$0);
    }
    return r;
  },
  truncate: function(xs, len) { return xs.slice(0, len); },
  insert: function(xs, index, value) { return [...xs.slice(0, index), value, ...xs.slice(index)]; },
  remove: function(xs, index) { return [...xs.slice(0, index), ...xs.slice(index + 1)]; }
};