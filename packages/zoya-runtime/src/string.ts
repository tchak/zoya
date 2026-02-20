export const $$String = {
  len(s: string): number {
    return s.length;
  },
  contains(s: string, needle: string): boolean {
    return s.includes(needle);
  },
  starts_with(s: string, prefix: string): boolean {
    return s.startsWith(prefix);
  },
  ends_with(s: string, suffix: string): boolean {
    return s.endsWith(suffix);
  },
  to_uppercase(s: string): string {
    return s.toUpperCase();
  },
  to_lowercase(s: string): string {
    return s.toLowerCase();
  },
  trim(s: string): string {
    return s.trim();
  },
  trim_start(s: string): string {
    return s.trimStart();
  },
  trim_end(s: string): string {
    return s.trimEnd();
  },
  replace(s: string, from: string, to: string): string {
    return s.replaceAll(from, to);
  },
  repeat(s: string, n: number): string {
    return s.repeat(n);
  },
  split(s: string, sep: string): string[] {
    return s.split(sep);
  },
  chars(s: string): string[] {
    return Array.from(s);
  },
  find(
    s: string,
    needle: string,
  ): { $tag: 'Some'; $0: number } | { $tag: 'None' } {
    const i = s.indexOf(needle);
    return i < 0 ? { $tag: 'None' } : { $tag: 'Some', $0: i };
  },
  slice(s: string, start: number, end: number): string {
    return s.slice(start, end);
  },
  reverse(s: string): string {
    return [...s].reverse().join('');
  },
  replace_first(s: string, from: string, to: string): string {
    return s.replace(from, to);
  },
  pad_start(s: string, len: number, fill: string): string {
    return s.padStart(len, fill);
  },
  pad_end(s: string, len: number, fill: string): string {
    return s.padEnd(len, fill);
  },
  to_int(s: string): { $tag: 'Some'; $0: number } | { $tag: 'None' } {
    const n = parseInt(s, 10);
    return isNaN(n) ? { $tag: 'None' } : { $tag: 'Some', $0: n };
  },
  to_float(s: string): { $tag: 'Some'; $0: number } | { $tag: 'None' } {
    const n = parseFloat(s);
    return isNaN(n) ? { $tag: 'None' } : { $tag: 'Some', $0: n };
  },
};
