export const $$List = {
  len(xs: unknown[]): number {
    return xs.length;
  },
  reverse(xs: unknown[]): unknown[] {
    return [...xs].reverse();
  },
  push(xs: unknown[], item: unknown): unknown[] {
    return [...xs, item];
  },
  map(xs: unknown[], f: (x: unknown) => unknown): unknown[] {
    return xs.map(f);
  },
  filter(xs: unknown[], f: (x: unknown) => boolean): unknown[] {
    return xs.filter(f);
  },
  fold(
    xs: unknown[],
    init: unknown,
    f: (acc: unknown, x: unknown) => unknown,
  ): unknown {
    return xs.reduce(f, init);
  },
  filter_map(
    xs: unknown[],
    f: (x: unknown) => { $tag: string; $0?: unknown },
  ): unknown[] {
    const r: unknown[] = [];
    for (let i = 0; i < xs.length; i++) {
      const v = f(xs[i]);
      if (v.$tag === 'Some') r.push(v.$0);
    }
    return r;
  },
  truncate(xs: unknown[], len: number): unknown[] {
    return xs.slice(0, len);
  },
  insert(xs: unknown[], index: number, value: unknown): unknown[] {
    return [...xs.slice(0, index), value, ...xs.slice(index)];
  },
  remove(xs: unknown[], index: number): unknown[] {
    return [...xs.slice(0, index), ...xs.slice(index + 1)];
  },
};
