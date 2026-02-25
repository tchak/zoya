import { $$Dict } from './hamt';
import { $$Set } from './set';
import { $$Task } from './task';
import { $$throw } from './error';
import type { DictValue } from './hamt';
import type { SetValue } from './set';
import type { TaskValue } from './task';

// JSValue — external representation crossing the JS<->Rust boundary
export type JSValue =
  | boolean
  | number
  | bigint
  | string
  | JSValueArray
  | JSValueObject;
interface JSValueArray extends Array<JSValue> {
  $tag?: string;
}
interface JSValueObject {
  $tag?: string;
  [key: string]: JSValue | undefined;
}

// Value — internal Zoya representation
export type Value =
  | boolean
  | number
  | bigint
  | string
  | Value[]
  | SetValue
  | DictValue
  | TaskValue
  | ((...args: Value[]) => Value)
  | { [key: string]: Value };

export async function $$zoya_to_js(v: unknown): Promise<unknown> {
  if (
    v === null ||
    v === undefined ||
    typeof v === 'boolean' ||
    typeof v === 'number' ||
    typeof v === 'string' ||
    typeof v === 'bigint' ||
    typeof v === 'function'
  )
    return v;
  if (Array.isArray(v)) {
    const result = [];
    for (let i = 0; i < v.length; i++) result.push(await $$zoya_to_js(v[i]));
    // Preserve existing $tag (Set, Dict)
    const tagged = v as unknown as Record<string, unknown>;
    if (tagged.$tag)
      (result as unknown as Record<string, unknown>).$tag = tagged.$tag;
    return result;
  }
  if (typeof v === 'object') {
    const obj = v as Record<string, unknown>;
    // Task: execute .run(), await the promise, tag result as 'Task'
    if (obj.$task === true) {
      const run = obj.run as () => Promise<unknown>;
      const value = await run();
      const arr = [await $$zoya_to_js(value)];
      (arr as unknown as Record<string, unknown>).$tag = 'Task';
      return arr;
    }
    // Set (HAMT-backed)
    if (obj.$$set === true) {
      const keys = $$Dict.keys(obj.$$data as ReturnType<typeof $$Dict.empty>);
      const result = [];
      for (let i = 0; i < keys.length; i++)
        result.push(await $$zoya_to_js(keys[i]));
      (result as unknown as Record<string, unknown>).$tag = 'Set';
      return result;
    }
    // Dict (HAMT-backed)
    if (obj.$$hamt === true) {
      const entries = $$Dict.entries(v as ReturnType<typeof $$Dict.empty>);
      const result = [];
      for (let i = 0; i < entries.length; i++) {
        result.push([
          await $$zoya_to_js(entries[i][0]),
          await $$zoya_to_js(entries[i][1]),
        ]);
      }
      (result as unknown as Record<string, unknown>).$tag = 'Dict';
      return result;
    }
    // Plain object
    const out: Record<string, unknown> = {};
    const keys = Object.keys(obj);
    for (let i = 0; i < keys.length; i++)
      out[keys[i]] = await $$zoya_to_js(obj[keys[i]]);
    return out;
  }
  return v;
}

export function $$js_to_zoya(v: unknown): unknown {
  if (
    v === null ||
    v === undefined ||
    typeof v === 'boolean' ||
    typeof v === 'number' ||
    typeof v === 'string' ||
    typeof v === 'bigint'
  )
    return v;
  if (Array.isArray(v)) {
    const tagged = v as unknown as Record<string, unknown>;
    if (tagged.$tag === 'Task') return $$Task.of($$js_to_zoya(v[0]));
    if (tagged.$tag === 'Set') return $$Set.from(v.map($$js_to_zoya));
    if (tagged.$tag === 'Dict')
      return $$Dict.from(
        v.map((e: unknown) => {
          const pair = e as [unknown, unknown];
          return [$$js_to_zoya(pair[0]), $$js_to_zoya(pair[1])];
        }),
      );
    return v.map($$js_to_zoya);
  }
  if (typeof v === 'object') {
    const obj = v as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    const keys = Object.keys(obj);
    for (let i = 0; i < keys.length; i++)
      out[keys[i]] = $$js_to_zoya(obj[keys[i]]);
    return out;
  }
  return v;
}

export async function $$run(
  qualified_path: string,
  ...args: unknown[]
): Promise<unknown> {
  const js_name = '$' + qualified_path.replace(/::/g, '$');
  const fn = (globalThis as Record<string, unknown>)[js_name];
  if (typeof fn !== 'function') {
    $$throw('PANIC', `function not found: ${qualified_path}`);
  }
  if (fn.length !== args.length) {
    $$throw(
      'PANIC',
      `arity mismatch for ${qualified_path}: expected ${fn.length} arguments, got ${args.length}`,
    );
  }
  const zoya_args = args.map($$js_to_zoya);
  const result = fn(...zoya_args);
  return $$zoya_to_js(result);
}
