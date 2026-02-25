// Persistent HAMT (Hash Array Mapped Trie) for Dict<K, V>
// All nodes are plain frozen JS objects with $$hamt marker.

import { $$eq } from './equality';

// --- Types ---

interface LeafNode {
  readonly $$hamt: true;
  readonly hash: number;
  readonly key: unknown;
  readonly value: unknown;
  readonly size: 1;
}

interface CollisionNode {
  readonly $$hamt: true;
  readonly hash: number;
  readonly bucket: ReadonlyArray<{ key: unknown; value: unknown }>;
  readonly size: number;
}

interface InternalNode {
  readonly $$hamt: true;
  readonly bitmap: number;
  readonly children: ReadonlyArray<HamtNode>;
  readonly size: number;
}

type HamtNode = LeafNode | CollisionNode | InternalNode;

export type DictValue = HamtNode;

// --- Hashing ---

function $$hash(v: unknown): number {
  if (typeof v === 'number') return v | 0;
  if (typeof v === 'bigint') return Number(v & 0x7fffffffn) | 0;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (typeof v === 'string') {
    let h = 0;
    for (let i = 0; i < v.length; i++) {
      h = (Math.imul(31, h) + v.charCodeAt(i)) | 0;
    }
    return h;
  }
  if (Array.isArray(v)) {
    let h = 1;
    for (let i = 0; i < v.length; i++) {
      h = (Math.imul(31, h) + $$hash(v[i])) | 0;
    }
    return h;
  }
  if (typeof v === 'object' && v !== null) {
    let h = 1;
    const keys = Object.keys(v).sort();
    for (let i = 0; i < keys.length; i++) {
      h = (Math.imul(31, h) + $$hash(keys[i])) | 0;
      h =
        (Math.imul(31, h) + $$hash((v as Record<string, unknown>)[keys[i]])) |
        0;
    }
    return h;
  }
  return 0;
}

const BITS = 5;
const WIDTH = 1 << BITS; // 32
const MASK = WIDTH - 1;

function popcount(x: number): number {
  x = x - ((x >> 1) & 0x55555555);
  x = (x & 0x33333333) + ((x >> 2) & 0x33333333);
  return (((x + (x >> 4)) & 0x0f0f0f0f) * 0x01010101) >> 24;
}

// Empty HAMT node
const EMPTY: InternalNode = Object.freeze({
  $$hamt: true as const,
  bitmap: 0,
  children: Object.freeze([]) as ReadonlyArray<HamtNode>,
  size: 0,
});

function leaf(hash: number, key: unknown, value: unknown): LeafNode {
  return Object.freeze({
    $$hamt: true as const,
    hash,
    key,
    value,
    size: 1 as const,
  });
}

function collision(
  hash: number,
  bucket: Array<{ key: unknown; value: unknown }>,
): CollisionNode {
  return Object.freeze({
    $$hamt: true as const,
    hash,
    bucket: Object.freeze(bucket),
    size: bucket.length,
  });
}

function internal(
  bitmap: number,
  children: HamtNode[],
  size: number,
): InternalNode {
  return Object.freeze({
    $$hamt: true as const,
    bitmap,
    children: Object.freeze(children),
    size,
  });
}

function isLeaf(node: HamtNode): node is LeafNode {
  return 'key' in node;
}
function isCollision(node: HamtNode): node is CollisionNode {
  return 'bucket' in node;
}
function isInternal(node: HamtNode): node is InternalNode {
  return 'bitmap' in node && !('bucket' in node) && !('key' in node);
}

function getIndex(bitmap: number, bit: number): number {
  return popcount(bitmap & (bit - 1));
}

// --- Lookup ---

function get(
  node: HamtNode,
  hash: number,
  key: unknown,
  shift: number,
): unknown | undefined {
  if (node === EMPTY) return undefined;
  if (isLeaf(node)) {
    return node.hash === hash && $$eq(node.key, key) ? node.value : undefined;
  }
  if (isCollision(node)) {
    if (node.hash !== hash) return undefined;
    for (let i = 0; i < node.bucket.length; i++) {
      if ($$eq(node.bucket[i].key, key)) return node.bucket[i].value;
    }
    return undefined;
  }
  // Internal node
  const frag = (hash >>> shift) & MASK;
  const bit = 1 << frag;
  if ((node.bitmap & bit) === 0) return undefined;
  const idx = getIndex(node.bitmap, bit);
  return get(node.children[idx], hash, key, shift + BITS);
}

// --- Insert ---

function insert(
  node: HamtNode,
  hash: number,
  key: unknown,
  value: unknown,
  shift: number,
): HamtNode {
  if (node === EMPTY) return leaf(hash, key, value);

  if (isLeaf(node)) {
    if (node.hash === hash) {
      if ($$eq(node.key, key)) {
        return leaf(hash, key, value); // replace
      }
      // Hash collision
      return collision(hash, [
        { key: node.key, value: node.value },
        { key, value },
      ]);
    }
    // Different hashes
    return mergeLeaves(shift, node, leaf(hash, key, value));
  }

  if (isCollision(node)) {
    if (node.hash === hash) {
      const newBucket = node.bucket.slice();
      for (let i = 0; i < newBucket.length; i++) {
        if ($$eq(newBucket[i].key, key)) {
          newBucket[i] = { key, value };
          return collision(hash, newBucket);
        }
      }
      newBucket.push({ key, value });
      return collision(hash, newBucket);
    }
    // Wrap collision into an internal node, then insert
    const frag1 = (node.hash >>> shift) & MASK;
    const bit1 = 1 << frag1;
    const newNode = internal(bit1, [node], node.size);
    return insert(newNode, hash, key, value, shift);
  }

  // Internal node
  const frag = (hash >>> shift) & MASK;
  const bit = 1 << frag;
  const idx = getIndex(node.bitmap, bit);

  if ((node.bitmap & bit) === 0) {
    // New slot
    const newChildren = (node.children as HamtNode[]).slice();
    newChildren.splice(idx, 0, leaf(hash, key, value));
    return internal(node.bitmap | bit, newChildren, node.size + 1);
  }

  // Existing slot — recurse
  const child = node.children[idx];
  const newChild = insert(child, hash, key, value, shift + BITS);
  if (newChild === child) return node;
  const newChildren = (node.children as HamtNode[]).slice();
  newChildren[idx] = newChild;
  return internal(
    node.bitmap,
    newChildren,
    node.size - child.size + newChild.size,
  );
}

