import { describe, it, expect } from 'vitest';
import { $$List } from '../src/list';

describe('$$List', () => {
  it('len', () => {
    expect($$List.len([1, 2, 3])).toBe(3);
    expect($$List.len([])).toBe(0);
  });

  it('reverse', () => {
    expect($$List.reverse([1, 2, 3])).toEqual([3, 2, 1]);
  });

  it('push', () => {
    expect($$List.push([1, 2], 3)).toEqual([1, 2, 3]);
  });

  it('push does not mutate original', () => {
    const xs = [1, 2];
    $$List.push(xs, 3);
    expect(xs).toEqual([1, 2]);
  });

  it('map', () => {
    expect($$List.map([1, 2, 3], (x) => (x as number) * 2)).toEqual([2, 4, 6]);
  });

  it('filter', () => {
    expect($$List.filter([1, 2, 3, 4], (x) => (x as number) % 2 === 0)).toEqual([2, 4]);
  });

  it('fold', () => {
    expect($$List.fold([1, 2, 3], 0, (acc, x) => (acc as number) + (x as number))).toBe(6);
  });

  it('filter_map', () => {
    const result = $$List.filter_map(
      [1, 2, 3, 4],
      (x) => (x as number) % 2 === 0 ? { $tag: "Some", $0: (x as number) * 10 } : { $tag: "None" }
    );
    expect(result).toEqual([20, 40]);
  });

  it('truncate', () => {
    expect($$List.truncate([1, 2, 3, 4, 5], 3)).toEqual([1, 2, 3]);
  });

  it('insert', () => {
    expect($$List.insert([1, 2, 3], 1, 99)).toEqual([1, 99, 2, 3]);
  });

  it('remove', () => {
    expect($$List.remove([1, 2, 3], 1)).toEqual([1, 3]);
  });
});
