import { describe, it, expect } from 'vitest';
import { $$Dict } from '../src/hamt';

describe('$$Dict', () => {
  it('creates an empty dict', () => {
    const d = $$Dict.empty();
    expect($$Dict.len(d)).toBe(0);
  });

  it('inserts and retrieves values', () => {
    let d = $$Dict.empty();
    d = $$Dict.insert(d, "a", 1);
    d = $$Dict.insert(d, "b", 2);
    expect($$Dict.get(d, "a")).toEqual({ $tag: "Some", $0: 1 });
    expect($$Dict.get(d, "b")).toEqual({ $tag: "Some", $0: 2 });
    expect($$Dict.get(d, "c")).toEqual({ $tag: "None" });
  });

  it('overwrites existing keys', () => {
    let d = $$Dict.empty();
    d = $$Dict.insert(d, "a", 1);
    d = $$Dict.insert(d, "a", 99);
    expect($$Dict.get(d, "a")).toEqual({ $tag: "Some", $0: 99 });
    expect($$Dict.len(d)).toBe(1);
  });

  it('removes values', () => {
    let d = $$Dict.empty();
    d = $$Dict.insert(d, "a", 1);
    d = $$Dict.insert(d, "b", 2);
    d = $$Dict.remove(d, "a");
    expect($$Dict.get(d, "a")).toEqual({ $tag: "None" });
    expect($$Dict.get(d, "b")).toEqual({ $tag: "Some", $0: 2 });
    expect($$Dict.len(d)).toBe(1);
  });

  it('is persistent (insert does not mutate)', () => {
    let d1 = $$Dict.empty();
    d1 = $$Dict.insert(d1, "a", 1);
    const d2 = $$Dict.insert(d1, "b", 2);
    expect($$Dict.len(d1)).toBe(1);
    expect($$Dict.len(d2)).toBe(2);
    expect($$Dict.has(d1, "b")).toBe(false);
  });

  it('tracks size correctly', () => {
    let d = $$Dict.empty();
    for (let i = 0; i < 100; i++) {
      d = $$Dict.insert(d, `key${i}`, i);
    }
    expect($$Dict.len(d)).toBe(100);
  });

  it('handles has correctly', () => {
    let d = $$Dict.empty();
    d = $$Dict.insert(d, "x", 42);
    expect($$Dict.has(d, "x")).toBe(true);
    expect($$Dict.has(d, "y")).toBe(false);
  });

  it('returns keys', () => {
    const d = $$Dict.from([["a", 1], ["b", 2], ["c", 3]]);
    const k = $$Dict.keys(d).sort();
    expect(k).toEqual(["a", "b", "c"]);
  });

  it('returns values', () => {
    const d = $$Dict.from([["a", 1], ["b", 2]]);
    const v = ($$Dict.values(d) as number[]).sort();
    expect(v).toEqual([1, 2]);
  });

  it('returns entries', () => {
    const d = $$Dict.from([["a", 1], ["b", 2]]);
    const e = $$Dict.entries(d).sort((a, b) => (a[0] as string).localeCompare(b[0] as string));
    expect(e).toEqual([["a", 1], ["b", 2]]);
  });

  it('handles integer keys', () => {
    let d = $$Dict.empty();
    d = $$Dict.insert(d, 1, "one");
    d = $$Dict.insert(d, 2, "two");
    expect($$Dict.get(d, 1)).toEqual({ $tag: "Some", $0: "one" });
    expect($$Dict.get(d, 3)).toEqual({ $tag: "None" });
  });

  it('handles hash collisions gracefully', () => {
    let d = $$Dict.empty();
    // Insert many keys to force collisions
    for (let i = 0; i < 1000; i++) {
      d = $$Dict.insert(d, i, i * 10);
    }
    expect($$Dict.len(d)).toBe(1000);
    for (let i = 0; i < 1000; i++) {
      expect($$Dict.get(d, i)).toEqual({ $tag: "Some", $0: i * 10 });
    }
  });

  it('creates from pairs', () => {
    const d = $$Dict.from([["x", 10], ["y", 20]]);
    expect($$Dict.len(d)).toBe(2);
    expect($$Dict.get(d, "x")).toEqual({ $tag: "Some", $0: 10 });
  });

  it('removes non-existent key returns same dict', () => {
    let d = $$Dict.from([["a", 1]]);
    const d2 = $$Dict.remove(d, "b");
    expect(d2).toBe(d);
  });
});
