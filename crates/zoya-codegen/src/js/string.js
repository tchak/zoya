var $$String = {
  len: function(s) { return s.length; },
  contains: function(s, needle) { return s.includes(needle); },
  starts_with: function(s, prefix) { return s.startsWith(prefix); },
  ends_with: function(s, suffix) { return s.endsWith(suffix); },
  to_uppercase: function(s) { return s.toUpperCase(); },
  to_lowercase: function(s) { return s.toLowerCase(); },
  trim: function(s) { return s.trim(); }
};