function mergeLeaves(shift: number, a: LeafNode, b: LeafNode): HamtNode {
  const fragA = (a.hash >>> shift) & MASK;
  const fragB = (b.hash >>> shift) & MASK;
  if (fragA === fragB) {
    const child = mergeLeaves(shift + BITS, a, b);
    const bit = 1 << fragA;
    return internal(bit, [child], a.size + b.size);
  }
  const bitA = 1 << fragA;
  const bitB = 1 << fragB;
  const bitmap = bitA | bitB;
  const children: HamtNode[] = fragA < fragB ? [a, b] : [b, a];
  return internal(bitmap, children, a.size + b.size);
}

// --- Remove ---

function remove(
  node: HamtNode,
  hash: number,
  key: unknown,
  shift: number,
): HamtNode {
  if (node === EMPTY) return EMPTY;

  if (isLeaf(node)) {
    return node.hash === hash && $$eq(node.key, key) ? EMPTY : node;
  }

  if (isCollision(node)) {
    if (node.hash !== hash) return node;
    const newBucket = node.bucket.filter((e) => !$$eq(e.key, key));
    if (newBucket.length === node.bucket.length) return node; // not found
    if (newBucket.length === 1)
      return leaf(hash, newBucket[0].key, newBucket[0].value);
    return collision(hash, newBucket);
  }

  // Internal node
  const frag = (hash >>> shift) & MASK;
  const bit = 1 << frag;
  if ((node.bitmap & bit) === 0) return node; // not found
  const idx = getIndex(node.bitmap, bit);
  const child = node.children[idx];
  const newChild = remove(child, hash, key, shift + BITS);
  if (newChild === child) return node; // unchanged

  if (newChild === EMPTY) {
    // Remove slot
    if (node.children.length === 1) return EMPTY;
    const newChildren = (node.children as HamtNode[]).slice();
    newChildren.splice(idx, 1);
    const newInternal = internal(
      node.bitmap ^ bit,
      newChildren,
      node.size - child.size,
    );
    // Collapse if only one child left and it's a leaf or collision
    if (
      newInternal.children.length === 1 &&
      !isInternal(newInternal.children[0])
    ) {
      return newInternal.children[0];
    }
    return newInternal;
  }

  const newChildren = (node.children as HamtNode[]).slice();
  newChildren[idx] = newChild;
  return internal(
    node.bitmap,
    newChildren,
    node.size - child.size + newChild.size,
  );
}

// --- Traversal ---

function collect(
  node: HamtNode,
  fn: (key: unknown, value: unknown) => void,
): void {
  if (node === EMPTY) return;
  if (isLeaf(node)) {
    fn(node.key, node.value);
    return;
  }
  if (isCollision(node)) {
    for (let i = 0; i < node.bucket.length; i++)
      fn(node.bucket[i].key, node.bucket[i].value);
    return;
  }
  for (let i = 0; i < node.children.length; i++) collect(node.children[i], fn);
}

function keys(node: HamtNode): unknown[] {
  const result: unknown[] = [];
  collect(node, (k) => {
    result.push(k);
  });
  return result;
}

function values(node: HamtNode): unknown[] {
  const result: unknown[] = [];
  collect(node, (_k, v) => {
    result.push(v);
  });
  return result;
}

function entries(node: HamtNode): [unknown, unknown][] {
  const result: [unknown, unknown][] = [];
  collect(node, (k, v) => {
    result.push([k, v]);
  });
  return result;
}

export const $$Dict = {
  empty(): HamtNode {
    return EMPTY;
  },
  get(
    d: HamtNode,
    k: unknown,
  ): { $tag: 'Some'; $0: unknown } | { $tag: 'None' } {
    const v = get(d, $$hash(k), k, 0);
    return v === undefined ? { $tag: 'None' } : { $tag: 'Some', $0: v };
  },
  insert(d: HamtNode, k: unknown, v: unknown): HamtNode {
    return insert(d, $$hash(k), k, v, 0);
  },
  remove(d: HamtNode, k: unknown): HamtNode {
    return remove(d, $$hash(k), k, 0);
  },
  keys,
  values,
  len(d: HamtNode): number {
    return d.size;
  },
  has(d: HamtNode, k: unknown): boolean {
    return get(d, $$hash(k), k, 0) !== undefined;
  },
  from(pairs: [unknown, unknown][]): HamtNode {
    let d: HamtNode = EMPTY;
    for (let i = 0; i < pairs.length; i++)
      d = insert(d, $$hash(pairs[i][0]), pairs[i][0], pairs[i][1], 0);
    return d;
  },
  entries,
};
