import { $$throw } from './error';

export function $$div(a: number, b: number): number {
  if (b === 0) $$throw("PANIC", "division by zero");
  return Math.trunc(a / b);
}

export function $$div_bigint(a: bigint, b: bigint): bigint {
  if (b === 0n) $$throw("PANIC", "division by zero");
  return a / b;
}

export function $$mod(a: number, b: number): number {
  if (b === 0) $$throw("PANIC", "modulo by zero");
  return a % b;
}

export function $$mod_bigint(a: bigint, b: bigint): bigint {
  if (b === 0n) $$throw("PANIC", "modulo by zero");
  return a % b;
}

export function $$pow(a: number, b: number): number {
  if (b < 0) $$throw("PANIC", "negative exponent");
  return a ** b;
}

export function $$pow_bigint(a: bigint, b: bigint): bigint {
  if (b < 0n) $$throw("PANIC", "negative exponent");
  return a ** b;
}
