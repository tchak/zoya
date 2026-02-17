// Note: $$eq and $$Dict have a circular dependency.
// This works because the bundler inlines everything into a single scope,
// and both are function/object declarations with no top-level init ordering issues.
// For TypeScript module resolution, we import $$Dict directly.

import { $$Dict } from './hamt';

export function $$is_obj(x: unknown): x is Record<string, unknown> {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}

export function $$eq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!$$eq(a[i], b[i])) return false;
    }
    return true;
  }
  if ($$is_obj(a) && $$is_obj(b)) {
    if (a.$$set === true && b.$$set === true) {
      const aData = a.$$data as ReturnType<typeof $$Dict.empty>;
      const bData = b.$$data as ReturnType<typeof $$Dict.empty>;
      if ($$Dict.len(aData) !== $$Dict.len(bData)) return false;
      const ks = $$Dict.keys(aData);
      for (let j = 0; j < ks.length; j++) {
        if (!$$Dict.has(bData, ks[j])) return false;
      }
      return true;
    }
    if (a.$$hamt === true && b.$$hamt === true) {
      const aNode = a as unknown as ReturnType<typeof $$Dict.empty>;
      const bNode = b as unknown as ReturnType<typeof $$Dict.empty>;
      if ((a as { size: number }).size !== (b as { size: number }).size) return false;
      const ea = $$Dict.entries(aNode);
      for (let i = 0; i < ea.length; i++) {
        const v = $$Dict.get(bNode, ea[i][0]) as { $tag: string; $0?: unknown };
        if (v.$tag === "None" || !$$eq(ea[i][1], v.$0)) return false;
      }
      return true;
    }
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) {
      if (!$$eq(a[k], b[k])) return false;
    }
    return true;
  }
  return a === b;
}
