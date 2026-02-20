import { describe, it, expect } from 'vitest';
import { $$String } from '../src/string';

describe('$$String', () => {
  it('len', () => {
    expect($$String.len('hello')).toBe(5);
    expect($$String.len('')).toBe(0);
  });

  it('contains', () => {
    expect($$String.contains('hello world', 'world')).toBe(true);
    expect($$String.contains('hello', 'xyz')).toBe(false);
  });

  it('starts_with/ends_with', () => {
    expect($$String.starts_with('hello', 'hel')).toBe(true);
    expect($$String.ends_with('hello', 'llo')).toBe(true);
  });

  it('to_uppercase/to_lowercase', () => {
    expect($$String.to_uppercase('hello')).toBe('HELLO');
    expect($$String.to_lowercase('HELLO')).toBe('hello');
  });

  it('trim', () => {
    expect($$String.trim('  hi  ')).toBe('hi');
    expect($$String.trim_start('  hi  ')).toBe('hi  ');
    expect($$String.trim_end('  hi  ')).toBe('  hi');
  });

  it('replace', () => {
    expect($$String.replace('aabaa', 'a', 'x')).toBe('xxbxx');
  });

  it('replace_first', () => {
    expect($$String.replace_first('aabaa', 'a', 'x')).toBe('xabaa');
  });

  it('split', () => {
    expect($$String.split('a,b,c', ',')).toEqual(['a', 'b', 'c']);
  });

  it('chars', () => {
    expect($$String.chars('abc')).toEqual(['a', 'b', 'c']);
  });

  it('find', () => {
    expect($$String.find('hello', 'll')).toEqual({ $tag: 'Some', $0: 2 });
    expect($$String.find('hello', 'xyz')).toEqual({ $tag: 'None' });
  });

  it('slice', () => {
    expect($$String.slice('hello', 1, 4)).toBe('ell');
  });

  it('reverse', () => {
    expect($$String.reverse('hello')).toBe('olleh');
  });

  it('pad_start/pad_end', () => {
    expect($$String.pad_start('5', 3, '0')).toBe('005');
    expect($$String.pad_end('5', 3, '0')).toBe('500');
  });

  it('repeat', () => {
    expect($$String.repeat('ab', 3)).toBe('ababab');
  });

  it('to_int', () => {
    expect($$String.to_int('42')).toEqual({ $tag: 'Some', $0: 42 });
    expect($$String.to_int('abc')).toEqual({ $tag: 'None' });
  });

  it('to_float', () => {
    expect($$String.to_float('3.14')).toEqual({ $tag: 'Some', $0: 3.14 });
    expect($$String.to_float('xyz')).toEqual({ $tag: 'None' });
  });
});
