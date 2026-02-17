export const $$Int = {
  abs(x: number): number { return Math.abs(x); },
  to_string(x: number): string { return String(x); },
  to_float(x: number): number { return x; },
  min(x: number, y: number): number { return Math.min(x, y); },
  max(x: number, y: number): number { return Math.max(x, y); },
  pow(x: number, y: number): number { return Math.pow(x, y); },
  clamp(x: number, min: number, max: number): number { return Math.min(Math.max(x, min), max); },
  signum(x: number): number { return Math.sign(x); },
  is_positive(x: number): boolean { return x > 0; },
  is_negative(x: number): boolean { return x < 0; },
  is_zero(x: number): boolean { return x === 0; },
  to_bigint(x: number): bigint { return BigInt(x); },
};
