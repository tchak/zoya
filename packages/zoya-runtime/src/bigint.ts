export const $$BigInt = {
  abs(x: bigint): bigint {
    return x < 0n ? -x : x;
  },
  to_string(x: bigint): string {
    return String(x);
  },
  min(x: bigint, y: bigint): bigint {
    return x < y ? x : y;
  },
  max(x: bigint, y: bigint): bigint {
    return x > y ? x : y;
  },
  pow(x: bigint, y: bigint): bigint {
    return x ** y;
  },
  clamp(x: bigint, min: bigint, max: bigint): bigint {
    return x < min ? min : x > max ? max : x;
  },
  signum(x: bigint): bigint {
    return x < 0n ? -1n : x > 0n ? 1n : 0n;
  },
  is_positive(x: bigint): boolean {
    return x > 0n;
  },
  is_negative(x: bigint): boolean {
    return x < 0n;
  },
  is_zero(x: bigint): boolean {
    return x === 0n;
  },
  to_int(x: bigint): number {
    return Number(x);
  },
};
