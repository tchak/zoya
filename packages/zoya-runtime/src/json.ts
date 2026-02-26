import { $$Dict } from './hamt';

export type Json =
  | null
  | boolean
  | number
  | string
  | Json[]
  | { [key: string]: Json };

export type ZoyaJson =
  | { $tag: 'Null'; $json: true }
  | { $tag: 'Bool'; $0: boolean; $json: true }
  | {
      $tag: 'Number';
      $0: { $tag: 'Int'; $0: number } | { $tag: 'Float'; $0: number };
      $json: true;
    }
  | { $tag: 'String'; $0: string; $json: true }
  | { $tag: 'Array'; $0: ZoyaJson[]; $json: true }
  | { $tag: 'Object'; $0: unknown; $json: true };

export function $$json_to_zoya(v: Json): ZoyaJson {
  if (v === null) return { $tag: 'Null', $json: true };
  if (typeof v === 'boolean') return { $tag: 'Bool', $0: v, $json: true };
  if (typeof v === 'number')
    return Number.isInteger(v)
      ? { $tag: 'Number', $0: { $tag: 'Int', $0: v }, $json: true }
      : { $tag: 'Number', $0: { $tag: 'Float', $0: v }, $json: true };
  if (typeof v === 'string') return { $tag: 'String', $0: v, $json: true };
  if (Array.isArray(v))
    return { $tag: 'Array', $0: v.map($$json_to_zoya), $json: true };
  return {
    $tag: 'Object',
    $0: $$Dict.from(
      Object.entries(v as Record<string, Json>).map(
        ([k, val]) => [k, $$json_to_zoya(val)] as [string, ZoyaJson],
      ),
    ),
    $json: true,
  };
}

export function $$zoya_to_json(v: ZoyaJson): Json {
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
      ) as Json;
  }
}
