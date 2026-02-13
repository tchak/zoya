// Persistent HAMT (Hash Array Mapped Trie) for Dict<K, V>
// All nodes are plain frozen JS objects with $$hamt marker.

var $$Dict = (function() {
  // --- Hashing ---
  function $$hash(v) {
    if (typeof v === "number") return v | 0;
    if (typeof v === "bigint") return Number(v & 0x7fffffffn) | 0;
    if (typeof v === "boolean") return v ? 1 : 0;
    if (typeof v === "string") {
      var h = 0;
      for (var i = 0; i < v.length; i++) {
        h = (Math.imul(31, h) + v.charCodeAt(i)) | 0;
      }
      return h;
    }
    if (Array.isArray(v)) {
      var h = 1;
      for (var i = 0; i < v.length; i++) {
        h = (Math.imul(31, h) + $$hash(v[i])) | 0;
      }
      return h;
    }
    if (typeof v === "object" && v !== null) {
      var h = 1;
      var keys = Object.keys(v).sort();
      for (var i = 0; i < keys.length; i++) {
        h = (Math.imul(31, h) + $$hash(keys[i])) | 0;
        h = (Math.imul(31, h) + $$hash(v[keys[i]])) | 0;
      }
      return h;
    }
    return 0;
  }

  var BITS = 5;
  var WIDTH = 1 << BITS; // 32
  var MASK = WIDTH - 1;

  function popcount(x) {
    x = x - ((x >> 1) & 0x55555555);
    x = (x & 0x33333333) + ((x >> 2) & 0x33333333);
    return (((x + (x >> 4)) & 0x0f0f0f0f) * 0x01010101) >> 24;
  }

  // Empty HAMT node
  var EMPTY = Object.freeze({ $$hamt: true, bitmap: 0, children: Object.freeze([]), size: 0 });

  // Leaf node: { $$hamt: true, hash, key, value, size: 1 }
  function leaf(hash, key, value) {
    return Object.freeze({ $$hamt: true, hash: hash, key: key, value: value, size: 1 });
  }

  // Collision node: { $$hamt: true, hash, bucket: [{key, value}, ...], size }
  function collision(hash, bucket) {
    return Object.freeze({ $$hamt: true, hash: hash, bucket: Object.freeze(bucket), size: bucket.length });
  }

  // Internal node: { $$hamt: true, bitmap, children: [...], size }
  function internal(bitmap, children, size) {
    return Object.freeze({ $$hamt: true, bitmap: bitmap, children: Object.freeze(children), size: size });
  }

  function isLeaf(node) { return node.key !== undefined; }
  function isCollision(node) { return node.bucket !== undefined; }
  function isInternal(node) { return node.bitmap !== undefined && node.bucket === undefined && node.key === undefined; }

  function getIndex(bitmap, bit) {
    return popcount(bitmap & (bit - 1));
  }

  // --- Lookup ---
  function get(node, hash, key, shift) {
    if (node === EMPTY) return undefined;
    if (isLeaf(node)) {
      return (node.hash === hash && $$eq(node.key, key)) ? node.value : undefined;
    }
    if (isCollision(node)) {
      if (node.hash !== hash) return undefined;
      for (var i = 0; i < node.bucket.length; i++) {
        if ($$eq(node.bucket[i].key, key)) return node.bucket[i].value;
      }
      return undefined;
    }
    // Internal node
    var frag = (hash >>> shift) & MASK;
    var bit = 1 << frag;
    if ((node.bitmap & bit) === 0) return undefined;
    var idx = getIndex(node.bitmap, bit);
    return get(node.children[idx], hash, key, shift + BITS);
  }

  // --- Insert ---
  function insert(node, hash, key, value, shift) {
    if (node === EMPTY) return leaf(hash, key, value);

    if (isLeaf(node)) {
      if (node.hash === hash) {
        if ($$eq(node.key, key)) {
          return leaf(hash, key, value); // replace
        }
        // Hash collision — create collision node
        return collision(hash, [{ key: node.key, value: node.value }, { key: key, value: value }]);
      }
      // Different hashes — create internal node with both
      return mergeLeaves(shift, node, leaf(hash, key, value));
    }

    if (isCollision(node)) {
      if (node.hash === hash) {
        var newBucket = node.bucket.slice();
        for (var i = 0; i < newBucket.length; i++) {
          if ($$eq(newBucket[i].key, key)) {
            newBucket[i] = { key: key, value: value };
            return collision(hash, newBucket);
          }
        }
        newBucket.push({ key: key, value: value });
        return collision(hash, newBucket);
      }
      // Wrap collision into an internal node, then insert
      var frag1 = (node.hash >>> shift) & MASK;
      var bit1 = 1 << frag1;
      var newNode = internal(bit1, [node], node.size);
      return insert(newNode, hash, key, value, shift);
    }

    // Internal node
    var frag = (hash >>> shift) & MASK;
    var bit = 1 << frag;
    var idx = getIndex(node.bitmap, bit);

    if ((node.bitmap & bit) === 0) {
      // New slot
      var newChildren = node.children.slice();
      newChildren.splice(idx, 0, leaf(hash, key, value));
      return internal(node.bitmap | bit, newChildren, node.size + 1);
    }

    // Existing slot — recurse
    var child = node.children[idx];
    var newChild = insert(child, hash, key, value, shift + BITS);
    if (newChild === child) return node;
    var newChildren = node.children.slice();
    newChildren[idx] = newChild;
    return internal(node.bitmap, newChildren, node.size - child.size + newChild.size);
  }

  function mergeLeaves(shift, a, b) {
    var fragA = (a.hash >>> shift) & MASK;
    var fragB = (b.hash >>> shift) & MASK;
    if (fragA === fragB) {
      var child = mergeLeaves(shift + BITS, a, b);
      var bit = 1 << fragA;
      return internal(bit, [child], a.size + b.size);
    }
    var bitA = 1 << fragA;
    var bitB = 1 << fragB;
    var bitmap = bitA | bitB;
    var children = fragA < fragB ? [a, b] : [b, a];
    return internal(bitmap, children, a.size + b.size);
  }

  // --- Remove ---
  function remove(node, hash, key, shift) {
    if (node === EMPTY) return EMPTY;

    if (isLeaf(node)) {
      return (node.hash === hash && $$eq(node.key, key)) ? EMPTY : node;
    }

    if (isCollision(node)) {
      if (node.hash !== hash) return node;
      var newBucket = node.bucket.filter(function(e) { return !$$eq(e.key, key); });
      if (newBucket.length === node.bucket.length) return node; // not found
      if (newBucket.length === 1) return leaf(hash, newBucket[0].key, newBucket[0].value);
      return collision(hash, newBucket);
    }

    // Internal node
    var frag = (hash >>> shift) & MASK;
    var bit = 1 << frag;
    if ((node.bitmap & bit) === 0) return node; // not found
    var idx = getIndex(node.bitmap, bit);
    var child = node.children[idx];
    var newChild = remove(child, hash, key, shift + BITS);
    if (newChild === child) return node; // unchanged

    if (newChild === EMPTY) {
      // Remove slot
      if (node.children.length === 1) return EMPTY;
      var newChildren = node.children.slice();
      newChildren.splice(idx, 1);
      var newInternal = internal(node.bitmap ^ bit, newChildren, node.size - child.size);
      // Collapse if only one child left and it's a leaf or collision
      if (newInternal.children.length === 1 && !isInternal(newInternal.children[0])) {
        return newInternal.children[0];
      }
      return newInternal;
    }

    var newChildren = node.children.slice();
    newChildren[idx] = newChild;
    return internal(node.bitmap, newChildren, node.size - child.size + newChild.size);
  }

  // --- Traversal ---
  function collect(node, fn) {
    if (node === EMPTY) return;
    if (isLeaf(node)) { fn(node.key, node.value); return; }
    if (isCollision(node)) {
      for (var i = 0; i < node.bucket.length; i++) fn(node.bucket[i].key, node.bucket[i].value);
      return;
    }
    for (var i = 0; i < node.children.length; i++) collect(node.children[i], fn);
  }

  function keys(node) {
    var result = [];
    collect(node, function(k) { result.push(k); });
    return result;
  }

  function values(node) {
    var result = [];
    collect(node, function(k, v) { result.push(v); });
    return result;
  }

  function entries(node) {
    var result = [];
    collect(node, function(k, v) { result.push([k, v]); });
    return result;
  }

  return {
    empty: function() { return EMPTY; },
    get: function(d, k) {
      var v = get(d, $$hash(k), k, 0);
      return v === undefined ? { $tag: "None" } : { $tag: "Some", $0: v };
    },
    insert: function(d, k, v) { return insert(d, $$hash(k), k, v, 0); },
    remove: function(d, k) { return remove(d, $$hash(k), k, 0); },
    keys: keys,
    values: values,
    len: function(d) { return d.size; },
    has: function(d, k) { return get(d, $$hash(k), k, 0) !== undefined; },
    from: function(pairs) { var d = EMPTY; for (var i = 0; i < pairs.length; i++) d = insert(d, $$hash(pairs[i][0]), pairs[i][0], pairs[i][1], 0); return d; },
    entries: entries
  };
})();
