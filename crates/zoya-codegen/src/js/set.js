// Persistent Set<T> backed by HAMT (wraps $$Dict with sentinel values)
var $$Set = (function() {
  var SENTINEL = true;
  var EMPTY = Object.freeze({ $$set: true, $$data: $$Dict.empty() });
  function wrap(data) { return Object.freeze({ $$set: true, $$data: data }); }
  return {
    empty: function() { return EMPTY; },
    contains: function(s, v) { return $$Dict.has(s.$$data, v); },
    insert: function(s, v) { return wrap($$Dict.insert(s.$$data, v, SENTINEL)); },
    remove: function(s, v) { return wrap($$Dict.remove(s.$$data, v)); },
    len: function(s) { return $$Dict.len(s.$$data); },
    to_list: function(s) { return $$Dict.keys(s.$$data); },
    is_disjoint: function(s, o) {
      var ks = $$Dict.keys(s.$$data);
      for (var i = 0; i < ks.length; i++) {
        if ($$Dict.has(o.$$data, ks[i])) return false;
      }
      return true;
    },
    is_subset: function(s, o) {
      var ks = $$Dict.keys(s.$$data);
      for (var i = 0; i < ks.length; i++) {
        if (!$$Dict.has(o.$$data, ks[i])) return false;
      }
      return true;
    },
    is_superset: function(s, o) {
      var ko = $$Dict.keys(o.$$data);
      for (var i = 0; i < ko.length; i++) {
        if (!$$Dict.has(s.$$data, ko[i])) return false;
      }
      return true;
    },
    difference: function(s, o) {
      var ks = $$Dict.keys(s.$$data);
      var d = s.$$data;
      for (var i = 0; i < ks.length; i++) {
        if ($$Dict.has(o.$$data, ks[i])) d = $$Dict.remove(d, ks[i]);
      }
      return wrap(d);
    },
    intersection: function(s, o) {
      var smaller, larger;
      if ($$Dict.len(s.$$data) <= $$Dict.len(o.$$data)) { smaller = s; larger = o; }
      else { smaller = o; larger = s; }
      var ks = $$Dict.keys(smaller.$$data);
      var d = $$Dict.empty();
      for (var i = 0; i < ks.length; i++) {
        if ($$Dict.has(larger.$$data, ks[i])) d = $$Dict.insert(d, ks[i], SENTINEL);
      }
      return wrap(d);
    },
    union: function(s, o) {
      var ko = $$Dict.keys(o.$$data);
      var d = s.$$data;
      for (var i = 0; i < ko.length; i++) {
        d = $$Dict.insert(d, ko[i], SENTINEL);
      }
      return wrap(d);
    },
    from: function(items) {
      var d = $$Dict.empty();
      for (var i = 0; i < items.length; i++) {
        d = $$Dict.insert(d, items[i], SENTINEL);
      }
      return wrap(d);
    }
  };
})();
