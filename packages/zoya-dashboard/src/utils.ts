/** Group items by their `module` field, sorted with root (empty module) first, then alphabetically. */
export function groupByModule<T extends { module: string }>(
  items: T[],
  sortFn: (a: T, b: T) => number,
): [string, T[]][] {
  const sorted = [...items].sort((a, b) => {
    const ma = a.module;
    const mb = b.module;
    if (ma === '' && mb !== '') return -1;
    if (ma !== '' && mb === '') return 1;
    const cmp = ma.localeCompare(mb);
    return cmp !== 0 ? cmp : sortFn(a, b);
  });

  const groups: [string, T[]][] = [];
  for (const item of sorted) {
    const last = groups[groups.length - 1];
    if (last && last[0] === item.module) {
      last[1].push(item);
    } else {
      groups.push([item.module, [item]]);
    }
  }
  return groups;
}
