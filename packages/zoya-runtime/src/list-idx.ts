export function $$list_idx(arr: unknown[], i: number): { $tag: "Some"; $0: unknown } | { $tag: "None" } {
  const idx = i < 0 ? arr.length + i : i;
  return idx >= 0 && idx < arr.length ? { $tag: "Some", $0: arr[idx] } : { $tag: "None" };
}
