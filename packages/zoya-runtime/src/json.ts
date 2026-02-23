import { $$Dict } from './hamt';
import { $$Task } from './task';

type ZoyaJson =
  | { $tag: 'Null' }
  | { $tag: 'Bool'; $0: boolean }
  | {
      $tag: 'Number';
      $0: { $tag: 'Int'; $0: number } | { $tag: 'Float'; $0: number };
    }
  | { $tag: 'String'; $0: string }
  | { $tag: 'Array'; $0: ZoyaJson[] }
  | { $tag: 'Object'; $0: unknown };

export function $$json_to_zoya(v: unknown): ZoyaJson {
  if (v === null) return { $tag: 'Null' };
  if (typeof v === 'boolean') return { $tag: 'Bool', $0: v };
  if (typeof v === 'number')
    return Number.isInteger(v)
      ? { $tag: 'Number', $0: { $tag: 'Int', $0: v } }
      : { $tag: 'Number', $0: { $tag: 'Float', $0: v } };
  if (typeof v === 'string') return { $tag: 'String', $0: v };
  if (Array.isArray(v)) return { $tag: 'Array', $0: v.map($$json_to_zoya) };
  return {
    $tag: 'Object',
    $0: $$Dict.from(
      Object.entries(v as Record<string, unknown>).map(
        ([k, val]) => [k, $$json_to_zoya(val)] as [unknown, unknown],
      ),
    ),
  };
}

export function $$zoya_to_json(v: ZoyaJson): unknown {
  switch (v.$tag) {
    case 'Null':
      return null;
    case 'Bool':
      return v.$0;
    case 'Number':
      return v.$0.$0;
    case 'String':
      return v.$0;
    case 'Array':
      return v.$0.map($$zoya_to_json);
    case 'Object':
      return Object.fromEntries(
        $$Dict
          .entries(v.$0 as ReturnType<typeof $$Dict.empty>)
          .map(([k, val]) => [k, $$zoya_to_json(val as ZoyaJson)]),
      );
  }
}

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
    if (tagged.$tag === 'Set') return $$Set_from(v.map($$js_to_zoya));
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

// Inline $$Set.from to avoid circular import (set -> hamt -> equality -> hamt)
function $$Set_from(items: unknown[]): {
  $$set: true;
  $$data: ReturnType<typeof $$Dict.empty>;
} {
  let d = $$Dict.empty();
  for (let i = 0; i < items.length; i++) {
    d = $$Dict.insert(d, items[i], true);
  }
  return Object.freeze({ $$set: true as const, $$data: d });
}
