import { describe, it, expect } from 'vitest';
import { $$Int } from '../src/int';

describe('$$Int', () => {
  it('abs', () => {
    expect($$Int.abs(-5)).toBe(5);
    expect($$Int.abs(5)).toBe(5);
    expect($$Int.abs(0)).toBe(0);
  });

  it('to_string', () => {
    expect($$Int.to_string(42)).toBe("42");
    expect($$Int.to_string(-1)).toBe("-1");
  });

  it('to_float', () => {
    expect($$Int.to_float(5)).toBe(5);
  });

  it('min/max', () => {
    expect($$Int.min(3, 7)).toBe(3);
    expect($$Int.max(3, 7)).toBe(7);
  });

  it('clamp', () => {
    expect($$Int.clamp(5, 0, 10)).toBe(5);
    expect($$Int.clamp(-5, 0, 10)).toBe(0);
    expect($$Int.clamp(15, 0, 10)).toBe(10);
  });

  it('signum', () => {
    expect($$Int.signum(5)).toBe(1);
    expect($$Int.signum(-5)).toBe(-1);
    expect($$Int.signum(0)).toBe(0);
  });

  it('is_positive/is_negative/is_zero', () => {
    expect($$Int.is_positive(5)).toBe(true);
    expect($$Int.is_positive(-5)).toBe(false);
    expect($$Int.is_negative(-5)).toBe(true);
    expect($$Int.is_zero(0)).toBe(true);
    expect($$Int.is_zero(1)).toBe(false);
  });

  it('to_bigint', () => {
    expect($$Int.to_bigint(42)).toBe(42n);
  });
});
