import { describe, it, expect } from 'vitest';
import { $$Set } from '../src/set';

describe('$$Set', () => {
  it('creates an empty set', () => {
    const s = $$Set.empty();
    expect($$Set.len(s)).toBe(0);
  });

  it('inserts and checks membership', () => {
    let s = $$Set.empty();
    s = $$Set.insert(s, 1);
    s = $$Set.insert(s, 2);
    expect($$Set.contains(s, 1)).toBe(true);
    expect($$Set.contains(s, 3)).toBe(false);
  });

  it('removes elements', () => {
    let s = $$Set.from([1, 2, 3]);
    s = $$Set.remove(s, 2);
    expect($$Set.contains(s, 2)).toBe(false);
    expect($$Set.len(s)).toBe(2);
  });

  it('converts to list', () => {
    const s = $$Set.from([3, 1, 2]);
    const list = ($$Set.to_list(s) as number[]).sort();
    expect(list).toEqual([1, 2, 3]);
  });

  it('handles duplicates', () => {
    const s = $$Set.from([1, 1, 2, 2, 3]);
    expect($$Set.len(s)).toBe(3);
  });

  it('computes union', () => {
    const a = $$Set.from([1, 2]);
    const b = $$Set.from([2, 3]);
    const u = $$Set.union(a, b);
    expect($$Set.len(u)).toBe(3);
    expect($$Set.contains(u, 1)).toBe(true);
    expect($$Set.contains(u, 3)).toBe(true);
  });

  it('computes intersection', () => {
    const a = $$Set.from([1, 2, 3]);
    const b = $$Set.from([2, 3, 4]);
    const i = $$Set.intersection(a, b);
    expect($$Set.len(i)).toBe(2);
    expect($$Set.contains(i, 2)).toBe(true);
    expect($$Set.contains(i, 3)).toBe(true);
  });

  it('computes difference', () => {
    const a = $$Set.from([1, 2, 3]);
    const b = $$Set.from([2, 3, 4]);
    const d = $$Set.difference(a, b);
    expect($$Set.len(d)).toBe(1);
    expect($$Set.contains(d, 1)).toBe(true);
  });

  it('checks disjoint', () => {
    expect($$Set.is_disjoint($$Set.from([1, 2]), $$Set.from([3, 4]))).toBe(true);
    expect($$Set.is_disjoint($$Set.from([1, 2]), $$Set.from([2, 3]))).toBe(false);
  });

  it('checks subset', () => {
    expect($$Set.is_subset($$Set.from([1, 2]), $$Set.from([1, 2, 3]))).toBe(true);
    expect($$Set.is_subset($$Set.from([1, 4]), $$Set.from([1, 2, 3]))).toBe(false);
  });

  it('checks superset', () => {
    expect($$Set.is_superset($$Set.from([1, 2, 3]), $$Set.from([1, 2]))).toBe(true);
    expect($$Set.is_superset($$Set.from([1, 2]), $$Set.from([1, 2, 3]))).toBe(false);
  });
});
