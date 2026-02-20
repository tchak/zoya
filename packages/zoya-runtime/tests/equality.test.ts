import { describe, it, expect } from 'vitest';
import { $$eq, $$is_obj } from '../src/equality';
import { $$Dict } from '../src/hamt';
import { $$Set } from '../src/set';

describe('$$is_obj', () => {
  it('returns true for plain objects', () => {
    expect($$is_obj({})).toBe(true);
    expect($$is_obj({ a: 1 })).toBe(true);
  });

  it('returns false for arrays', () => {
    expect($$is_obj([])).toBe(false);
    expect($$is_obj([1, 2])).toBe(false);
  });

  it('returns false for null', () => {
    expect($$is_obj(null)).toBe(false);
  });

  it('returns false for primitives', () => {
    expect($$is_obj(42)).toBe(false);
    expect($$is_obj('hello')).toBe(false);
    expect($$is_obj(true)).toBe(false);
  });
});

describe('$$eq', () => {
  it('compares primitives', () => {
    expect($$eq(1, 1)).toBe(true);
    expect($$eq(1, 2)).toBe(false);
    expect($$eq('a', 'a')).toBe(true);
    expect($$eq('a', 'b')).toBe(false);
    expect($$eq(true, true)).toBe(true);
    expect($$eq(true, false)).toBe(false);
  });

  it('compares arrays deeply', () => {
    expect($$eq([1, 2, 3], [1, 2, 3])).toBe(true);
    expect($$eq([1, 2], [1, 2, 3])).toBe(false);
    expect($$eq([1, [2, 3]], [1, [2, 3]])).toBe(true);
    expect($$eq([1, [2, 3]], [1, [2, 4]])).toBe(false);
  });

  it('compares empty arrays', () => {
    expect($$eq([], [])).toBe(true);
  });

  it('compares objects deeply', () => {
    expect($$eq({ a: 1, b: 2 }, { a: 1, b: 2 })).toBe(true);
    expect($$eq({ a: 1 }, { a: 2 })).toBe(false);
    expect($$eq({ a: 1 }, { b: 1 })).toBe(false);
  });

  it('compares nested objects', () => {
    expect($$eq({ a: { b: 1 } }, { a: { b: 1 } })).toBe(true);
    expect($$eq({ a: { b: 1 } }, { a: { b: 2 } })).toBe(false);
  });

  it('compares dicts', () => {
    const d1 = $$Dict.from([
      ['a', 1],
      ['b', 2],
    ]);
    const d2 = $$Dict.from([
      ['a', 1],
      ['b', 2],
    ]);
    const d3 = $$Dict.from([
      ['a', 1],
      ['b', 3],
    ]);
    expect($$eq(d1, d2)).toBe(true);
    expect($$eq(d1, d3)).toBe(false);
  });

  it('compares sets', () => {
    const s1 = $$Set.from([1, 2, 3]);
    const s2 = $$Set.from([1, 2, 3]);
    const s3 = $$Set.from([1, 2, 4]);
    expect($$eq(s1, s2)).toBe(true);
    expect($$eq(s1, s3)).toBe(false);
  });

  it('compares sets of different sizes', () => {
    const s1 = $$Set.from([1, 2]);
    const s2 = $$Set.from([1, 2, 3]);
    expect($$eq(s1, s2)).toBe(false);
  });
});
