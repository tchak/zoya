import { describe, it, expect } from 'vitest';
import { $$Float } from '../src/float';

describe('$$Float', () => {
  it('abs', () => {
    expect($$Float.abs(-3.14)).toBeCloseTo(3.14);
    expect($$Float.abs(3.14)).toBeCloseTo(3.14);
  });

  it('to_string', () => {
    expect($$Float.to_string(3.14)).toBe('3.14');
  });

  it('to_int', () => {
    expect($$Float.to_int(3.7)).toBe(3);
    expect($$Float.to_int(-3.7)).toBe(-3);
  });

  it('floor/ceil/round', () => {
    expect($$Float.floor(3.7)).toBe(3);
    expect($$Float.ceil(3.2)).toBe(4);
    expect($$Float.round(3.5)).toBe(4);
  });

  it('sqrt', () => {
    expect($$Float.sqrt(9)).toBe(3);
    expect($$Float.sqrt(2)).toBeCloseTo(1.414);
  });

  it('min/max', () => {
    expect($$Float.min(1.5, 2.5)).toBe(1.5);
    expect($$Float.max(1.5, 2.5)).toBe(2.5);
  });

  it('clamp', () => {
    expect($$Float.clamp(5.0, 0.0, 10.0)).toBe(5.0);
    expect($$Float.clamp(-1.0, 0.0, 10.0)).toBe(0.0);
  });

  it('signum', () => {
    expect($$Float.signum(5.0)).toBe(1);
    expect($$Float.signum(-5.0)).toBe(-1);
    expect($$Float.signum(0.0)).toBe(0);
  });

  it('is_positive/is_negative/is_zero', () => {
    expect($$Float.is_positive(1.0)).toBe(true);
    expect($$Float.is_negative(-1.0)).toBe(true);
    expect($$Float.is_zero(0.0)).toBe(true);
  });
});
