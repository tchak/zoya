import { describe, it, expect } from 'vitest';
import '../src/index';
import type { Value } from '../src/zoya';
import { $$value_to_zoya } from '../src/zoya';
import { $$Set } from '../src/set';
import { $$Dict } from '../src/hamt';

describe('$$value_to_zoya', () => {
  it('converts Int', () => {
    expect($$value_to_zoya({ Int: 42 })).toBe(42);
  });

  it('converts BigInt', () => {
    expect($$value_to_zoya({ BigInt: 99 })).toBe(99n);
  });

  it('converts Float', () => {
    expect($$value_to_zoya({ Float: 3.14 })).toBe(3.14);
  });

  it('converts Bool', () => {
    expect($$value_to_zoya({ Bool: true })).toBe(true);
    expect($$value_to_zoya({ Bool: false })).toBe(false);
  });

  it('converts String', () => {
    expect($$value_to_zoya({ String: 'hello' })).toBe('hello');
  });

  it('converts List', () => {
    const v: Value = { List: [{ Int: 1 }, { Int: 2 }, { Int: 3 }] };
    expect($$value_to_zoya(v)).toEqual([1, 2, 3]);
  });

  it('converts Tuple', () => {
    const v: Value = { Tuple: [{ Int: 1 }, { String: 'a' }] };
    expect($$value_to_zoya(v)).toEqual([1, 'a']);
  });

  it('converts Set', () => {
    const v: Value = { Set: [{ Int: 1 }, { Int: 2 }] };
    const result = $$value_to_zoya(v);
    expect($$Set.contains(result as ReturnType<typeof $$Set.empty>, 1)).toBe(
      true,
    );
    expect($$Set.contains(result as ReturnType<typeof $$Set.empty>, 2)).toBe(
      true,
    );
    expect($$Set.len(result as ReturnType<typeof $$Set.empty>)).toBe(2);
  });

  it('converts Dict', () => {
    const v: Value = {
      Dict: [
        [{ Int: 1 }, { String: 'a' }],
        [{ Int: 2 }, { String: 'b' }],
      ],
    };
    const result = $$value_to_zoya(v) as ReturnType<typeof $$Dict.empty>;
    expect($$Dict.len(result)).toBe(2);
    expect($$Dict.get(result, 1)).toEqual({ $tag: 'Some', $0: 'a' });
    expect($$Dict.get(result, 2)).toEqual({ $tag: 'Some', $0: 'b' });
  });

  it('converts Struct with Unit data', () => {
    const v: Value = {
      Struct: { name: 'Point', module: 'root::geo', data: 'Unit' },
    };
    expect($$value_to_zoya(v)).toEqual({ $tag: 'Point' });
  });

  it('converts Struct with Struct data', () => {
    const v: Value = {
      Struct: {
        name: 'Point',
        module: 'root::geo',
        data: { Struct: { x: { Int: 10 }, y: { Int: 20 } } },
      },
    };
    expect($$value_to_zoya(v)).toEqual({ $tag: 'Point', x: 10, y: 20 });
  });

  it('converts Struct with Tuple data', () => {
    const v: Value = {
      Struct: {
        name: 'Wrapper',
        module: 'root::types',
        data: { Tuple: [{ Int: 42 }] },
      },
    };
    expect($$value_to_zoya(v)).toEqual({ $tag: 'Wrapper', $0: 42 });
  });

  it('converts EnumVariant with Unit data', () => {
    const v: Value = {
      EnumVariant: {
        enum_name: 'Option',
        variant_name: 'None',
        module: 'root::std',
        data: 'Unit',
      },
    };
    expect($$value_to_zoya(v)).toEqual({ $tag: 'None' });
  });

  it('converts EnumVariant with Tuple data', () => {
    const v: Value = {
      EnumVariant: {
        enum_name: 'Option',
        variant_name: 'Some',
        module: 'root::std',
        data: { Tuple: [{ Int: 42 }] },
      },
    };
    expect($$value_to_zoya(v)).toEqual({ $tag: 'Some', $0: 42 });
  });

  it('converts EnumVariant with Struct data', () => {
    const v: Value = {
      EnumVariant: {
        enum_name: 'Message',
        variant_name: 'Move',
        module: 'root::msg',
        data: { Struct: { x: { Int: 5 }, y: { Int: 10 } } },
      },
    };
    expect($$value_to_zoya(v)).toEqual({ $tag: 'Move', x: 5, y: 10 });
  });

  it('converts Task', () => {
    const v: Value = { Task: { Int: 99 } };
    const result = $$value_to_zoya(v) as {
      $task: true;
      run: () => Promise<unknown>;
    };
    expect(result.$task).toBe(true);
    return result.run().then((val) => expect(val).toBe(99));
  });

  it('converts Bytes', () => {
    const v: Value = { Bytes: [1, 2, 3, 255] };
    const result = $$value_to_zoya(v);
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result).toEqual(new Uint8Array([1, 2, 3, 255]));
  });

  it('converts nested values', () => {
    const v: Value = {
      List: [
        {
          Struct: {
            name: 'Point',
            module: 'root',
            data: { Struct: { x: { Int: 1 }, y: { Int: 2 } } },
          },
        },
        {
          Struct: {
            name: 'Point',
            module: 'root',
            data: { Struct: { x: { Int: 3 }, y: { Int: 4 } } },
          },
        },
      ],
    };
    expect($$value_to_zoya(v)).toEqual([
      { $tag: 'Point', x: 1, y: 2 },
      { $tag: 'Point', x: 3, y: 4 },
    ]);
  });
});
