import { $$throw } from './error';
import type { DictValue } from './hamt';
import { $$Dict } from './hamt';
import type { Json } from './json';
import { $$json_to_zoya, $$zoya_to_json, type ZoyaJson } from './json';
import type { SetValue } from './set';
import { $$Set } from './set';
import type { TaskValue } from './task';
import { $$Task } from './task';

// JSValue — serde externally-tagged format crossing the JS<->Rust boundary
export type JSValue =
  | { Int: number }
  | { BigInt: number }
  | { Float: number }
  | { Bool: boolean }
  | { String: string }
  | { Array: JSValue[] }
  | { Object: Record<string, JSValue> }
  | { Bytes: number[] }
  | { Json: Json };

// ZoyaValue — internal Zoya representation
export type ZoyaValue =
  | boolean
  | number
  | bigint
  | string
  | ZoyaValue[]
  | Uint8Array
  | SetValue
  | DictValue
  | TaskValue
  | ((...args: ZoyaValue[]) => ZoyaValue)
  | { [key: string]: ZoyaValue };

// Value — serde-compatible representation matching Rust's externally-tagged enum format
type ValueData =
  | 'Unit'
  | { Tuple: Value[] }
  | { Struct: Record<string, Value> };

export type Value =
  | { Int: number }
  | { BigInt: number }
  | { Float: number }
  | { Bool: boolean }
  | { String: string }
  | { List: Value[] }
  | { Tuple: Value[] }
  | { Set: Value[] }
  | { Dict: [Value, Value][] }
  | { Struct: { name: string; module: string; data: ValueData } }
  | {
      EnumVariant: {
        enum_name: string;
        variant_name: string;
        module: string;
        data: ValueData;
      };
    }
  | { Task: Value }
  | { Bytes: number[] }
  | { Json: Json };

function valueDataToZoya(data: ValueData): Record<string, ZoyaValue> {
  if (data === 'Unit') return {};
  if ('Tuple' in data) {
    const out: Record<string, ZoyaValue> = {};
    for (let i = 0; i < data.Tuple.length; i++)
      out[`$${i}`] = $$value_to_zoya(data.Tuple[i]);
    return out;
  }
  const out: Record<string, ZoyaValue> = {};
  const keys = Object.keys(data.Struct);
  for (let i = 0; i < keys.length; i++)
    out[keys[i]] = $$value_to_zoya(data.Struct[keys[i]]);
  return out;
}

export function $$value_to_zoya(v: Value): ZoyaValue {
  if ('Int' in v) return v.Int;
  if ('BigInt' in v) return globalThis.BigInt(v.BigInt);
  if ('Float' in v) return v.Float;
  if ('Bool' in v) return v.Bool;
  if ('String' in v) return v.String;
  if ('List' in v) return v.List.map($$value_to_zoya);
  if ('Tuple' in v) return v.Tuple.map($$value_to_zoya);
  if ('Set' in v) return $$Set.from(v.Set.map($$value_to_zoya));
  if ('Dict' in v)
    return $$Dict.from(
      v.Dict.map(([k, val]) => [$$value_to_zoya(k), $$value_to_zoya(val)]),
    );
  if ('Struct' in v) {
    const obj: Record<string, ZoyaValue> = { $tag: v.Struct.name };
    Object.assign(obj, valueDataToZoya(v.Struct.data));
    return obj;
  }
  if ('EnumVariant' in v) {
    const obj: Record<string, ZoyaValue> = { $tag: v.EnumVariant.variant_name };
    Object.assign(obj, valueDataToZoya(v.EnumVariant.data));
    return obj;
  }
  if ('Task' in v) return $$Task.of($$value_to_zoya(v.Task));
  if ('Bytes' in v) return new Uint8Array(v.Bytes);
  if ('Json' in v) return $$json_to_zoya(v.Json) as ZoyaValue;
  $$throw('PANIC', `unexpected value in $$value_to_zoya: ${JSON.stringify(v)}`);
}

export async function $$zoya_to_js(v: ZoyaValue): Promise<JSValue> {
  if (v === null || v === undefined) {
    $$throw('PANIC', `unexpected ${v} in $$zoya_to_js`);
  }
  if (typeof v === 'function') {
    $$throw('PANIC', 'unexpected function in $$zoya_to_js');
  }
  if (typeof v === 'boolean') return { Bool: v };
  if (typeof v === 'number')
    return Number.isInteger(v) ? { Int: v } : { Float: v };
  if (typeof v === 'bigint') return { BigInt: Number(v) };
  if (typeof v === 'string') return { String: v };
  if (v instanceof Uint8Array) return { Bytes: Array.from(v) };
  if (Array.isArray(v)) {
    const items: JSValue[] = [];
    for (let i = 0; i < v.length; i++)
      items.push(await $$zoya_to_js(v[i] as ZoyaValue));
    return { Array: items };
  }
  if (typeof v === 'object') {
    const obj = v as Record<string, unknown>;
    // Task: execute .run(), await the promise
    if (obj.$task === true) {
      const run = obj.run as () => Promise<ZoyaValue>;
      const value = await run();
      return { Array: [await $$zoya_to_js(value)] };
    }
    // Set (HAMT-backed)
    if (obj.$$set === true) {
      const keys = $$Dict.keys(obj.$$data as DictValue);
      const items: JSValue[] = [];
      for (let i = 0; i < keys.length; i++)
        items.push(await $$zoya_to_js(keys[i] as ZoyaValue));
      return { Array: items };
    }
    // Dict (HAMT-backed)
    if (obj.$$hamt === true) {
      const entries = $$Dict.entries(v as DictValue);
      const items: JSValue[] = [];
      for (let i = 0; i < entries.length; i++) {
        items.push({
          Array: [
            await $$zoya_to_js(entries[i][0] as ZoyaValue),
            await $$zoya_to_js(entries[i][1] as ZoyaValue),
          ],
        });
      }
      return { Array: items };
    }
    // JSON enum variant — short-circuit with $$zoya_to_json
    if (obj.$json === true) {
      return {
        Json: $$zoya_to_json(v as ZoyaJson),
      };
    }
    // Plain object (struct/enum) — includes $tag as a regular field
    const out: Record<string, JSValue> = {};
    const keys = Object.keys(obj);
    for (let i = 0; i < keys.length; i++)
      out[keys[i]] = await $$zoya_to_js(obj[keys[i]] as ZoyaValue);
    return { Object: out };
  }
  $$throw('PANIC', `unexpected value in $$zoya_to_js: ${typeof v}`);
}

const $$jobs: ZoyaValue[] = [];

export function $$enqueue(job: ZoyaValue): ZoyaValue[] {
  $$jobs.push(job);
  return []; // unit
}

export async function $$run(
  qualified_path: string,
  ...args: Value[]
): Promise<{ value: JSValue; jobs: JSValue[] }> {
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
  const zoya_args = args.map($$value_to_zoya);
  const result = fn(...zoya_args);
  const collected = $$jobs.splice(0);
  return {
    value: await $$zoya_to_js(result),
    jobs: await Promise.all(collected.map($$zoya_to_js)),
  };
}
