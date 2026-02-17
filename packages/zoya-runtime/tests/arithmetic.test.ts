import { describe, it, expect } from 'vitest';
import { $$div, $$div_bigint, $$mod, $$mod_bigint, $$pow, $$pow_bigint } from '../src/arithmetic';
import { $$ZoyaError } from '../src/error';

describe('$$div', () => {
  it('performs integer division', () => {
    expect($$div(10, 3)).toBe(3);
    expect($$div(7, 2)).toBe(3);
    expect($$div(-7, 2)).toBe(-3);
  });

  it('throws on division by zero', () => {
    expect(() => $$div(10, 0)).toThrow($$ZoyaError);
    expect(() => $$div(10, 0)).toThrow('division by zero');
  });
});

describe('$$div_bigint', () => {
  it('performs bigint division', () => {
    expect($$div_bigint(10n, 3n)).toBe(3n);
  });

  it('throws on division by zero', () => {
    expect(() => $$div_bigint(10n, 0n)).toThrow($$ZoyaError);
  });
});

describe('$$mod', () => {
  it('performs modulo', () => {
    expect($$mod(10, 3)).toBe(1);
    expect($$mod(7, 2)).toBe(1);
  });

  it('throws on modulo by zero', () => {
    expect(() => $$mod(10, 0)).toThrow($$ZoyaError);
    expect(() => $$mod(10, 0)).toThrow('modulo by zero');
  });
});

describe('$$mod_bigint', () => {
  it('performs bigint modulo', () => {
    expect($$mod_bigint(10n, 3n)).toBe(1n);
  });

  it('throws on modulo by zero', () => {
    expect(() => $$mod_bigint(10n, 0n)).toThrow($$ZoyaError);
  });
});

describe('$$pow', () => {
  it('computes power', () => {
    expect($$pow(2, 10)).toBe(1024);
    expect($$pow(3, 0)).toBe(1);
  });

  it('throws on negative exponent', () => {
    expect(() => $$pow(2, -1)).toThrow($$ZoyaError);
    expect(() => $$pow(2, -1)).toThrow('negative exponent');
  });
});

describe('$$pow_bigint', () => {
  it('computes bigint power', () => {
    expect($$pow_bigint(2n, 10n)).toBe(1024n);
  });

  it('throws on negative exponent', () => {
    expect(() => $$pow_bigint(2n, -1n)).toThrow($$ZoyaError);
  });
});
