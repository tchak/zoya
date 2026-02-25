import { $$Dict } from './hamt';

export interface SetValue {
  readonly $$set: true;
  readonly $$data: ReturnType<typeof $$Dict.empty>;
}

const SENTINEL = true;
const EMPTY: SetValue = Object.freeze({
  $$set: true as const,
  $$data: $$Dict.empty(),
});

function wrap(data: ReturnType<typeof $$Dict.empty>): SetValue {
  return Object.freeze({ $$set: true as const, $$data: data });
}

export const $$Set = {
  empty(): SetValue {
    return EMPTY;
  },
  contains(s: SetValue, v: unknown): boolean {
    return $$Dict.has(s.$$data, v);
  },
  insert(s: SetValue, v: unknown): SetValue {
    return wrap($$Dict.insert(s.$$data, v, SENTINEL));
  },
  remove(s: SetValue, v: unknown): SetValue {
    return wrap($$Dict.remove(s.$$data, v));
  },
  len(s: SetValue): number {
    return $$Dict.len(s.$$data);
  },
  to_list(s: SetValue): unknown[] {
    return $$Dict.keys(s.$$data);
  },
  is_disjoint(s: SetValue, o: SetValue): boolean {
    const ks = $$Dict.keys(s.$$data);
    for (let i = 0; i < ks.length; i++) {
      if ($$Dict.has(o.$$data, ks[i])) return false;
    }
    return true;
  },
  is_subset(s: SetValue, o: SetValue): boolean {
    const ks = $$Dict.keys(s.$$data);
    for (let i = 0; i < ks.length; i++) {
      if (!$$Dict.has(o.$$data, ks[i])) return false;
    }
    return true;
  },
  is_superset(s: SetValue, o: SetValue): boolean {
    const ko = $$Dict.keys(o.$$data);
    for (let i = 0; i < ko.length; i++) {
      if (!$$Dict.has(s.$$data, ko[i])) return false;
    }
    return true;
  },
  difference(s: SetValue, o: SetValue): SetValue {
    const ks = $$Dict.keys(s.$$data);
    let d = s.$$data;
    for (let i = 0; i < ks.length; i++) {
      if ($$Dict.has(o.$$data, ks[i])) d = $$Dict.remove(d, ks[i]);
    }
    return wrap(d);
  },
  intersection(s: SetValue, o: SetValue): SetValue {
    let smaller: SetValue, larger: SetValue;
    if ($$Dict.len(s.$$data) <= $$Dict.len(o.$$data)) {
      smaller = s;
      larger = o;
    } else {
      smaller = o;
      larger = s;
    }
    const ks = $$Dict.keys(smaller.$$data);
    let d = $$Dict.empty();
    for (let i = 0; i < ks.length; i++) {
      if ($$Dict.has(larger.$$data, ks[i]))
        d = $$Dict.insert(d, ks[i], SENTINEL);
    }
    return wrap(d);
  },
  union(s: SetValue, o: SetValue): SetValue {
    const ko = $$Dict.keys(o.$$data);
    let d = s.$$data;
    for (let i = 0; i < ko.length; i++) {
      d = $$Dict.insert(d, ko[i], SENTINEL);
    }
    return wrap(d);
  },
  from(items: unknown[]): SetValue {
    let d = $$Dict.empty();
    for (let i = 0; i < items.length; i++) {
      d = $$Dict.insert(d, items[i], SENTINEL);
    }
    return wrap(d);
  },
};
