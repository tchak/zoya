import { describe, it, expect } from 'vitest';
import { $$BigInt } from '../src/bigint';

describe('$$BigInt', () => {
  it('abs', () => {
    expect($$BigInt.abs(-5n)).toBe(5n);
    expect($$BigInt.abs(5n)).toBe(5n);
    expect($$BigInt.abs(0n)).toBe(0n);
  });

  it('to_string', () => {
    expect($$BigInt.to_string(42n)).toBe('42');
  });

  it('min/max', () => {
    expect($$BigInt.min(3n, 7n)).toBe(3n);
    expect($$BigInt.max(3n, 7n)).toBe(7n);
  });

  it('clamp', () => {
    expect($$BigInt.clamp(5n, 0n, 10n)).toBe(5n);
    expect($$BigInt.clamp(-5n, 0n, 10n)).toBe(0n);
    expect($$BigInt.clamp(15n, 0n, 10n)).toBe(10n);
  });

  it('signum', () => {
    expect($$BigInt.signum(5n)).toBe(1n);
    expect($$BigInt.signum(-5n)).toBe(-1n);
    expect($$BigInt.signum(0n)).toBe(0n);
  });

  it('is_positive/is_negative/is_zero', () => {
    expect($$BigInt.is_positive(5n)).toBe(true);
    expect($$BigInt.is_negative(-5n)).toBe(true);
    expect($$BigInt.is_zero(0n)).toBe(true);
  });

  it('to_int', () => {
    expect($$BigInt.to_int(42n)).toBe(42);
  });
});
